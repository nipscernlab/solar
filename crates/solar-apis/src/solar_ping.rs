//! `solar.ping`: is SOLAR there, and how long does a round trip take?

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use solar_core::api::{
    ANY, Api, ApiSpec, DEFAULT_MAX_OUTPUT_BYTES, Example, SideEffect, Stability,
};
use solar_core::clock;
use solar_core::context::Context;
use solar_core::error::SolarError;

/// The parameters of `solar.ping`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Anything the caller wants back, to tell one ping from another.
    #[serde(default)]
    pub message: Option<String>,
}

/// The output of `solar.ping`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Output {
    /// Always `true`. A caller that sees this saw a whole, well formed response.
    pub pong: bool,
    /// The message that arrived, `null` when there was none.
    pub echo: Option<String>,
    /// When SOLAR handled the ping: RFC 3339, UTC, microseconds.
    pub received_at: String,
}

/// Answers, and reports when the answer was made.
#[derive(Debug)]
pub struct SolarPing;

impl Api for SolarPing {
    const NAME: &'static str = "solar.ping";
    const VERSION: &'static str = "1.0.0";
    type Params = Params;
    type Output = Output;

    fn spec() -> ApiSpec {
        ApiSpec {
            summary: "Answers immediately, to prove SOLAR is there",
            description: "\
The cheapest call SOLAR has. It touches nothing, reads nothing and starts nothing, so the \
time it takes is the time the protocol itself costs: the round trip of a line through a \
pipe plus dispatch. That makes it the call to measure with, and the call to reach for when \
something is not answering and the question is whether SOLAR is alive at all.\n\n\
Whatever is passed in `message` comes back in `echo`, untouched, which is how a caller \
tells one ping from another when several are in flight.",
            errors: Vec::new(),
            side_effects: vec![SideEffect::None],
            idempotent: true,
            stability: Stability::Experimental,
            since: "0.1.0",
            timeout_ms: 1_000,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            examples: vec![
                Example::exact(
                    "bare",
                    "A ping with no message at all",
                    json!({}),
                    json!({"pong": true, "echo": null, "received_at": ANY}),
                ),
                Example::exact(
                    "with_a_message",
                    "A ping that carries something to echo",
                    json!({"message": "hi"}),
                    json!({"pong": true, "echo": "hi", "received_at": ANY}),
                ),
            ],
        }
    }

    fn call(_ctx: &Context, params: Params) -> Result<Output, SolarError> {
        Ok(Output {
            pong: true,
            echo: params.message,
            received_at: clock::now_rfc3339_micros(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::context;

    #[test]
    fn a_ping_answers_with_the_message_it_was_given() {
        let output = SolarPing::call(
            &context("solar.ping"),
            Params {
                message: Some("hi".to_owned()),
            },
        )
        .unwrap();
        assert!(output.pong);
        assert_eq!(output.echo.as_deref(), Some("hi"));
        assert!(output.received_at.ends_with('Z'));
    }

    #[test]
    fn a_ping_without_a_message_echoes_nothing() {
        let output = SolarPing::call(&context("solar.ping"), Params { message: None }).unwrap();
        assert_eq!(output.echo, None);
    }
}
