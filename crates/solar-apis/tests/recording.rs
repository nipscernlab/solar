//! The recording format is what `docs/RECORDING.md` says it is.
//!
//! The document carries a JSON Schema for one line, so that something other than SOLAR can
//! write a recording and have it replayed. A schema in prose drifts from the code that
//! writes the file; this compiles the schema out of the document and validates against it
//! what SOLAR really writes.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test reports failure by panicking; integration tests are their own crate"
)]

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use solar_core::recording::{Direction, Header, RECORDING_FORMAT_VERSION, Recorder, read};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate lives two directories below the repository root")
        .to_path_buf()
}

/// The schema written in `docs/RECORDING.md`, taken out of its fenced block.
///
/// The document holds more than one `json` block: the example recording is one, and it is
/// NDJSON rather than a single document. The schema is the block that declares `$schema`,
/// and there must be exactly one of those, so this fails rather than guessing.
fn schema_from_the_document() -> Value {
    let text = std::fs::read_to_string(repository_root().join("docs/RECORDING.md"))
        .expect("docs/RECORDING.md must be readable");

    let mut blocks = Vec::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        match (line.trim_end(), current.as_mut()) {
            ("```json", None) => current = Some(String::new()),
            ("```", Some(_)) => blocks.push(current.take().unwrap_or_default()),
            (_, Some(block)) => {
                block.push_str(line);
                block.push('\n');
            }
            _ => {}
        }
    }

    let schemas: Vec<Value> = blocks
        .iter()
        .filter_map(|block| serde_json::from_str::<Value>(block).ok())
        .filter(|value| value.get("$schema").is_some())
        .collect();

    assert_eq!(
        schemas.len(),
        1,
        "docs/RECORDING.md must hold exactly one json block that declares $schema"
    );
    schemas.into_iter().next().unwrap_or(Value::Null)
}

/// A recording of a real session, as `solar serve --stdio --record` writes one.
fn a_real_recording() -> String {
    let mut written = Vec::new();
    {
        let mut recorder = Recorder::new(&mut written);
        recorder
            .write_header()
            .expect("writing to a Vec cannot fail");
        recorder
            .record(
                Direction::In,
                r#"{"jsonrpc":"2.0","id":1,"method":"solar.ping"}"#,
            )
            .expect("writing to a Vec cannot fail");
        recorder
            .record(Direction::Out, r#"{"jsonrpc":"2.0","id":1,"result":{}}"#)
            .expect("writing to a Vec cannot fail");
        // A line that was never JSON is exactly the kind worth recording.
        recorder
            .record(Direction::In, "{not json")
            .expect("writing to a Vec cannot fail");
    }
    String::from_utf8(written).expect("SOLAR writes UTF-8")
}

#[test]
fn every_line_of_a_recording_validates_against_the_schema_in_the_document() {
    let schema = schema_from_the_document();
    let validator =
        jsonschema::validator_for(&schema).expect("the schema in docs/RECORDING.md must compile");

    let recording = a_real_recording();
    for (number, line) in recording.lines().enumerate() {
        let value: Value = serde_json::from_str(line)
            .unwrap_or_else(|failure| panic!("line {} is not JSON: {failure}", number + 1));
        assert!(
            validator.is_valid(&value),
            "line {} does not match the schema in docs/RECORDING.md: {line}",
            number + 1
        );
    }
}

#[test]
fn the_schema_refuses_what_the_document_says_it_refuses() {
    let schema = schema_from_the_document();
    let validator = jsonschema::validator_for(&schema).expect("the schema must compile");

    let refused = [
        // A header without the version that names the format.
        json!({"at": "2026-09-27T21:05:49.374016Z", "solar_version": "0.3.0", "protocol": "solar/1"}),
        // An entry with a direction nobody defined.
        json!({"at": "2026-09-27T21:05:49.374016Z", "direction": "sideways", "line": "{}"}),
        // A line that is an object rather than the text that crossed.
        json!({"at": "2026-09-27T21:05:49.374016Z", "direction": "in", "line": {"jsonrpc": "2.0"}}),
        // A timestamp that is not the shape SOLAR writes.
        json!({"at": "2026-09-27 21:05:49", "direction": "in", "line": "{}"}),
        // A member nobody declared.
        json!({"at": "2026-09-27T21:05:49.374016Z", "direction": "in", "line": "{}", "note": "hello"}),
    ];

    for value in refused {
        assert!(
            !validator.is_valid(&value),
            "the schema accepts something it should refuse: {value}"
        );
    }
}

#[test]
fn the_document_states_the_version_this_build_writes() {
    let text = std::fs::read_to_string(repository_root().join("docs/RECORDING.md"))
        .expect("docs/RECORDING.md must be readable");
    assert!(
        text.contains(&format!(
            "**Format version:**\n`{RECORDING_FORMAT_VERSION}`"
        )) || text.contains(&format!("**Format version:** `{RECORDING_FORMAT_VERSION}`")),
        "docs/RECORDING.md does not state that this build writes {RECORDING_FORMAT_VERSION}"
    );
    assert!(
        text.contains(&format!("| `{RECORDING_FORMAT_VERSION}` |")),
        "docs/RECORDING.md has no row for {RECORDING_FORMAT_VERSION} in its table of versions"
    );
}

#[test]
fn a_recording_written_here_is_read_back_whole() {
    let recording = read(&a_real_recording()).expect("what SOLAR writes, SOLAR reads");

    let header = recording.header.expect("the first line is a header");
    assert_eq!(header.solar_recording, RECORDING_FORMAT_VERSION);
    assert_eq!(header.protocol, "solar/1");

    assert_eq!(recording.entries.len(), 3);
    assert_eq!(recording.entries[0].direction, Direction::In);
    assert_eq!(recording.entries[2].line, "{not json");
}

#[test]
fn a_version_this_build_does_not_read_is_refused_and_says_so() {
    let header = json!({
        "solar_recording": "2.0.0",
        "at": "2026-09-27T21:05:49.374016Z",
        "solar_version": "9.9.9",
        "protocol": "solar/2",
    });
    let text = format!("{header}\n");

    let failure = read(&text).expect_err("a major nobody knows is refused");
    assert!(
        failure.contains("2.0.0"),
        "it names the file's version: {failure}"
    );
    assert!(
        failure.contains(RECORDING_FORMAT_VERSION),
        "and the one this build reads: {failure}"
    );
    assert!(
        failure.contains("docs/RECORDING.md"),
        "and where to read about it: {failure}"
    );
}

#[test]
fn a_minor_version_this_build_has_not_seen_is_read_anyway() {
    // The same rule as every other version here: a reader refuses a different major and
    // reads a different minor by ignoring what it does not know.
    let text = concat!(
        r#"{"solar_recording":"1.7.0","at":"2026-09-27T21:05:49.374016Z","solar_version":"9.9.9","protocol":"solar/1"}"#,
        "\n",
        r#"{"at":"2026-09-27T21:05:49.374181Z","direction":"in","line":"{}"}"#,
        "\n",
    );
    let recording = read(text).expect("a later minor is still this format");
    assert_eq!(recording.entries.len(), 1);
}

#[test]
fn a_file_with_no_header_is_read_as_what_it_looks_like() {
    // Recordings written before the format was versioned, by SOLAR 0.2.0 and earlier.
    let text = concat!(
        r#"{"at":"2026-09-27T21:05:49.374181Z","direction":"in","line":"{}"}"#,
        "\n",
    );
    let recording = read(text).expect("an old recording is still readable");
    assert!(recording.header.is_none(), "there is nothing to report");
    assert_eq!(recording.entries.len(), 1);
}

#[test]
fn a_header_anywhere_but_the_first_line_is_refused() {
    let text = concat!(
        r#"{"at":"2026-09-27T21:05:49.374181Z","direction":"in","line":"{}"}"#,
        "\n",
        r#"{"solar_recording":"1.0.0","at":"2026-09-27T21:05:49.374016Z","solar_version":"0.3.0","protocol":"solar/1"}"#,
        "\n",
    );
    let failure = read(text).expect_err("a header belongs on the first line");
    assert!(failure.contains("first line"), "{failure}");
}

#[test]
fn the_header_this_build_writes_names_this_build() {
    let header = Header::current();
    assert_eq!(header.solar_recording, RECORDING_FORMAT_VERSION);
    assert_eq!(header.solar_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(header.protocol, "solar/1");
    assert!(header.at.ends_with('Z'));
}
