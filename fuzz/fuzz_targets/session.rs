//! The session loop, against an arbitrary stream.
//!
//! The loop reads lines, refuses the oversized ones, skips the blank ones and answers the
//! rest. This drives it with whatever bytes the fuzzer produces and checks the promise
//! that matters: the session ends, and every line it answered is one well formed line.

#![no_main]

use libfuzzer_sys::fuzz_target;
use solar_core::dispatch::Dispatcher;
use solar_core::server::serve;

fuzz_target!(|data: &[u8]| {
    static DISPATCHER: std::sync::OnceLock<Dispatcher> = std::sync::OnceLock::new();
    let dispatcher = DISPATCHER.get_or_init(solar_apis::dispatcher);

    let mut output = Vec::new();
    let answered = serve(data, &mut output, dispatcher).expect("writing to a Vec cannot fail");

    let text = String::from_utf8(output).expect("SOLAR writes UTF-8");
    let lines = text.lines().count() as u64;
    assert_eq!(lines, answered, "every answer is exactly one line");
    for line in text.lines() {
        assert!(serde_json::from_str::<serde_json::Value>(line).is_ok(), "{line} is not JSON");
    }
});
