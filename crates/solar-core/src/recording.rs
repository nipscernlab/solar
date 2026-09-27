//! A session written down, and played back.
//!
//! A bug report that says "it answered wrongly" is a conversation. A bug report with a
//! recording is a file somebody can replay. `solar serve --stdio --record <file>` writes
//! every line in and every line out, with the time each crossed, and `solar replay
//! <file>` sends the requests again and says where the answers differ.
//!
//! The file is NDJSON, one entry per line, so it can be read by anything and trimmed with
//! a text editor before being attached to a report.

use std::io::Write;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::clock;

/// Which way a line crossed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// A request, from the caller to SOLAR.
    In,
    /// A response, from SOLAR to the caller.
    Out,
}

/// One line of a session, as it is written down.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// When it crossed: RFC 3339, UTC, microseconds.
    pub at: String,
    /// Which way.
    pub direction: Direction,
    /// The line itself, exactly as it was, without its newline.
    pub line: String,
}

/// Writes a session down as it happens.
#[derive(Debug)]
pub struct Recorder<W: Write> {
    into: W,
}

impl<W: Write> Recorder<W> {
    /// A recorder that writes into `into`.
    pub const fn new(into: W) -> Self {
        Self { into }
    }

    /// Writes one line down.
    ///
    /// # Errors
    ///
    /// Returns whatever the underlying writer returned. A session whose recording cannot
    /// be written stops, because a half written recording is worse than none.
    pub fn record(&mut self, direction: Direction, line: &str) -> std::io::Result<()> {
        let entry = Entry {
            at: clock::now_rfc3339_micros(),
            direction,
            line: line.to_owned(),
        };
        let text = serde_json::to_string(&entry)
            .unwrap_or_else(|_| String::from(r#"{"at":"","direction":"in","line":""}"#));
        writeln!(self.into, "{text}")?;
        self.into.flush()
    }
}

/// Reads a recording back.
///
/// # Errors
///
/// Returns which line of the file could not be read and why.
pub fn read(text: &str) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    for (number, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let entry: Entry = serde_json::from_str(line).map_err(|failure| {
            format!(
                "line {} of the recording is not an entry: {failure}",
                number + 1
            )
        })?;
        entries.push(entry);
    }
    Ok(entries)
}

/// The members of `meta` that differ between two runs of the same call.
pub const VOLATILE_META: [&str; 3] = ["started_at", "duration_us", "solar_version"];

/// What a timestamp SOLAR writes looks like: `YYYY-MM-DDThh:mm:ss.ffffffZ`.
const TIMESTAMP_LENGTH: usize = 27;

/// Whether a string is one of the timestamps SOLAR writes.
///
/// The shape is exact, so a message that merely contains a date is not mistaken for one.
/// A timestamp differs between any two runs and is never evidence of a bug, wherever it
/// sits: `meta.started_at`, or `received_at` inside the answer of `solar.ping`.
fn is_a_timestamp(text: &str) -> bool {
    text.len() == TIMESTAMP_LENGTH
        && text.ends_with('Z')
        && text.as_bytes().get(10) == Some(&b'T')
        && text.as_bytes().get(4) == Some(&b'-')
        && text.as_bytes().get(19) == Some(&b'.')
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '-' | ':' | '.' | 'T' | 'Z'))
}

/// Replaces everything that differs between two runs, so that two responses to the same
/// request can be compared.
///
/// Two kinds of value go: the members of `meta` listed in [`VOLATILE_META`], and any
/// timestamp anywhere in the response. Everything else is compared as it stands,
/// including the whole of `data`: a replay that ignored more than it had to would pass
/// while the answer changed.
#[must_use]
pub fn without_volatile_values(response: &str) -> Value {
    let Ok(mut value) = serde_json::from_str::<Value>(response) else {
        return Value::String(response.to_owned());
    };

    // `meta` sits in one of two places, depending on whether the call succeeded.
    for place in [
        ["result", "meta"].as_slice(),
        ["error", "data", "meta"].as_slice(),
    ] {
        if let Some(meta) = follow(&mut value, place).and_then(Value::as_object_mut) {
            for member in VOLATILE_META {
                if meta.contains_key(member) {
                    meta.insert(member.to_owned(), Value::String("[volatile]".to_owned()));
                }
            }
        }
    }

    replace_timestamps(&mut value);
    value
}

/// Replaces every timestamp in a value, however deep.
fn replace_timestamps(value: &mut Value) {
    match value {
        Value::String(text) if is_a_timestamp(text) => {
            *value = Value::String("[timestamp]".to_owned());
        }
        Value::Array(items) => items.iter_mut().for_each(replace_timestamps),
        Value::Object(members) => {
            members
                .iter_mut()
                .for_each(|(_, member)| replace_timestamps(member));
        }
        _ => {}
    }
}

/// Walks a path of member names into a value, if every step is there.
fn follow<'a>(value: &'a mut Value, path: &[&str]) -> Option<&'a mut Value> {
    let mut node = value;
    for step in path {
        node = node.get_mut(*step)?;
    }
    Some(node)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A_PING: &str = r#"{"jsonrpc":"2.0","id":1,"result":{"data":{"pong":true,"received_at":"2026-09-27T05:15:05.235392Z"},"meta":{"request_id":1,"method":"solar.ping","api_version":"1.0.0","solar_version":"0.1.0","protocol":"solar/1","started_at":"2026-09-27T05:15:05.235074Z","duration_us":348,"os":"windows","arch":"x86_64"},"warnings":[]}}"#;

    #[test]
    fn a_session_is_written_down_in_order_with_its_times() {
        let mut written = Vec::new();
        let mut recorder = Recorder::new(&mut written);
        recorder.record(Direction::In, "{\"a\":1}").unwrap();
        recorder.record(Direction::Out, "{\"b\":2}").unwrap();

        let entries = read(&String::from_utf8(written).unwrap()).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].direction, Direction::In);
        assert_eq!(entries[0].line, "{\"a\":1}");
        assert_eq!(entries[1].direction, Direction::Out);
        assert!(
            entries[0].at.ends_with('Z'),
            "every entry says when it crossed"
        );
        assert!(entries[0].at <= entries[1].at, "and they are in order");
    }

    #[test]
    fn a_line_with_a_newline_in_it_survives_the_round_trip() {
        // The recording is NDJSON, so a line that contains an escaped newline must not
        // become two entries.
        let mut written = Vec::new();
        let mut recorder = Recorder::new(&mut written);
        recorder
            .record(Direction::In, "{\"a\":\"one\ntwo\"}")
            .unwrap();

        let text = String::from_utf8(written).unwrap();
        assert_eq!(text.lines().count(), 1);
        assert_eq!(read(&text).unwrap()[0].line, "{\"a\":\"one\ntwo\"}");
    }

    #[test]
    fn a_recording_that_is_not_one_says_which_line_is_wrong() {
        let failure = read("{\"at\":\"now\",\"direction\":\"in\",\"line\":\"x\"}\nnot an entry\n")
            .unwrap_err();
        assert!(failure.contains("line 2"), "{failure}");
    }

    #[test]
    fn two_runs_of_the_same_call_compare_equal() {
        let later = A_PING
            .replace("2026-09-27T05:15:05.235392Z", "2026-09-27T09:00:00.000001Z")
            .replace("2026-09-27T05:15:05.235074Z", "2026-09-27T09:00:00.000000Z")
            .replace("\"duration_us\":348", "\"duration_us\":9");
        assert_eq!(
            without_volatile_values(A_PING),
            without_volatile_values(&later),
            "a timestamp and a duration are never evidence of a bug"
        );
    }

    #[test]
    fn a_different_answer_does_not_compare_equal() {
        let changed = A_PING.replace("\"pong\":true", "\"pong\":false");
        assert_ne!(
            without_volatile_values(A_PING),
            without_volatile_values(&changed)
        );

        let renamed = A_PING.replace("\"method\":\"solar.ping\"", "\"method\":\"solar.pong\"");
        assert_ne!(
            without_volatile_values(A_PING),
            without_volatile_values(&renamed)
        );

        let other_version =
            A_PING.replace("\"api_version\":\"1.0.0\"", "\"api_version\":\"2.0.0\"");
        assert_ne!(
            without_volatile_values(A_PING),
            without_volatile_values(&other_version),
            "the version of the API that answered is not volatile"
        );
    }

    #[test]
    fn only_the_exact_shape_of_a_timestamp_is_taken_for_one() {
        assert!(is_a_timestamp("2026-09-27T05:15:05.235392Z"));
        assert!(!is_a_timestamp("2026-09-27T05:15:05Z"), "no microseconds");
        assert!(!is_a_timestamp("2026-09-27"), "a date is not a timestamp");
        assert!(!is_a_timestamp(
            "the meeting is at 2026-09-27T05:15:05.235392Z"
        ));
        assert!(!is_a_timestamp(""));
    }

    #[test]
    fn something_that_is_not_json_is_compared_as_it_stands() {
        assert_eq!(
            without_volatile_values("not json"),
            Value::String("not json".to_owned())
        );
    }
}
