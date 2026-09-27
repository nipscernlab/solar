//! The NDJSON session: read a line, answer it, read the next one.
//!
//! The loop is deliberately sequential. One request is read, dispatched and answered before
//! the next is read, so responses come back in request order and a caller never has to
//! correlate anything. Concurrency, if it is ever needed, is a protocol change and not a
//! detail of this loop.

use std::io::{BufRead, Write};

use crate::clock;
use crate::dispatch::Dispatcher;
use crate::error::{ErrorDetail, SolarError};
use crate::logging;
use crate::meta::Meta;
use crate::protocol::{MAX_REQUEST_BYTES, Response};
use crate::reason::Reason;
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

/// Runs a session until the input ends.
///
/// Every line that is a message produces exactly one response line, flushed immediately so
/// that a caller reading a pipe never waits for a buffer to fill. A line that is empty or
/// only whitespace is not a message and produces nothing.
///
/// # Errors
///
/// Returns the first input or output failure. A failure here means the session itself broke,
/// which the command line interface reports as exit code 70.
pub fn serve<R: BufRead, W: Write>(
    mut input: R,
    mut output: W,
    dispatcher: &Dispatcher,
) -> std::io::Result<u64> {
    let mut buffer: Vec<u8> = Vec::with_capacity(8 * 1024);
    let mut answered = 0u64;

    loop {
        match read_line_limited(&mut input, &mut buffer, MAX_REQUEST_BYTES)? {
            Outcome::Eof => {
                logging::info(&format!("the session ended after {answered} calls"));
                return Ok(answered);
            }
            Outcome::TooLong(bytes) => {
                logging::warn(&format!("a request of {bytes} bytes was refused"));
                let response = bare_failure(too_large(bytes));
                write_response(&mut output, &response)?;
                answered += 1;
            }
            Outcome::Line => {
                let line = match std::str::from_utf8(&buffer) {
                    Ok(text) => text.trim_end_matches('\r'),
                    Err(broken) => {
                        let response = bare_failure(not_utf8(broken.valid_up_to()));
                        write_response(&mut output, &response)?;
                        answered += 1;
                        continue;
                    }
                };

                if line.trim().is_empty() {
                    continue;
                }

                logging::trace(&format!("--> {line}"));
                let response = dispatcher.handle_line(line);
                write_response(&mut output, &response)?;
                answered += 1;
            }
        }
    }
}

/// Serialises one response and flushes it.
fn write_response<W: Write>(output: &mut W, response: &Response) -> std::io::Result<()> {
    let line = response.to_line();
    logging::trace(&format!("<-- {line}"));
    output.write_all(line.as_bytes())?;
    output.write_all(b"\n")?;
    output.flush()
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
    fn the_session_reports_how_many_calls_it_answered() {
        let dispatcher = empty_dispatcher();
        let mut output = Vec::new();
        let input = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"a.b\"}\n\n{\"bad\"\n";
        let answered = serve(input.as_bytes(), &mut output, &dispatcher).unwrap();
        assert_eq!(answered, 2);
    }
}
