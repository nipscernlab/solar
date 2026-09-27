//! The NDJSON session: read the input on one thread, run the calls on another.
//!
//! **Calls run one at a time, in the order they arrived**, so a client that never cancels
//! sees its responses in the order of its requests. A batch is one message: it answers
//! with one line holding the array of its responses, in the order of its elements, which
//! is section 3.2 of the contract.
//!
//! The reading is a thread of its own because of section 9: a session has to keep reading
//! while a call is running, or `solar.cancel` would wait behind the very call it is
//! cancelling. `solar.cancel` is the one message answered where it is read; everything
//! else goes through the queue, and the queue is bounded by section 9.6.

use std::io::{BufRead, Write};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use crate::cancel::Cancellation;
use crate::clock;
use crate::dispatch::{Dispatcher, InSession};
use crate::error::{ErrorDetail, SolarError};
use crate::logging;
use crate::meta::Meta;
use crate::protocol::{Incoming, MAX_REQUEST_BYTES, Request, Response, read_line};
use crate::reason::Reason;
use crate::recording::{Direction, Recorder};
use crate::session::{CANCEL_METHOD, SessionState, Work, refusal_response};
use crate::status::Status;

/// What one attempt at reading a line produced.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// A whole line, which may still be empty.
    Line,
    /// A line longer than the limit, already discarded up to its newline.
    TooLong(usize),
    /// The input ended.
    Eof,
}

/// Reads one line, refusing to buffer more than `limit` bytes of it.
///
/// Once the limit is passed, what was read is dropped and the rest of the line is consumed
/// without being kept, so that the next message is still read from the right place.
fn read_line_limited<R: BufRead>(
    reader: &mut R,
    line: &mut Vec<u8>,
    limit: usize,
) -> std::io::Result<Outcome> {
    line.clear();
    let mut overflowed = false;
    let mut total = 0usize;

    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            return Ok(if total == 0 {
                Outcome::Eof
            } else if overflowed {
                Outcome::TooLong(total)
            } else {
                Outcome::Line
            });
        }

        #[allow(
            clippy::single_match_else,
            reason = "the two arms are peers, found the newline or not; if let would bury one"
        )]
        match chunk.iter().position(|byte| *byte == b'\n') {
            Some(index) => {
                total += index;
                if !overflowed {
                    if line.len() + index > limit {
                        overflowed = true;
                        line.clear();
                    } else {
                        line.extend_from_slice(chunk.get(..index).unwrap_or_default());
                    }
                }
                reader.consume(index + 1);
                return Ok(if overflowed {
                    Outcome::TooLong(total)
                } else {
                    Outcome::Line
                });
            }
            None => {
                let length = chunk.len();
                total += length;
                if !overflowed {
                    if line.len() + length > limit {
                        overflowed = true;
                        line.clear();
                    } else {
                        line.extend_from_slice(chunk);
                    }
                }
                reader.consume(length);
            }
        }
    }
}

/// The error a line that is too long becomes.
fn too_large(bytes: usize) -> SolarError {
    SolarError::new(
        Reason::MessageTooLarge,
        format!("The request line is longer than the {MAX_REQUEST_BYTES} byte limit."),
    )
    .with_detail(
        ErrorDetail::new(Status::ResourceExhausted)
            .expected(format!("at most {MAX_REQUEST_BYTES} bytes on one line"))
            .received(bytes)
            .hint(
                "SOLAR stopped reading at the limit, so the id could not be recovered. Send \
                 smaller requests; a path to a file is smaller than the file.",
            ),
    )
}

/// The error a line that is not UTF-8 becomes.
fn not_utf8(position: usize) -> SolarError {
    SolarError::new(
        Reason::ParseError,
        format!("The request is not valid UTF-8, starting at byte {position}."),
    )
    .with_detail(
        ErrorDetail::new(Status::InvalidArgument)
            .expected("UTF-8")
            .received(position)
            .hint("Encode the request as UTF-8. JSON has no other encoding in solar/1."),
    )
}

/// Where a session writes, and what it writes down as it goes.
///
/// Both threads of a session write here, so it lives behind one lock: a response is one
/// line, and two half lines would not be a protocol.
struct Sink<W: Write, F: Write> {
    output: W,
    recorder: Option<Recorder<F>>,
}

impl<W: Write, F: Write> Sink<W, F> {
    /// Writes one line, flushes it, and records it as output.
    fn write(&mut self, line: &str) -> std::io::Result<()> {
        logging::trace(&format!("<-- {line}"));
        self.output.write_all(line.as_bytes())?;
        self.output.write_all(b"\n")?;
        self.output.flush()?;
        if let Some(recorder) = self.recorder.as_mut() {
            recorder.record(Direction::Out, line)?;
        }
        Ok(())
    }

    /// Records one line as input. Nothing is written to the output.
    fn record_in(&mut self, line: &str) -> std::io::Result<()> {
        if let Some(recorder) = self.recorder.as_mut() {
            recorder.record(Direction::In, line)?;
        }
        Ok(())
    }
}

/// Everything a session shares between the thread that reads and the thread that runs.
struct Wiring<'a, W: Write, F: Write> {
    state: Arc<SessionState>,
    sink: Mutex<Sink<W, F>>,
    dispatcher: &'a Dispatcher,
    answered: AtomicU64,
}

impl<W: Write, F: Write> Wiring<'_, W, F> {
    /// Writes one line and counts the calls it answered.
    fn answer(&self, line: &str, calls: usize) -> std::io::Result<()> {
        self.sink
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .write(line)?;
        self.answered
            .fetch_add(calls as u64, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    /// Writes the `CANCELLED` responses a cancellation produced, before its own answer.
    fn answer_the_cancelled(&self) -> std::io::Result<()> {
        for response in self.state.take_pending() {
            self.answer(&response.to_line(), 1)?;
        }
        Ok(())
    }
}

/// Runs a session until the input ends.
///
/// Every line that is a message produces exactly one response line, flushed immediately so
/// that a caller reading a pipe never waits for a buffer to fill. A line that is empty or
/// only whitespace is not a message and produces nothing. The count returned is of calls
/// answered, so a batch of five counts five while writing one line.
///
/// # Errors
///
/// Returns the first input or output failure. A failure here means the session itself broke,
/// which the command line interface reports as exit code 70.
pub fn serve<R: BufRead + Send, W: Write + Send>(
    input: R,
    output: W,
    dispatcher: &Dispatcher,
) -> std::io::Result<u64> {
    serve_recording(input, output, dispatcher, None::<&mut std::io::Sink>)
}

/// The same session, written down as it happens.
///
/// Every line in and every line out is appended to `recorder`, with the time it crossed,
/// which is what `solar serve --stdio --record` does and what `solar replay` reads.
///
/// # Errors
///
/// Returns the first input, output or recording failure. A session whose recording
/// cannot be written stops, because half a recording is worse than none.
pub fn serve_recording<R: BufRead + Send, W: Write + Send, F: Write + Send>(
    input: R,
    output: W,
    dispatcher: &Dispatcher,
    recorder: Option<&mut F>,
) -> std::io::Result<u64> {
    let mut recorder = recorder.map(Recorder::new);
    if let Some(recorder) = recorder.as_mut() {
        // The first line of a recording says what the file is, so that a reader knows
        // before it reads. `docs/RECORDING.md` is the specification.
        recorder.write_header()?;
    }

    let wiring = Wiring {
        state: Arc::new(SessionState::new()),
        sink: Mutex::new(Sink { output, recorder }),
        dispatcher,
        answered: AtomicU64::new(0),
    };

    // Two threads, because section 9 of the contract says a session keeps reading while a
    // call is running: otherwise `solar.cancel` would wait behind the call it cancels.
    // The calls themselves still run one at a time, in the order they arrived.
    let reading = std::thread::scope(|scope| {
        let reader = scope.spawn(|| {
            let outcome = read_into(input, &wiring);
            // Whatever happened, the running thread must stop waiting for more work.
            wiring.state.close();
            outcome
        });
        let running = run_from_queue(&wiring);
        let reading = reader.join().unwrap_or_else(|_| {
            Err(std::io::Error::other(
                "the thread reading the session panicked",
            ))
        });
        running.and(reading)
    });

    let answered = wiring.answered.load(std::sync::atomic::Ordering::SeqCst);
    reading?;
    logging::info(&format!("the session ended after {answered} calls"));
    Ok(answered)
}

/// Reads the input, answers what cannot wait, and queues the rest.
fn read_into<R: BufRead, W: Write, F: Write>(
    mut input: R,
    wiring: &Wiring<'_, W, F>,
) -> std::io::Result<()> {
    let mut buffer: Vec<u8> = Vec::with_capacity(8 * 1024);

    loop {
        match read_line_limited(&mut input, &mut buffer, MAX_REQUEST_BYTES)? {
            Outcome::Eof => return Ok(()),
            Outcome::TooLong(bytes) => {
                logging::warn(&format!("a request of {bytes} bytes was refused"));
                wiring.answer(&bare_failure(too_large(bytes)).to_line(), 1)?;
            }
            Outcome::Line => {
                let line = match std::str::from_utf8(&buffer) {
                    Ok(text) => text.trim_end_matches('\r'),
                    Err(broken) => {
                        let response = bare_failure(not_utf8(broken.valid_up_to()));
                        wiring.answer(&response.to_line(), 1)?;
                        continue;
                    }
                };

                if line.trim().is_empty() {
                    continue;
                }

                logging::trace(&format!("--> {line}"));
                wiring
                    .sink
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .record_in(line)?;
                accept(line, wiring)?;
            }
        }
    }
}

/// Answers a line that cannot wait, or puts it in the queue.
fn accept<W: Write, F: Write>(line: &str, wiring: &Wiring<'_, W, F>) -> std::io::Result<()> {
    let started_at = clock::now_rfc3339_micros();
    let start = Instant::now();

    let work = match read_line(line) {
        Incoming::One(Ok(request)) if request.method == CANCEL_METHOD => {
            return cancel_now(request, started_at, start, wiring);
        }
        Incoming::One(Ok(request)) => Work::One(request),
        Incoming::One(Err(failed)) | Incoming::Refused(failed) => Work::Failed(failed),
        Incoming::Batch(elements) => Work::Batch(elements),
    };

    let bytes = line.len();
    match wiring.state.admit(work, bytes, started_at, start) {
        Ok(()) => Ok(()),
        Err((refusal, work)) => {
            // Section 9.6: a session that cannot queue a message still answers it, at
            // once, so that the caller learns now rather than when the queue drains.
            let calls = work.calls();
            let response = refusal_response(refusal, &work, &wiring.state);
            wiring.answer(&response.to_line(), calls)
        }
    }
}

/// Answers `solar.cancel` the moment it is read, which is what section 9.1 promises.
fn cancel_now<W: Write, F: Write>(
    request: Request,
    started_at: String,
    start: Instant,
    wiring: &Wiring<'_, W, F>,
) -> std::io::Result<()> {
    // A cancellation is answered even when the id it carries is in flight, because it is
    // not a call on that id: it is a question about it.
    let in_session = InSession {
        cancellation: Cancellation::new(),
        session: Some(Arc::clone(&wiring.state)),
    };
    let response = wiring
        .dispatcher
        .dispatch_in_session(request, started_at, start, &in_session);

    // A call cancelled while it was queued has already been answered with CANCELLED, so
    // that response goes out before the one that says so.
    wiring.answer_the_cancelled()?;
    wiring.answer(&response.to_line(), 1)
}

/// Runs the queued messages, one at a time, in the order they arrived.
fn run_from_queue<W: Write, F: Write>(wiring: &Wiring<'_, W, F>) -> std::io::Result<()> {
    while let Some((queued, token)) = wiring.state.take() {
        let ids = queued.work.ids();
        let calls = queued.work.calls();
        let in_session = InSession {
            cancellation: token,
            session: Some(Arc::clone(&wiring.state)),
        };
        let line = wiring.dispatcher.answer_work(
            queued.work,
            &queued.started_at,
            queued.start,
            &in_session,
        );
        // Finished before the line is written: a cancellation that arrives from here on
        // is told `already_finished`, which is exactly what happened.
        wiring.state.finish(&ids);
        wiring.answer(&line, calls)?;
    }
    Ok(())
}

/// A response to something that never became a request, so it has no id and no method.
fn bare_failure(error: SolarError) -> Response {
    let meta = Meta::new(None, None, None, clock::now_rfc3339_micros(), 0);
    Response::failure(None, error, meta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::RegistryBuilder;
    use serde_json::Value;
    use std::sync::Arc;

    fn empty_dispatcher() -> Dispatcher {
        Dispatcher::new(Arc::new(RegistryBuilder::new().build().unwrap()))
    }

    fn run(input: &str) -> Vec<Value> {
        let dispatcher = empty_dispatcher();
        let mut output = Vec::new();
        serve(input.as_bytes(), &mut output, &dispatcher).unwrap();
        String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[test]
    fn one_line_in_one_line_out() {
        let responses = run("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"solar.ping\"}\n");
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0]["id"], 1);
        assert_eq!(responses[0]["error"]["data"]["reason"], "METHOD_NOT_FOUND");
    }

    #[test]
    fn responses_come_back_in_request_order() {
        use std::fmt::Write as _;
        let mut input = String::new();
        for id in 1..=5 {
            let _ = writeln!(
                input,
                "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"method\":\"a.b\"}}"
            );
        }
        let responses = run(&input);
        let ids: Vec<i64> = responses
            .iter()
            .map(|r| r["id"].as_i64().unwrap())
            .collect();
        assert_eq!(ids, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn blank_lines_are_not_messages() {
        let responses = run("\n   \n\t\n");
        assert!(responses.is_empty());
    }

    #[test]
    fn a_carriage_return_before_the_newline_is_accepted() {
        let responses = run("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"a.b\"}\r\n");
        assert_eq!(responses[0]["error"]["data"]["reason"], "METHOD_NOT_FOUND");
    }

    #[test]
    fn a_last_line_without_a_newline_is_still_answered() {
        let responses = run("{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"a.b\"}");
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0]["id"], 9);
    }

    #[test]
    fn broken_json_is_answered_and_the_session_continues() {
        let responses = run("not json\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"a.b\"}\n");
        assert_eq!(responses.len(), 2);
        assert_eq!(responses[0]["error"]["code"], -32700);
        assert_eq!(responses[0]["id"], Value::Null);
        assert_eq!(responses[1]["id"], 2);
    }

    #[test]
    fn a_line_past_the_limit_is_refused_and_the_next_one_is_read() {
        let mut input = String::from("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"a.b\",\"pad\":\"");
        input.push_str(&"x".repeat(64));
        input.push_str("\"}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"a.b\"}\n");

        let dispatcher = empty_dispatcher();
        let mut output = Vec::new();
        let mut reader = input.as_bytes();
        let mut buffer = Vec::new();
        // The same loop as `serve`, with a limit small enough to trip on the first line
        // and wide enough for the second one, which is 39 bytes long.
        let first = read_line_limited(&mut reader, &mut buffer, 64).unwrap();
        assert!(matches!(first, Outcome::TooLong(_)));
        assert!(buffer.is_empty(), "nothing of an oversized line is kept");
        let second = read_line_limited(&mut reader, &mut buffer, 64).unwrap();
        assert_eq!(
            second,
            Outcome::Line,
            "the stream stays aligned after a refusal"
        );
        assert_eq!(
            String::from_utf8(buffer.clone()).unwrap(),
            "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"a.b\"}"
        );
        serve(input.as_bytes(), &mut output, &dispatcher).unwrap();
    }

    #[test]
    fn a_line_of_exactly_the_limit_is_read_and_one_byte_more_is_not() {
        // The limit is a limit, not a margin, and it is checked on both paths through the
        // reader: the chunk that holds the newline, and the chunk that does not.
        for with_newline in [true, false] {
            let mut input: Vec<u8> = vec![b'x'; 64];
            if with_newline {
                input.push(b'\n');
            }
            let mut reader = input.as_slice();
            let mut buffer = Vec::new();
            assert_eq!(
                read_line_limited(&mut reader, &mut buffer, 64).unwrap(),
                Outcome::Line,
                "64 bytes is not more than a limit of 64 (newline: {with_newline})"
            );
            assert_eq!(buffer.len(), 64);

            let mut input: Vec<u8> = vec![b'x'; 65];
            if with_newline {
                input.push(b'\n');
            }
            let mut reader = input.as_slice();
            let mut buffer = Vec::new();
            assert!(
                matches!(
                    read_line_limited(&mut reader, &mut buffer, 64).unwrap(),
                    Outcome::TooLong(_)
                ),
                "65 bytes is more than a limit of 64 (newline: {with_newline})"
            );
            assert!(buffer.is_empty(), "nothing of an oversized line is kept");
        }
    }

    #[test]
    fn an_oversized_line_reports_how_long_it_really_was() {
        // The number goes into the error the caller reads, so it has to be the length of
        // the line rather than of the piece that happened to cross the limit.
        for (bytes, with_newline) in [(100usize, true), (100, false), (5_000, true)] {
            let mut input: Vec<u8> = vec![b'x'; bytes];
            if with_newline {
                input.push(b'\n');
            }
            let mut reader = input.as_slice();
            let mut buffer = Vec::new();
            let outcome = read_line_limited(&mut reader, &mut buffer, 64).unwrap();
            assert_eq!(
                outcome,
                Outcome::TooLong(bytes),
                "a line of {bytes} bytes is reported as {bytes} bytes"
            );
        }
    }

    #[test]
    fn the_error_for_an_oversized_line_carries_the_length_it_was_given() {
        let error = too_large(4_096);
        assert_eq!(error.details()[0].received, Value::from(4_096));
        assert!(error.message().contains(&MAX_REQUEST_BYTES.to_string()));
    }

    #[test]
    fn invalid_utf8_is_a_parse_error() {
        let dispatcher = empty_dispatcher();
        let mut output = Vec::new();
        let input: &[u8] = &[0xff, 0xfe, b'\n'];
        serve(input, &mut output, &dispatcher).unwrap();
        let response: Value =
            serde_json::from_str(String::from_utf8(output).unwrap().trim()).unwrap();
        assert_eq!(response["error"]["data"]["reason"], "PARSE_ERROR");
        assert_eq!(response["error"]["code"], -32700);
    }

    #[test]
    fn the_message_too_large_error_says_what_the_limit_is() {
        let error = too_large(99);
        assert_eq!(error.reason(), Reason::MessageTooLarge);
        assert_eq!(error.details()[0].received, Value::from(99));
        assert!(error.message().contains(&MAX_REQUEST_BYTES.to_string()));
    }

    #[test]
    fn a_batch_is_one_line_in_and_one_line_out() {
        let responses = run(
            "[{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"a.b\"},{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"a.b\"}]\n",
        );
        assert_eq!(responses.len(), 1, "one message, one line");
        let array = responses[0].as_array().unwrap();
        assert_eq!(array.len(), 2);
        assert_eq!(array[0]["id"], 1);
        assert_eq!(array[1]["id"], 2);
    }

    #[test]
    fn a_batch_counts_as_many_calls_as_it_holds() {
        let dispatcher = empty_dispatcher();
        let mut output = Vec::new();
        let input = "[{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"a.b\"},{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"a.b\"},{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"a.b\"}]\n";
        let answered = serve(input.as_bytes(), &mut output, &dispatcher).unwrap();
        assert_eq!(answered, 3, "three calls, written on one line");
    }

    #[test]
    fn a_batch_that_is_wrong_as_a_whole_answers_with_one_response_not_an_array() {
        let responses = run("[]\n");
        assert_eq!(responses.len(), 1);
        assert!(responses[0].is_object(), "not an array: {}", responses[0]);
        assert_eq!(responses[0]["error"]["data"]["reason"], "BATCH_EMPTY");
        assert_eq!(responses[0]["error"]["code"], -32600);
    }

    #[test]
    fn a_batch_and_a_single_request_may_share_a_session() {
        let responses = run(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"a.b\"}\n[{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"a.b\"}]\n{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"a.b\"}\n",
        );
        assert_eq!(responses.len(), 3);
        assert_eq!(responses[0]["id"], 1);
        assert_eq!(responses[1][0]["id"], 2);
        assert_eq!(responses[2]["id"], 3);
    }

    #[test]
    fn the_session_reports_how_many_calls_it_answered() {
        let dispatcher = empty_dispatcher();
        let mut output = Vec::new();
        let input = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"a.b\"}\n\n{\"bad\"\n";
        let answered = serve(input.as_bytes(), &mut output, &dispatcher).unwrap();
        assert_eq!(answered, 2);
    }
}
