//! The two promises that are hardest to keep: a handler that panics and one that hangs.
//!
//! Both are checked against a registry of APIs that exist only here, because no real API
//! is allowed to do either. What matters is not only that the caller gets an answer, but
//! that the session is still usable afterwards.

// A test reports failure by panicking, and two of these APIs panic on purpose.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use solar_core::api::{Api, ApiSpec, Example, SideEffect, Stability};
use solar_core::context::Context;
use solar_core::dispatch::Dispatcher;
use solar_core::error::SolarError;
use solar_core::registry::RegistryBuilder;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NoParams {}

#[derive(Debug, Serialize, JsonSchema)]
struct Fine {
    fine: bool,
}

fn spec(summary: &'static str, timeout_ms: u64) -> ApiSpec {
    ApiSpec {
        summary,
        description: "An API that exists only inside the tests of dispatch, to prove what \
                      happens when a handler misbehaves.",
        errors: Vec::new(),
        side_effects: vec![SideEffect::None],
        idempotent: true,
        stability: Stability::Experimental,
        since: "0.1.0",
        timeout_ms,
        examples: vec![Example::exact(
            "plain",
            "The only call",
            json!({}),
            json!({"fine": true}),
        )],
    }
}

/// An API that answers normally.
struct Works;

impl Api for Works {
    const NAME: &'static str = "test.works";
    const VERSION: &'static str = "1.0.0";
    type Params = NoParams;
    type Output = Fine;
    fn spec() -> ApiSpec {
        spec("Answers normally", 1_000)
    }
    fn call(_ctx: &Context, _params: NoParams) -> Result<Fine, SolarError> {
        Ok(Fine { fine: true })
    }
}

/// An API that panics, which no real API is allowed to do.
struct Panics;

impl Api for Panics {
    const NAME: &'static str = "test.panics";
    const VERSION: &'static str = "1.0.0";
    type Params = NoParams;
    type Output = Fine;
    fn spec() -> ApiSpec {
        spec("Panics on purpose", 1_000)
    }
    fn call(_ctx: &Context, _params: NoParams) -> Result<Fine, SolarError> {
        panic!("the handler fell over");
    }
}

/// An API that ignores its budget entirely.
struct Hangs;

impl Api for Hangs {
    const NAME: &'static str = "test.hangs";
    const VERSION: &'static str = "1.0.0";
    type Params = NoParams;
    type Output = Fine;
    fn spec() -> ApiSpec {
        spec("Runs far past its budget", 150)
    }
    fn call(_ctx: &Context, _params: NoParams) -> Result<Fine, SolarError> {
        std::thread::sleep(Duration::from_secs(5));
        Ok(Fine { fine: true })
    }
}

fn dispatcher() -> Dispatcher {
    let registry = RegistryBuilder::new()
        .register::<Works>()
        .register::<Panics>()
        .register::<Hangs>()
        .build()
        .expect("the test registry must be valid");
    Dispatcher::new(Arc::new(registry))
}

fn call(dispatcher: &Dispatcher, method: &str) -> Value {
    let line = format!("{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"{method}\"}}");
    serde_json::from_str(&dispatcher.handle_line(&line).to_line()).expect("a response is JSON")
}

#[test]
fn a_handler_that_panics_becomes_an_internal_error_and_the_session_lives() {
    let dispatcher = dispatcher();

    let response = call(&dispatcher, "test.panics");
    assert_eq!(response["error"]["code"], -32603);
    assert_eq!(response["error"]["data"]["status"], "INTERNAL");
    assert_eq!(response["error"]["data"]["reason"], "HANDLER_PANIC");
    assert!(
        response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("the handler fell over"),
        "the panic message must reach the caller: {}",
        response["error"]["message"]
    );
    let hint = response["error"]["data"]["details"][0]["hint"]
        .as_str()
        .unwrap();
    assert!(
        hint.contains("dispatch.rs"),
        "the panic location must be reported: {hint}"
    );

    let after = call(&dispatcher, "test.works");
    assert_eq!(
        after["result"]["data"]["fine"], true,
        "the session must survive a panic"
    );
}

#[test]
fn a_handler_that_runs_too_long_is_given_up_on_and_the_session_lives() {
    let dispatcher = dispatcher();
    let started = Instant::now();

    let response = call(&dispatcher, "test.hangs");
    let waited = started.elapsed();

    assert_eq!(response["error"]["data"]["status"], "DEADLINE_EXCEEDED");
    assert_eq!(response["error"]["data"]["reason"], "HANDLER_TIMEOUT");
    assert_eq!(response["error"]["code"], -32005);
    assert!(
        waited < Duration::from_secs(2),
        "the budget of 150 ms was not enforced: waited {waited:?}"
    );
    assert!(
        response["error"]["data"]["meta"]["duration_us"]
            .as_u64()
            .unwrap()
            >= 150_000,
        "meta must report the time that was really spent"
    );

    // The abandoned thread is still sleeping. The next call must not wait for it, and
    // must not receive its answer either.
    let started = Instant::now();
    let after = call(&dispatcher, "test.works");
    assert_eq!(after["result"]["data"]["fine"], true);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "a fresh worker must be started"
    );
}

#[test]
fn a_call_after_a_call_reuses_the_same_worker_and_stays_correct() {
    let dispatcher = dispatcher();
    for _ in 0..50 {
        let response = call(&dispatcher, "test.works");
        assert_eq!(response["result"]["data"]["fine"], true);
        assert_eq!(response["result"]["meta"]["api_version"], "1.0.0");
    }
}

#[test]
fn a_broken_registry_answers_every_call_instead_of_taking_the_process_down() {
    let problems = RegistryBuilder::new()
        .register::<Works>()
        .register::<Works>()
        .build()
        .expect_err("the same API twice is not a valid registry");
    let dispatcher = Dispatcher::broken(problems);

    let response = call(&dispatcher, "test.works");
    assert_eq!(response["error"]["data"]["status"], "INTERNAL");
    assert_eq!(response["error"]["data"]["reason"], "INVARIANT_BROKEN");
    assert!(dispatcher.registry().is_none());
}
