//! Parameter decoding, against arbitrary JSON.
//!
//! Dispatch reads parameters before a handler ever runs, and the error it builds walks the
//! schema of the API. This drives that path with whatever the fuzzer invents.

#![no_main]

use libfuzzer_sys::fuzz_target;
use solar_core::dispatch::Dispatcher;

fuzz_target!(|data: &[u8]| {
    static DISPATCHER: std::sync::OnceLock<Dispatcher> = std::sync::OnceLock::new();
    let dispatcher = DISPATCHER.get_or_init(solar_apis::dispatcher);

    let Ok(params) = std::str::from_utf8(data) else {
        return;
    };
    // Every API in turn, so the fuzzer reaches every parameter type there is.
    for method in ["solar.ping", "solar.describe", "solar.manifest", "solar.version", "system.info"]
    {
        let line = format!("{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"{method}\",\"params\":{params}}}");
        let response = dispatcher.handle_line(&line);
        let text = response.to_line();
        assert!(!text.contains('\n'), "a response must never break the framing");
        assert!(serde_json::from_str::<serde_json::Value>(&text).is_ok());
    }
});
