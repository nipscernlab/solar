//! The cap on abandoned handlers, section 10 of the contract.
//!
//! A handler that overruns its budget is abandoned rather than killed, and keeps its stack
//! until it finishes on its own. Without a cap, an API that overruns on every call would
//! consume the process one thread at a time. This file fills the cap on purpose.
//!
//! It is a test binary of its own because the count is a process-wide number: sharing it
//! with the other tests of dispatch would make them watch each other rather than the cap.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test reports failure by panicking; integration tests are their own crate"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use solar_core::api::{Api, ApiSpec, DEFAULT_MAX_OUTPUT_BYTES, Example, SideEffect, Stability};
use solar_core::context::Context;
use solar_core::dispatch::{Dispatcher, MAX_ABANDONED_WORKERS, abandoned_workers};
use solar_core::error::SolarError;
use solar_core::registry::RegistryBuilder;

/// Every lingering handler watches this, and the test sets it when it is done with them.
static RELEASED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NoParams {}

#[derive(Debug, Serialize, JsonSchema)]
struct Fine {
    fine: bool,
}

/// An API that ignores its budget, and waits until the test lets it go.
struct Lingers;

impl Api for Lingers {
    const NAME: &'static str = "test.lingers";
    const VERSION: &'static str = "1.0.0";
    type Params = NoParams;
    type Output = Fine;

    fn spec() -> ApiSpec {
        ApiSpec {
            summary: "Overruns a very small budget and stays alive",
            description: "An API that exists only inside this test, to fill the cap on \
                          abandoned handlers that section 10 of the contract declares.",
            errors: Vec::new(),
            side_effects: vec![SideEffect::None],
            idempotent: true,
            stability: Stability::Experimental,
            since: "0.1.0",
            timeout_ms: 10,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            examples: vec![Example::exact(
                "plain",
                "The only call",
                json!({}),
                json!({"fine": true}),
            )],
        }
    }

    fn call(_ctx: &Context, _params: NoParams) -> Result<Fine, SolarError> {
        // Held until the test releases every lingering handler at once. The deadline is a
        // backstop, so that a failing test leaves no thread behind.
        let deadline = Instant::now() + Duration::from_secs(20);
        while !RELEASED.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(Fine { fine: true })
    }
}

fn call(dispatcher: &Dispatcher) -> Value {
    let line = r#"{"jsonrpc":"2.0","id":1,"method":"test.lingers"}"#;
    serde_json::from_str(&dispatcher.handle_line(line).to_line()).expect("a response is JSON")
}

#[test]
fn abandoned_handlers_are_capped_and_the_count_is_visible() {
    let registry = RegistryBuilder::new()
        .register::<Lingers>()
        .build()
        .expect("the test registry must be valid");
    let dispatcher = Dispatcher::new(Arc::new(registry));

    assert_eq!(
        abandoned_workers(),
        0,
        "this binary starts with nothing abandoned"
    );

    // Every call overruns its ten milliseconds and leaves its thread behind, until the cap
    // is reached and dispatch refuses to start another one.
    let mut refused = None;
    for _ in 0..=MAX_ABANDONED_WORKERS {
        let response = call(&dispatcher);
        let reason = response["error"]["data"]["reason"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        if reason == "TOO_MANY_ABANDONED" {
            refused = Some(response);
            break;
        }
        assert_eq!(reason, "HANDLER_TIMEOUT", "{response}");
    }

    let refused = refused.expect("the cap must be reached, and must then refuse");
    assert_eq!(refused["error"]["data"]["status"], "UNAVAILABLE");
    assert_eq!(refused["error"]["code"], -32006);
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap()
            .contains(&MAX_ABANDONED_WORKERS.to_string()),
        "the message says what the cap is: {}",
        refused["error"]["message"]
    );
    assert_eq!(
        abandoned_workers(),
        MAX_ABANDONED_WORKERS,
        "the count a caller reads is the count that was enforced"
    );

    // Released, the threads end, and the count falls back to nothing.
    RELEASED.store(true, Ordering::SeqCst);
    let deadline = Instant::now() + Duration::from_secs(20);
    while abandoned_workers() > 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        abandoned_workers(),
        0,
        "an abandoned handler stops being counted when it finishes"
    );

    // With room again, a call runs as it always did.
    let response = call(&dispatcher);
    assert_eq!(response["result"]["data"]["fine"], true);
}
