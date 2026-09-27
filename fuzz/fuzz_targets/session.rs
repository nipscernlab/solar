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
    // One message is one line, and one line answers one call unless it is a batch, in
    // which case it answers as many calls as the array holds.
    let mut calls = 0u64;
    for line in text.lines() {
        let value: serde_json::Value =
            serde_json::from_str(line).unwrap_or_else(|_| panic!("{line} is not JSON"));
        calls += match value.as_array() {
            Some(answers) => answers.len() as u64,
            None => 1,
        };
    }
    assert_eq!(calls, answered, "every call is answered exactly once");
});
