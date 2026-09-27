//! Section 9 of the contract, against a session that is really running.
//!
//! The promise being tested is the one everything else rests on: **every request gets
//! exactly one response**, its result or `CANCELLED`, never both and never neither,
//! however the cancellation and the call are interleaved. The races are driven on purpose
//! here, with handlers that block until the test lets them go, so the interleaving is the
//! test rather than an accident of timing.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test reports failure by panicking; integration tests are their own crate"
)]

use std::io::{BufRead, Read};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use solar_core::api::{Api, ApiSpec, DEFAULT_MAX_OUTPUT_BYTES, Example, SideEffect, Stability};
use solar_core::context::Context;
use solar_core::dispatch::Dispatcher;
use solar_core::error::SolarError;
use solar_core::registry::RegistryBuilder;
use solar_core::server::serve;
use solar_core::session::MAX_QUEUED_REQUESTS;

// ---------------------------------------------------------------------------------------
// A reader the test writes to while the session is running.
// ---------------------------------------------------------------------------------------

/// Input a test feeds line by line, so a session can be driven while it is running.
///
/// Reading blocks until the test sends the next line or closes the sender, which is what
/// standard input does and what a `&[u8]` cannot do.
struct Keyboard {
    lines: Receiver<String>,
    buffer: Vec<u8>,
    at: usize,
}

impl Keyboard {
    fn new() -> (Sender<String>, Keyboard) {
        let (sender, lines) = channel();
        (
            sender,
            Keyboard {
                lines,
                buffer: Vec::new(),
                at: 0,
            },
        )
    }
}

impl Read for Keyboard {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let available = self.fill_buf()?.len();
        let taken = available.min(out.len());
        out[..taken].copy_from_slice(&self.buffer[self.at..self.at + taken]);
        self.consume(taken);
        Ok(taken)
    }
}

impl BufRead for Keyboard {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if self.at == self.buffer.len() {
            match self.lines.recv() {
                Ok(line) => {
                    self.buffer = line.into_bytes();
                    self.at = 0;
                }
                // Every sender is gone, which is the end of the input.
                Err(_) => return Ok(&[]),
            }
        }
        Ok(&self.buffer[self.at..])
    }

    fn consume(&mut self, amount: usize) {
        self.at = (self.at + amount).min(self.buffer.len());
    }
}

/// Output the test can read while the session is still writing to it.
#[derive(Clone, Default)]
struct Tape(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Tape {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Tape {
    /// Every whole line written so far, as JSON.
    fn lines(&self) -> Vec<Value> {
        let bytes = self.0.lock().unwrap().clone();
        String::from_utf8(bytes)
            .expect("SOLAR writes UTF-8")
            .lines()
            .map(|line| serde_json::from_str(line).expect("a response is JSON"))
            .collect()
    }

    /// Waits until `count` lines have been written, or gives up after two seconds.
    fn wait_for(&self, count: usize) -> Vec<Value> {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            let lines = self.lines();
            if lines.len() >= count {
                return lines;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!(
            "waited two seconds for {count} lines and saw {}",
            self.lines().len()
        );
    }
}

// ---------------------------------------------------------------------------------------
// Handlers that block until the test lets them go.
// ---------------------------------------------------------------------------------------

/// Lets a test hold a handler inside the session and release it when it chooses.
static GATE: Mutex<Option<Sender<()>>> = Mutex::new(None);
static RELEASE: Mutex<Option<Receiver<()>>> = Mutex::new(None);
/// The gate is one pair of channels for the whole test binary, so the tests that use it
/// take their turn. Tests run in parallel, and two of them sharing a gate would be two
/// tests watching each other rather than the session.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// Opens a gate: the handler waits at it, and the test is told when it gets there.
///
/// The guard that comes back keeps the other gate tests out until it is dropped.
fn gate() -> (Receiver<()>, Sender<()>, std::sync::MutexGuard<'static, ()>) {
    let turn = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (arrived_sender, arrived) = channel();
    let (release, waiting) = channel();
    *GATE.lock().unwrap() = Some(arrived_sender);
    *RELEASE.lock().unwrap() = Some(waiting);
    (arrived, release, turn)
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NoParams {}

#[derive(Debug, Serialize, JsonSchema)]
struct Done {
    done: bool,
}

fn spec(summary: &'static str, timeout_ms: u64) -> ApiSpec {
    ApiSpec {
        summary,
        description: "An API that exists only inside the cancellation tests, to hold a \
                      session still while the test drives the race it wants.",
        errors: Vec::new(),
        side_effects: vec![SideEffect::None],
        idempotent: true,
        stability: Stability::Experimental,
        since: "0.1.0",
        timeout_ms,
        max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
        examples: vec![Example::exact(
            "plain",
            "The only call",
            json!({}),
            json!({"done": true}),
        )],
    }
}

/// Waits at the gate, and checks its token when it is let go.
struct Waits;

impl Api for Waits {
    const NAME: &'static str = "test.waits";
    const VERSION: &'static str = "1.0.0";
    type Params = NoParams;
    type Output = Done;

    fn spec() -> ApiSpec {
        spec("Waits at the gate the test holds", 5_000)
    }

    fn call(ctx: &Context, _params: NoParams) -> Result<Done, SolarError> {
        if let Some(arrived) = GATE.lock().unwrap().as_ref() {
            let _ = arrived.send(());
        }
        // Held here until the test releases it, or until two seconds pass, so that a test
        // that fails leaves no thread behind.
        let waiting = RELEASE.lock().unwrap().take();
        if let Some(waiting) = waiting {
            let _ = waiting.recv_timeout(Duration::from_secs(2));
        }
        if ctx.is_cancelled() {
            return Err(ctx.cancelled());
        }
        Ok(Done { done: true })
    }
}

/// Answers at once.
struct Quick;

impl Api for Quick {
    const NAME: &'static str = "test.quick";
    const VERSION: &'static str = "1.0.0";
    type Params = NoParams;
    type Output = Done;

    fn spec() -> ApiSpec {
        spec("Answers at once", 1_000)
    }

    fn call(_ctx: &Context, _params: NoParams) -> Result<Done, SolarError> {
        Ok(Done { done: true })
    }
}

fn dispatcher() -> Dispatcher {
    let registry = RegistryBuilder::new()
        .register::<solar_apis::solar_cancel::SolarCancel>()
        .register::<Quick>()
        .register::<Waits>()
        .build()
        .expect("the test registry must be valid");
    Dispatcher::new(Arc::new(registry))
}

fn request(id: i64, method: &str) -> String {
    format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"method\":\"{method}\"}}\n")
}

fn cancel(id: i64, target: i64) -> String {
    format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"method\":\"solar.cancel\",\"params\":{{\"id\":{target}}}}}\n"
    )
}

/// The response with this id, whichever line it came out on.
fn answer(lines: &[Value], id: i64) -> Value {
    lines
        .iter()
        .find(|line| line["id"] == json!(id))
        .unwrap_or_else(|| panic!("nothing answered id {id}: {lines:?}"))
        .clone()
}

// ---------------------------------------------------------------------------------------
// The promises.
// ---------------------------------------------------------------------------------------

#[test]
fn a_queued_call_is_cancelled_before_it_starts() {
    let (arrived, release, _turn) = gate();
    let (keys, input) = Keyboard::new();
    let tape = Tape::default();
    let writing = tape.clone();

    let session = std::thread::spawn(move || serve(input, writing, &dispatcher()).unwrap());

    // One call is held inside the session, so the next one can only be queued.
    keys.send(request(1, "test.waits")).unwrap();
    arrived
        .recv_timeout(Duration::from_secs(2))
        .expect("the handler reached the gate");
    keys.send(request(2, "test.quick")).unwrap();
    keys.send(cancel(3, 2)).unwrap();

    // The cancellation is answered while the first call is still running, which is the
    // whole point of section 9.1.
    let lines = tape.wait_for(2);
    let said = answer(&lines, 3);
    assert_eq!(said["result"]["data"]["outcome"], "cancelled_while_queued");

    let cancelled = answer(&lines, 2);
    assert_eq!(cancelled["error"]["data"]["status"], "CANCELLED");
    assert_eq!(cancelled["error"]["data"]["reason"], "CALL_CANCELLED");

    release.send(()).unwrap();
    drop(keys);
    let answered = session.join().unwrap();

    let lines = tape.lines();
    assert_eq!(answered, 3, "three requests, three responses");
    assert_eq!(lines.len(), 3);
    assert_eq!(answer(&lines, 1)["result"]["data"]["done"], json!(true));
    // Exactly one response for the cancelled call, and it is the CANCELLED one.
    assert_eq!(
        lines.iter().filter(|line| line["id"] == json!(2)).count(),
        1
    );
}

#[test]
fn a_running_call_is_told_and_answers_cancelled() {
    let (arrived, release, _turn) = gate();
    let (keys, input) = Keyboard::new();
    let tape = Tape::default();
    let writing = tape.clone();

    let session = std::thread::spawn(move || serve(input, writing, &dispatcher()).unwrap());

    keys.send(request(1, "test.waits")).unwrap();
    arrived
        .recv_timeout(Duration::from_secs(2))
        .expect("the handler reached the gate");
    keys.send(cancel(2, 1)).unwrap();

    let lines = tape.wait_for(1);
    assert_eq!(
        answer(&lines, 2)["result"]["data"]["outcome"],
        "cancellation_requested"
    );

    // The handler is let go, sees its token, and ends with CANCELLED rather than a result.
    release.send(()).unwrap();
    drop(keys);
    let answered = session.join().unwrap();

    let lines = tape.lines();
    assert_eq!(answered, 2);
    let cancelled = answer(&lines, 1);
    assert_eq!(cancelled["error"]["data"]["status"], "CANCELLED");
    assert_eq!(
        lines.iter().filter(|line| line["id"] == json!(1)).count(),
        1,
        "never both a result and a cancellation"
    );
}

#[test]
fn a_call_that_already_finished_is_reported_as_finished() {
    let (keys, input) = Keyboard::new();
    let tape = Tape::default();
    let writing = tape.clone();
    let session = std::thread::spawn(move || serve(input, writing, &dispatcher()).unwrap());

    keys.send(request(1, "test.quick")).unwrap();
    tape.wait_for(1);
    keys.send(cancel(2, 1)).unwrap();
    let lines = tape.wait_for(2);
    assert_eq!(
        answer(&lines, 2)["result"]["data"]["outcome"],
        "already_finished"
    );

    drop(keys);
    session.join().unwrap();
}

#[test]
fn an_id_nothing_is_using_is_unknown() {
    let (keys, input) = Keyboard::new();
    let tape = Tape::default();
    let writing = tape.clone();
    let session = std::thread::spawn(move || serve(input, writing, &dispatcher()).unwrap());

    keys.send(cancel(1, 9999)).unwrap();
    let lines = tape.wait_for(1);
    assert_eq!(answer(&lines, 1)["result"]["data"]["outcome"], "unknown");

    drop(keys);
    session.join().unwrap();
}

#[test]
fn an_id_that_is_still_in_flight_is_refused_for_a_new_call() {
    let (arrived, release, _turn) = gate();
    let (keys, input) = Keyboard::new();
    let tape = Tape::default();
    let writing = tape.clone();
    let session = std::thread::spawn(move || serve(input, writing, &dispatcher()).unwrap());

    keys.send(request(1, "test.waits")).unwrap();
    arrived
        .recv_timeout(Duration::from_secs(2))
        .expect("the handler reached the gate");
    // The same id again, while the first is still unanswered.
    keys.send(request(1, "test.quick")).unwrap();

    let lines = tape.wait_for(1);
    let refused = answer(&lines, 1);
    assert_eq!(refused["error"]["data"]["reason"], "ID_IN_FLIGHT");
    assert_eq!(refused["error"]["data"]["status"], "INVALID_ARGUMENT");

    release.send(()).unwrap();
    drop(keys);
    session.join().unwrap();
}

#[test]
fn a_client_that_never_cancels_sees_its_responses_in_order() {
    // The promise of section 9.1, which is what the reading thread must not break.
    let (keys, input) = Keyboard::new();
    let tape = Tape::default();
    let writing = tape.clone();
    let session = std::thread::spawn(move || serve(input, writing, &dispatcher()).unwrap());

    for id in 1..=40 {
        keys.send(request(id, "test.quick")).unwrap();
    }
    drop(keys);
    let answered = session.join().unwrap();
    assert_eq!(answered, 40);

    let ids: Vec<i64> = tape
        .lines()
        .iter()
        .map(|line| line["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, (1..=40).collect::<Vec<i64>>());
}

#[test]
fn every_request_gets_exactly_one_response_however_the_race_falls() {
    // The same race, run many times with no synchronisation at all between the call and
    // the cancellation: whichever wins, there is one response per request and no more.
    for round in 0..40i64 {
        let (keys, input) = Keyboard::new();
        let tape = Tape::default();
        let writing = tape.clone();
        let session = std::thread::spawn(move || serve(input, writing, &dispatcher()).unwrap());

        keys.send(request(1, "test.quick")).unwrap();
        keys.send(cancel(2, 1)).unwrap();
        drop(keys);
        let answered = session.join().unwrap();

        let lines = tape.lines();
        assert_eq!(answered, 2, "round {round}");
        assert_eq!(lines.len(), 2, "round {round}: {lines:?}");
        for id in [1, 2] {
            assert_eq!(
                lines.iter().filter(|line| line["id"] == json!(id)).count(),
                1,
                "round {round}: id {id} was answered more than once or not at all"
            );
        }
        // Whatever happened to the call, its response is a result or a cancellation.
        let call = answer(&lines, 1);
        let outcome = answer(&lines, 2)["result"]["data"]["outcome"]
            .as_str()
            .unwrap()
            .to_owned();
        match outcome.as_str() {
            "cancelled_while_queued" => {
                assert_eq!(
                    call["error"]["data"]["status"], "CANCELLED",
                    "round {round}"
                );
            }
            "already_finished" | "cancellation_requested" => {
                assert!(
                    call.get("result").is_some() || call["error"]["data"]["status"] == "CANCELLED",
                    "round {round}: {call}"
                );
            }
            other => panic!("round {round}: {other} is not an outcome of a queued call"),
        }
    }
}

#[test]
fn a_full_queue_refuses_new_requests_and_still_answers_a_cancellation() {
    // Section 9.6: the session keeps reading, and says at once that it cannot queue more.
    let (arrived, release, _turn) = gate();
    let (keys, input) = Keyboard::new();
    let tape = Tape::default();
    let writing = tape.clone();
    let session = std::thread::spawn(move || serve(input, writing, &dispatcher()).unwrap());

    // One call is held inside the session, so nothing else can leave the queue.
    keys.send(request(1, "test.waits")).unwrap();
    arrived
        .recv_timeout(Duration::from_secs(2))
        .expect("the handler reached the gate");

    let queued = i64::try_from(MAX_QUEUED_REQUESTS).unwrap();
    for id in 2..=queued + 1 {
        keys.send(request(id, "test.quick")).unwrap();
    }
    // One more than the queue may hold.
    keys.send(request(queued + 2, "test.quick")).unwrap();

    let lines = tape.wait_for(1);
    let refused = answer(&lines, queued + 2);
    assert_eq!(refused["error"]["data"]["status"], "RESOURCE_EXHAUSTED");
    assert_eq!(refused["error"]["data"]["reason"], "QUEUE_FULL");

    // A cancellation is answered even now, which is when it matters most.
    keys.send(cancel(queued + 3, 2)).unwrap();
    let lines = tape.wait_for(3);
    assert_eq!(
        answer(&lines, queued + 3)["result"]["data"]["outcome"],
        "cancelled_while_queued"
    );

    release.send(()).unwrap();
    drop(keys);
    let answered = session.join().unwrap();

    // Every request sent was answered exactly once: the held one, the queue, the refusal,
    // the cancelled one and the cancellation itself.
    let lines = tape.lines();
    assert_eq!(answered, u64::try_from(lines.len()).unwrap());
    assert_eq!(answered, u64::try_from(queued).unwrap() + 3);
    for id in 1..=queued + 3 {
        assert_eq!(
            lines.iter().filter(|line| line["id"] == json!(id)).count(),
            1,
            "id {id} was answered more than once or not at all"
        );
    }
}

#[test]
fn a_cancellation_inside_a_batch_waits_its_turn_like_any_other_element() {
    // Section 9.1: a batch is one message, and its elements run in order. Only a
    // `solar.cancel` sent on its own is answered where it is read.
    let (arrived, release, _turn) = gate();
    let (keys, input) = Keyboard::new();
    let tape = Tape::default();
    let writing = tape.clone();
    let session = std::thread::spawn(move || serve(input, writing, &dispatcher()).unwrap());

    keys.send(request(1, "test.waits")).unwrap();
    arrived
        .recv_timeout(Duration::from_secs(2))
        .expect("the handler reached the gate");

    // A batch holding a cancellation for the call that is running. It goes into the
    // queue, so nothing is answered while the first call is held.
    keys.send(
        "[{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"solar.cancel\",\"params\":{\"id\":1}}]\n"
            .to_owned(),
    )
    .unwrap();

    std::thread::sleep(Duration::from_millis(50));
    assert!(
        tape.lines().is_empty(),
        "a cancellation inside a batch waits its turn: {:?}",
        tape.lines()
    );

    release.send(()).unwrap();
    drop(keys);
    let answered = session.join().unwrap();

    let lines = tape.lines();
    assert_eq!(answered, 2);
    assert_eq!(lines.len(), 2, "one line for the call, one for the batch");
    // By the time the batch runs, the call it names has been answered.
    let batch = lines
        .iter()
        .find(|line| line.is_array())
        .expect("the batch answers with an array");
    assert_eq!(batch[0]["result"]["data"]["outcome"], "already_finished");
}
