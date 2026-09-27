//! What a session knows while it is running: what is queued, what is running, what is done.
//!
//! Section 9 of the contract. A session reads its input on one thread and runs the calls on
//! another, so that `solar.cancel` can be answered while a call is still running. Everything
//! the two threads share lives here, behind **one** lock, which is what makes the promise of
//! section 9.3 provable: a cancellation finds a call either waiting or running, never both
//! and never neither.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use crate::cancel::{CancelOutcome, Cancellation};
use crate::clock;
use crate::error::{ErrorDetail, SolarError};
use crate::meta::Meta;
use crate::protocol::{Request, RequestError, RequestId, Response};
use crate::reason::Reason;
use crate::status::Status;

/// The method that is answered the moment it is read, rather than in its turn.
pub const CANCEL_METHOD: &str = "solar.cancel";

/// How many unanswered requests one session may hold.
pub const MAX_QUEUED_REQUESTS: usize = 256;

/// How many bytes of unanswered request text one session may hold.
pub const MAX_QUEUED_BYTES: usize = 64 * 1024 * 1024;

/// How many answered identifiers a session remembers, so it can say `already_finished`.
pub const REMEMBERED_IDS: usize = 1024;

/// One message waiting its turn, already read and already understood.
#[derive(Debug)]
pub enum Work {
    /// One request, which is the ordinary case.
    One(Request),
    /// A batch, answered with one array in the order of its elements.
    Batch(Vec<Result<Request, Box<RequestError>>>),
    /// A line that did not become a request, answered in its turn so that order holds.
    Failed(Box<RequestError>),
}

impl Work {
    /// Every identifier this message will answer with, which is what makes an id in flight.
    #[must_use]
    pub fn ids(&self) -> Vec<RequestId> {
        match self {
            Work::One(request) => vec![request.id.clone()],
            Work::Batch(elements) => elements
                .iter()
                .filter_map(|element| element.as_ref().ok().map(|request| request.id.clone()))
                .collect(),
            Work::Failed(failed) => failed.id.clone().into_iter().collect(),
        }
    }

    /// How many calls this message holds, which is what a session counts.
    #[must_use]
    pub fn calls(&self) -> usize {
        match self {
            Work::One(_) | Work::Failed(_) => 1,
            Work::Batch(elements) => elements.len(),
        }
    }
}

/// One message in the queue, with what its response needs to know.
#[derive(Debug)]
pub struct Queued {
    /// The message itself.
    pub work: Work,
    /// When it was read, for `meta.started_at`.
    pub started_at: String,
    /// When it was read, for `meta.duration_us`: waiting is part of the duration.
    pub start: Instant,
    /// How many bytes of input it holds, for the byte bound of section 9.6.
    bytes: usize,
}

/// The message that is running now, and the token its calls watch.
///
/// A batch is one message, so it runs under every identifier it carries: cancelling any
/// one of them asks the whole message to stop, and each element that has not run yet
/// answers `CANCELLED`.
#[derive(Debug)]
struct Running {
    ids: Vec<RequestId>,
    token: Cancellation,
}

/// Why a message could not be admitted to the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The queue holds as much as it may hold, by count or by bytes.
    QueueFull,
    /// An identifier of the message belongs to a call that has not been answered yet.
    IdInFlight,
}

/// Everything the reading thread and the running thread share.
#[derive(Debug, Default)]
struct Inner {
    queue: VecDeque<Queued>,
    queued_bytes: usize,
    in_flight: Vec<RequestId>,
    running: Option<Running>,
    finished: VecDeque<RequestId>,
    /// Responses somebody else owes the output: a call cancelled while it was queued.
    pending: Vec<Response>,
    /// Whether the input has ended, so the running thread knows to stop waiting.
    closed: bool,
}

/// The state of one session, shared by the thread that reads and the thread that runs.
#[derive(Debug, Default)]
pub struct SessionState {
    inner: Mutex<Inner>,
    /// Signals the running thread that there is work, or that there will never be again.
    work: Condvar,
}

impl SessionState {
    /// A session that has read nothing yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether an identifier belongs to a call this session has not answered yet.
    #[must_use]
    pub fn is_in_flight(&self, id: &RequestId) -> bool {
        let inner = self.lock();
        inner.in_flight.contains(id)
            || inner
                .running
                .as_ref()
                .is_some_and(|running| running.ids.contains(id))
    }

    /// Puts a message in the queue, or gives it back with the reason it could not go there.
    ///
    /// # Errors
    ///
    /// [`Refusal::QueueFull`] when the queue holds as much as section 9.6 allows, and
    /// [`Refusal::IdInFlight`] when an identifier of the message names a call that has
    /// not been answered yet, which section 9.5 refuses.
    pub fn admit(
        &self,
        work: Work,
        bytes: usize,
        started_at: String,
        start: Instant,
    ) -> Result<(), (Refusal, Work)> {
        let mut inner = self.lock();

        if inner.queue.len() >= MAX_QUEUED_REQUESTS
            || inner.queued_bytes.saturating_add(bytes) > MAX_QUEUED_BYTES
        {
            return Err((Refusal::QueueFull, work));
        }

        let ids = work.ids();
        if ids.iter().any(|id| {
            inner.in_flight.contains(id)
                || inner
                    .running
                    .as_ref()
                    .is_some_and(|running| running.ids.contains(id))
        }) {
            return Err((Refusal::IdInFlight, work));
        }

        inner.in_flight.extend(ids);
        inner.queued_bytes += bytes;
        inner.queue.push_back(Queued {
            work,
            started_at,
            start,
            bytes,
        });
        drop(inner);
        self.work.notify_one();
        Ok(())
    }

    /// How full the queue is: messages waiting, and bytes of request text.
    #[must_use]
    pub fn load(&self) -> (usize, usize) {
        let inner = self.lock();
        (inner.queue.len(), inner.queued_bytes)
    }

    /// Waits for the next message, and marks it running.
    ///
    /// Returns `None` once the input has ended and the queue is empty, which is the only
    /// way a session finishes.
    #[must_use]
    pub fn take(&self) -> Option<(Queued, Cancellation)> {
        let mut inner = self.lock();
        loop {
            if let Some(queued) = inner.queue.pop_front() {
                inner.queued_bytes = inner.queued_bytes.saturating_sub(queued.bytes);
                let token = Cancellation::new();
                inner.running = Some(Running {
                    ids: queued.work.ids(),
                    token: token.clone(),
                });
                return Some((queued, token));
            }
            if inner.closed {
                return None;
            }
            inner = self
                .work
                .wait(inner)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Records that a message has been answered, so its identifiers are free again.
    pub fn finish(&self, ids: &[RequestId]) {
        let mut inner = self.lock();
        inner.running = None;
        for id in ids {
            inner.in_flight.retain(|waiting| waiting != id);
            inner.finished.push_back(id.clone());
        }
        while inner.finished.len() > REMEMBERED_IDS {
            inner.finished.pop_front();
        }
    }

    /// Cancels the call with this identifier, and says what that did.
    ///
    /// A call that was still queued is removed and answered at once with `CANCELLED`; its
    /// response waits in [`SessionState::take_pending`] for whoever owns the output.
    pub fn cancel(&self, id: &RequestId) -> CancelOutcome {
        let mut inner = self.lock();

        if let Some(running) = inner.running.as_ref()
            && running.ids.contains(id)
        {
            running.token.cancel();
            return CancelOutcome::CancellationRequested;
        }

        if let Some(index) = inner
            .queue
            .iter()
            .position(|queued| queued.work.ids().contains(id))
        {
            let Some(queued) = inner.queue.remove(index) else {
                // `position` just found it, so this cannot happen. Saying `unknown` is
                // the honest answer, since nothing was cancelled.
                return CancelOutcome::Unknown;
            };
            inner.queued_bytes = inner.queued_bytes.saturating_sub(queued.bytes);
            for cancelled in queued.work.ids() {
                inner.in_flight.retain(|waiting| *waiting != cancelled);
                inner.finished.push_back(cancelled.clone());
                inner
                    .pending
                    .push(cancelled_response(&cancelled, &queued.started_at));
            }
            while inner.finished.len() > REMEMBERED_IDS {
                inner.finished.pop_front();
            }
            return CancelOutcome::CancelledWhileQueued;
        }

        if inner.finished.contains(id) {
            return CancelOutcome::AlreadyFinished;
        }

        CancelOutcome::Unknown
    }

    /// The responses a cancellation produced, which the session still owes the output.
    #[must_use]
    pub fn take_pending(&self) -> Vec<Response> {
        std::mem::take(&mut self.lock().pending)
    }

    /// Says that the input has ended. The queue is still answered to its end.
    pub fn close(&self) {
        self.lock().closed = true;
        self.work.notify_all();
    }

    /// Whether the input has ended, which is how the reading thread knows to stop.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.lock().closed
    }
}

/// The response a call cancelled before it started is answered with.
fn cancelled_response(id: &RequestId, started_at: &str) -> Response {
    let meta = Meta::new(Some(id.clone()), None, None, started_at.to_owned(), 0);
    Response::failure(
        Some(id.clone()),
        Cancellation::as_error("a call cancelled before it started"),
        meta,
    )
}

/// The error a message that arrives at a full queue is refused with.
#[must_use]
pub fn queue_full_error(messages: usize, bytes: usize) -> SolarError {
    SolarError::new(
        Reason::QueueFull,
        format!(
            "The session is already holding {messages} unanswered requests, {bytes} bytes of them."
        ),
    )
    .with_detail(
        ErrorDetail::new(Status::ResourceExhausted)
            .expected(format!(
                "at most {MAX_QUEUED_REQUESTS} unanswered requests and {MAX_QUEUED_BYTES} bytes"
            ))
            .received(format!("{messages} requests, {bytes} bytes"))
            .hint(
                "Read the responses that are waiting. The session keeps reading, and \
                 solar.cancel is still answered while the queue is full.",
            ),
    )
}

/// The error a message whose identifier is already alive is refused with.
#[must_use]
pub fn id_in_flight_error(id: &RequestId) -> SolarError {
    SolarError::new(
        Reason::IdInFlight,
        format!("The id {id} belongs to a call this session has not answered yet."),
    )
    .with_detail(
        ErrorDetail::new(Status::InvalidArgument)
            .field("id")
            .expected("an id no unanswered call is using")
            .received(serde_json::to_value(id).unwrap_or(serde_json::Value::Null))
            .hint(
                "Cancellation targets a call by its id, so an id names one call at a \
                 time. Wait for the response, or use another id.",
            ),
    )
}

/// The response a refused message is answered with, in the shape the wire expects.
#[must_use]
pub fn refusal_response(refusal: Refusal, work: &Work, state: &SessionState) -> Response {
    let (messages, bytes) = state.load();
    let id = work.ids().first().cloned();
    let error = match refusal {
        Refusal::QueueFull => queue_full_error(messages, bytes),
        Refusal::IdInFlight => id
            .as_ref()
            .map_or_else(|| queue_full_error(messages, bytes), id_in_flight_error),
    };
    let method = match work {
        Work::One(request) => Some(request.method.clone()),
        Work::Batch(_) | Work::Failed(_) => None,
    };
    let meta = Meta::new(id.clone(), method, None, clock::now_rfc3339_micros(), 0);
    Response::failure(id, error, meta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::parse_request;

    fn id(number: i64) -> RequestId {
        RequestId::Number(number.into())
    }

    fn request(number: i64) -> Work {
        let line = format!("{{\"jsonrpc\":\"2.0\",\"id\":{number},\"method\":\"a.b\"}}");
        Work::One(parse_request(&line).expect("a well formed request"))
    }

    fn put(state: &SessionState, number: i64) -> Result<(), (Refusal, Work)> {
        state.admit(
            request(number),
            64,
            clock::now_rfc3339_micros(),
            Instant::now(),
        )
    }

    #[test]
    fn a_message_comes_back_in_the_order_it_went_in() {
        let state = SessionState::new();
        for number in 1..=5 {
            put(&state, number).expect("the queue has room");
        }
        state.close();

        let mut seen = Vec::new();
        while let Some((queued, _token)) = state.take() {
            seen.extend(queued.work.ids());
            state.finish(&queued.work.ids());
        }
        assert_eq!(seen, (1..=5).map(id).collect::<Vec<_>>());
    }

    #[test]
    fn taking_the_last_message_of_a_closed_session_ends_it() {
        let state = SessionState::new();
        put(&state, 1).expect("the queue has room");
        state.close();
        assert!(state.take().is_some());
        assert!(state.take().is_none(), "nothing more will ever arrive");
    }

    #[test]
    fn an_id_is_in_flight_while_it_waits_and_while_it_runs() {
        let state = SessionState::new();
        put(&state, 7).expect("the queue has room");
        assert!(state.is_in_flight(&id(7)), "queued is in flight");

        let (queued, _token) = state.take().expect("the message is there");
        assert!(state.is_in_flight(&id(7)), "running is in flight too");

        state.finish(&queued.work.ids());
        assert!(!state.is_in_flight(&id(7)), "answered is free again");
    }

    #[test]
    fn the_same_id_twice_is_refused_while_the_first_is_alive() {
        let state = SessionState::new();
        put(&state, 1).expect("the queue has room");
        let (refusal, work) = put(&state, 1).expect_err("the id is in flight");
        assert_eq!(refusal, Refusal::IdInFlight);
        assert_eq!(work.ids(), vec![id(1)]);

        // Answered, the id is usable again.
        let (queued, _token) = state.take().expect("the message is there");
        state.finish(&queued.work.ids());
        put(&state, 1).expect("the id is free again");
    }

    #[test]
    fn the_queue_refuses_more_than_it_may_hold() {
        let state = SessionState::new();
        for number in 0..i64::try_from(MAX_QUEUED_REQUESTS).unwrap() {
            put(&state, number).expect("the queue has room");
        }
        let (refusal, _work) = put(&state, 9_999).expect_err("the queue is full");
        assert_eq!(refusal, Refusal::QueueFull);
        assert_eq!(state.load().0, MAX_QUEUED_REQUESTS);
    }

    #[test]
    fn the_queue_refuses_more_bytes_than_it_may_hold() {
        let state = SessionState::new();
        state
            .admit(
                request(1),
                MAX_QUEUED_BYTES,
                clock::now_rfc3339_micros(),
                Instant::now(),
            )
            .expect("the first message fits exactly");
        assert_eq!(state.load(), (1, MAX_QUEUED_BYTES));

        let (refusal, _work) = state
            .admit(request(2), 1, clock::now_rfc3339_micros(), Instant::now())
            .expect_err("one byte more than the bound");
        assert_eq!(refusal, Refusal::QueueFull);
    }

    #[test]
    fn taking_a_message_gives_its_bytes_back_to_the_queue() {
        let state = SessionState::new();
        put(&state, 1).expect("the queue has room");
        assert_eq!(state.load(), (1, 64));
        let _ = state.take().expect("the message is there");
        assert_eq!(state.load(), (0, 0), "a message that left is not held");
    }

    #[test]
    fn cancelling_a_queued_call_removes_it_and_answers_it() {
        let state = SessionState::new();
        put(&state, 1).expect("the queue has room");
        put(&state, 2).expect("the queue has room");

        assert_eq!(state.cancel(&id(2)), CancelOutcome::CancelledWhileQueued);
        assert_eq!(state.load().0, 1, "the cancelled message is gone");

        let pending = state.take_pending();
        assert_eq!(pending.len(), 1, "a cancelled call is answered at once");
        assert!(pending[0].to_line().contains("CALL_CANCELLED"));
        assert!(state.take_pending().is_empty(), "taken once, not twice");

        // What is left is the other message, untouched.
        let (queued, _token) = state.take().expect("the message is there");
        assert_eq!(queued.work.ids(), vec![id(1)]);
    }

    #[test]
    fn cancelling_a_running_call_sets_its_token() {
        let state = SessionState::new();
        put(&state, 1).expect("the queue has room");
        let (_queued, token) = state.take().expect("the message is there");
        assert!(!token.is_cancelled());

        assert_eq!(state.cancel(&id(1)), CancelOutcome::CancellationRequested);
        assert!(token.is_cancelled(), "the handler is told");
        assert!(
            state.take_pending().is_empty(),
            "a running call answers for itself"
        );
    }

    #[test]
    fn cancelling_a_call_that_finished_says_so() {
        let state = SessionState::new();
        put(&state, 1).expect("the queue has room");
        let (queued, _token) = state.take().expect("the message is there");
        state.finish(&queued.work.ids());
        assert_eq!(state.cancel(&id(1)), CancelOutcome::AlreadyFinished);
    }

    #[test]
    fn cancelling_an_id_nothing_is_using_says_so() {
        let state = SessionState::new();
        assert_eq!(state.cancel(&id(404)), CancelOutcome::Unknown);
    }

    #[test]
    fn the_limits_are_the_numbers_the_contract_states() {
        // Written as plain numbers, because what matters is that they are the values
        // section 9.6 and 9.7 declare, not that an expression equals itself.
        assert_eq!(MAX_QUEUED_REQUESTS, 256);
        assert_eq!(MAX_QUEUED_BYTES, 67_108_864);
        assert_eq!(REMEMBERED_IDS, 1_024);
    }

    #[test]
    fn cancelling_one_queued_message_leaves_the_others_in_flight() {
        let state = SessionState::new();
        for number in 1..=4 {
            put(&state, number).expect("the queue has room");
        }

        assert_eq!(state.cancel(&id(2)), CancelOutcome::CancelledWhileQueued);

        assert!(!state.is_in_flight(&id(2)), "the cancelled one is free");
        for still_waiting in [1, 3, 4] {
            assert!(
                state.is_in_flight(&id(still_waiting)),
                "id {still_waiting} is still waiting its turn and is still in flight"
            );
        }
        assert_eq!(state.load().0, 3);
    }

    #[test]
    fn cancelling_keeps_the_window_of_remembered_identifiers_bounded() {
        // The same bound as `finish`, on the other path that adds to it: a session that
        // cancels a great many queued calls must not grow either.
        let state = SessionState::new();
        let first = id(0);
        for number in 0..i64::try_from(REMEMBERED_IDS).unwrap() {
            put(&state, number).expect("the queue has room");
            assert_eq!(
                state.cancel(&id(number)),
                CancelOutcome::CancelledWhileQueued
            );
        }
        assert_eq!(state.cancel(&first), CancelOutcome::AlreadyFinished);

        put(&state, 5_000).expect("the queue has room");
        assert_eq!(
            state.cancel(&id(5_000)),
            CancelOutcome::CancelledWhileQueued
        );
        assert_eq!(
            state.cancel(&first),
            CancelOutcome::Unknown,
            "the oldest identifier has left the window"
        );
    }

    #[test]
    fn a_session_remembers_only_the_most_recent_identifiers() {
        let state = SessionState::new();
        let first = id(0);
        for number in 0..i64::try_from(REMEMBERED_IDS).unwrap() {
            state.finish(&[id(number)]);
        }
        assert_eq!(
            state.cancel(&first),
            CancelOutcome::AlreadyFinished,
            "the window is still holding the oldest one"
        );

        // One more pushes the oldest out, and an id older than the window is unknown.
        state.finish(&[id(1_000_000)]);
        assert_eq!(state.cancel(&first), CancelOutcome::Unknown);
        assert_eq!(state.cancel(&id(1_000_000)), CancelOutcome::AlreadyFinished);
    }

    #[test]
    fn a_batch_runs_under_every_id_it_carries() {
        let state = SessionState::new();
        let line =
            r#"[{"jsonrpc":"2.0","id":1,"method":"a.b"},{"jsonrpc":"2.0","id":2,"method":"a.b"}]"#;
        let crate::protocol::Incoming::Batch(elements) = crate::protocol::read_line(line) else {
            panic!("an array is a batch");
        };
        let work = Work::Batch(elements);
        assert_eq!(work.calls(), 2);
        state
            .admit(work, 96, clock::now_rfc3339_micros(), Instant::now())
            .expect("the queue has room");

        assert!(state.is_in_flight(&id(1)));
        assert!(state.is_in_flight(&id(2)));

        let (_queued, token) = state.take().expect("the message is there");
        // Cancelling any id the batch carries asks the whole message to stop.
        assert_eq!(state.cancel(&id(2)), CancelOutcome::CancellationRequested);
        assert!(token.is_cancelled());
    }

    #[test]
    fn a_line_that_never_parsed_still_waits_its_turn() {
        let state = SessionState::new();
        let failed = parse_request("{not json").expect_err("this line does not parse");
        let work = Work::Failed(Box::new(failed));
        assert_eq!(work.calls(), 1);
        assert!(work.ids().is_empty(), "a line with no id claims no id");
        state
            .admit(work, 9, clock::now_rfc3339_micros(), Instant::now())
            .expect("the queue has room");
        state.close();
        assert!(state.take().is_some(), "it is answered in its turn");
    }

    #[test]
    fn a_refusal_says_which_limit_was_reached() {
        let state = SessionState::new();
        let queue_full = refusal_response(Refusal::QueueFull, &request(1), &state);
        let line = queue_full.to_line();
        assert!(line.contains("QUEUE_FULL"), "{line}");
        assert!(line.contains(&MAX_QUEUED_REQUESTS.to_string()), "{line}");

        let in_flight = refusal_response(Refusal::IdInFlight, &request(1), &state);
        let line = in_flight.to_line();
        assert!(line.contains("ID_IN_FLIGHT"), "{line}");
        assert!(line.contains("\"id\":1"), "{line}");
    }

    #[test]
    fn a_session_is_closed_only_once_it_is_told_to_be() {
        let state = SessionState::new();
        assert!(!state.is_closed());
        state.close();
        assert!(state.is_closed());
    }
}
