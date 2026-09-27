//! Batches, against arbitrary arrays.
//!
//! Section 3.2 of the contract promises three things that hold whatever the array holds:
//! the answer is one line, a batch answers with one response per element in the order of
//! the elements, and a batch refused as a whole answers with a single response instead.

#![no_main]

use libfuzzer_sys::fuzz_target;
use serde_json::Value;
use solar_core::dispatch::Dispatcher;
use solar_core::protocol::{Incoming, read_line};

fuzz_target!(|data: &[u8]| {
    let Ok(line) = std::str::from_utf8(data) else {
        return;
    };
    static DISPATCHER: std::sync::OnceLock<Dispatcher> = std::sync::OnceLock::new();
    let dispatcher = DISPATCHER.get_or_init(solar_apis::dispatcher);

    let read = read_line(line);
    let answer = dispatcher.answer_line(line);
    assert!(!answer.line.contains('\n'), "the framing is one line");

    let value: Value =
        serde_json::from_str(&answer.line).expect("an answer is JSON");

    match read {
        Incoming::Batch(elements) => {
            let answers = value.as_array().expect("a batch answers with an array");
            assert_eq!(answers.len(), elements.len(), "one response per element");
            assert_eq!(answer.calls, elements.len());
        }
        Incoming::One(_) | Incoming::Refused(_) => {
            assert!(value.is_object(), "a single response is an object");
            assert_eq!(answer.calls, 1);
        }
    }
});
