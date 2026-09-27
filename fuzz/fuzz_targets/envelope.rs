//! The envelope parser, against arbitrary bytes.
//!
//! What is being proved is the first principle at its outermost edge: whatever arrives,
//! `parse_request` either returns a request or returns an error, and never panics.

#![no_main]

use libfuzzer_sys::fuzz_target;
use solar_core::protocol::parse_request;

fuzz_target!(|data: &[u8]| {
    let Ok(line) = std::str::from_utf8(data) else {
        return;
    };
    match parse_request(line) {
        Ok(request) => {
            // A request that parsed must carry what the contract promises it carries.
            assert!(!request.method.is_empty());
        }
        Err(failed) => {
            // An error that reaches the wire always has at least one detail entry.
            let error = failed.error.ensure_detailed();
            assert!(!error.details().is_empty());
        }
    }
});
