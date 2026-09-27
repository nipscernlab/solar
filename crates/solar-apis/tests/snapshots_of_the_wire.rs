//! Every shape that goes on the wire, frozen.
//!
//! A snapshot test answers a question the other tests do not: *did anything change?* The
//! contract tests check that a response obeys the rules; these check that it is the same
//! response as yesterday, character for character, so that a change to an error message,
//! a field order or a hint is something somebody chose rather than something that
//! happened.
//!
//! What changes between runs is redacted: timestamps, durations, paths, and the version
//! of the build. What is left is the shape and the words.
//!
//! A failing snapshot is reviewed with `cargo insta review`, never accepted blindly. The
//! diff is the point.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test reports failure by panicking; integration tests are their own crate"
)]

use insta::{assert_json_snapshot, with_settings};
use serde_json::{Value, json};
use solar_core::dispatch::Dispatcher;

fn dispatcher() -> Dispatcher {
    solar_apis::dispatcher()
}

/// The answer to one line, as JSON, ready to snapshot. A batch answers with an array.
fn answer(line: &str) -> Value {
    let answered = dispatcher().answer_line(line);
    serde_json::from_str(&answered.line).expect("an answer is JSON")
}

/// A batch one element past the limit, which is refused as a whole.
fn too_many_elements() -> String {
    use std::fmt::Write as _;
    let mut line = String::from("[");
    for id in 0..=solar_core::protocol::MAX_BATCH_ELEMENTS {
        if id > 0 {
            line.push(',');
        }
        let _ = write!(
            line,
            r#"{{"jsonrpc":"2.0","id":{id},"method":"solar.ping"}}"#
        );
    }
    line.push(']');
    line
}

/// A request line for a method and its parameters.
fn request(method: &str, params: &Value) -> String {
    json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).to_string()
}

macro_rules! snapshot_envelope {
    ($name:ident, $line:expr) => {
        #[test]
        fn $name() {
            with_settings!({sort_maps => true}, {
                assert_json_snapshot!(answer(&$line), {
                    ".**.started_at" => "[timestamp]",
                    ".**.duration_us" => "[duration]",
                    ".**.received_at" => "[timestamp]",
                    ".**.solar_version" => "[version]",
                    ".**.os" => "[os]",
                    ".**.arch" => "[arch]",
                });
            });
        }
    };
}

// One snapshot per reason that dispatch itself can produce, which is the list a caller
// has to be able to handle.
snapshot_envelope!(parse_error, "{not json");
snapshot_envelope!(
    notification_not_supported,
    r#"{"jsonrpc":"2.0","method":"solar.ping"}"#
);
snapshot_envelope!(batch_empty, "[]");
snapshot_envelope!(batch_too_large, &too_many_elements());
snapshot_envelope!(
    duplicate_id_in_a_batch,
    r#"[{"jsonrpc":"2.0","id":7,"method":"solar.ping"},{"jsonrpc":"2.0","id":7,"method":"solar.version"}]"#
);
snapshot_envelope!(
    a_batch_of_a_good_and_a_bad_element,
    r#"[{"jsonrpc":"2.0","id":1,"method":"solar.ping"},{"jsonrpc":"2.0","id":2,"method":"solar.pign"}]"#
);
snapshot_envelope!(
    missing_field_in_the_envelope,
    r#"{"id":1,"method":"solar.ping"}"#
);
snapshot_envelope!(
    type_mismatch_in_the_envelope,
    r#"{"jsonrpc":"2.0","id":1,"method":"solar.ping","params":[1]}"#
);
snapshot_envelope!(
    invalid_value_in_the_envelope,
    r#"{"jsonrpc":"1.0","id":1,"method":"solar.ping"}"#
);
snapshot_envelope!(
    method_not_found,
    r#"{"jsonrpc":"2.0","id":1,"method":"solar.pign"}"#
);
snapshot_envelope!(
    api_not_found,
    request("solar.describe", &json!({"api": "solar.pign"}))
);
snapshot_envelope!(
    missing_field_in_params,
    request("solar.describe", &json!({}))
);
snapshot_envelope!(
    unknown_field_in_params,
    request("solar.ping", &json!({"mesage": "hi"}))
);
snapshot_envelope!(
    type_mismatch_in_params,
    request("solar.describe", &json!({"api": 3}))
);
snapshot_envelope!(
    a_successful_ping,
    request("solar.ping", &json!({"message": "hi"}))
);

#[test]
fn the_manifest_is_the_document_it_was() {
    let registry = solar_apis::registry().expect("a valid registry");
    let manifest = solar_core::manifest::build(registry);
    with_settings!({sort_maps => true}, {
        assert_json_snapshot!(manifest, {".solar_version" => "[version]"});
    });
}
