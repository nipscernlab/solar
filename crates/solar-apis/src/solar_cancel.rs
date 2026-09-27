//! `solar.cancel`: stop waiting for a call that is queued or running.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use solar_core::api::{
    ANY, Api, ApiSpec, DEFAULT_MAX_OUTPUT_BYTES, ErrorSpec, Example, SideEffect, Stability,
};
use solar_core::cancel::CancelOutcome;
use solar_core::context::Context;
use solar_core::error::SolarError;
use solar_core::protocol::RequestId;

/// The parameters of `solar.cancel`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The `id` of the call to cancel, exactly as it was sent: a number or a string.
    pub id: RequestId,
}

/// The output of `solar.cancel`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Output {
    /// The `id` that was asked about, echoed so an answer stands on its own.
    pub id: RequestId,
    /// What asking did. Nothing here is a failure: a call that had already finished, and
    /// an `id` nothing is using, are both answers rather than errors.
    pub outcome: CancelOutcome,
}

/// Asks a call to stop, and reports what that did.
#[derive(Debug)]
pub struct SolarCancel;

impl Api for SolarCancel {
    const NAME: &'static str = "solar.cancel";
    const VERSION: &'static str = "1.0.0";
    type Params = Params;
    type Output = Output;

    fn spec() -> ApiSpec {
        ApiSpec {
            summary: "Asks the call with this id to stop, and says what that did",
            description: "\
SOLAR accepts no notifications, so cancelling is an ordinary call rather than a special \
message. It is the one call a session answers the moment it is read, instead of in its \
turn: a cancellation that waited behind the call it is cancelling would be useless.\n\n\
Cancelling is a request, not a command. A call that had not started is removed and \
answered at once with CANCELLED. A call that is running is told, and it ends with its own \
result or with CANCELLED, whichever it reaches first, because a handler checks its token \
at points where stopping is safe. Either way the cancelled call gets exactly one \
response.\n\n\
Nothing here is an error. An id that finished before the cancellation arrived answers \
`already_finished`, and an id this session has never seen answers `unknown`, which is also \
what a call outside a session answers, since there is nothing there to cancel. A session \
remembers the ids of its last 1024 answered calls, so an older one is reported `unknown` \
rather than `already_finished`.",
            errors: vec![ErrorSpec::of(solar_core::reason::Reason::TypeMismatch)],
            side_effects: vec![SideEffect::None],
            idempotent: true,
            stability: Stability::Experimental,
            since: "0.2.0",
            timeout_ms: 1_000,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            examples: vec![Example::exact(
                "an_id_nothing_is_using",
                "Cancelling an id no call in this session carries",
                json!({"id": 4321}),
                json!({"id": ANY, "outcome": "unknown"}),
            )],
        }
    }

    fn call(ctx: &Context, params: Params) -> Result<Output, SolarError> {
        let outcome = ctx
            .session()
            .map_or(CancelOutcome::Unknown, |session| session.cancel(&params.id));
        Ok(Output {
            id: params.id,
            outcome,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::context;

    fn id(number: i64) -> RequestId {
        RequestId::Number(number.into())
    }

    #[test]
    fn a_call_outside_a_session_has_nothing_to_cancel() {
        let output = SolarCancel::call(&context("solar.cancel"), Params { id: id(1) }).unwrap();
        assert_eq!(output.outcome, CancelOutcome::Unknown);
        assert_eq!(output.id, id(1));
    }

    #[test]
    fn the_id_is_echoed_exactly_as_it_arrived() {
        let output = SolarCancel::call(
            &context("solar.cancel"),
            Params {
                id: RequestId::Text("a-string-id".to_owned()),
            },
        )
        .unwrap();
        assert_eq!(output.id, RequestId::Text("a-string-id".to_owned()));
    }
}
