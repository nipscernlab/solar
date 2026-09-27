//! `solar.describe`: everything about one API.

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use solar_core::api::{
    ANY, Api, ApiSpec, DEFAULT_MAX_OUTPUT_BYTES, ErrorSpec, Example, SideEffect, Stability,
};
use solar_core::context::Context;
use solar_core::error::SolarError;
use solar_core::manifest::{ManifestEntry, entry_of};
use solar_core::reason::Reason;

/// The parameters of `solar.describe`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The method name to describe, for example `solar.ping`.
    pub api: String,
}

/// Describes one API.
#[derive(Debug)]
pub struct SolarDescribe;

impl Api for SolarDescribe {
    const NAME: &'static str = "solar.describe";
    const VERSION: &'static str = "1.0.0";
    type Params = Params;
    type Output = ManifestEntry;

    fn spec() -> ApiSpec {
        ApiSpec {
            summary: "Describes one API, with its schemas and its examples",
            description: "\
The entry of a single API, exactly as it appears in the manifest: the summary and the \
description, the failures it declares, what it touches, whether it is idempotent, its \
timeout, the JSON Schemas of its parameters and of its output, and examples that really \
run.\n\n\
This is the call to make before making any other call. An agent that reads the entry \
first does not have to guess a parameter name, and a name that does not exist comes back \
with the closest ones that do.",
            errors: vec![ErrorSpec::of(Reason::ApiNotFound)],
            side_effects: vec![SideEffect::None],
            idempotent: true,
            stability: Stability::Experimental,
            since: "0.1.0",
            timeout_ms: 2_000,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            examples: vec![Example::subset(
                "describe_ping",
                "Everything about solar.ping",
                json!({"api": "solar.ping"}),
                json!({
                    "name": "solar.ping",
                    "version": ANY,
                    "stability": "experimental",
                    "idempotent": true,
                    "params_schema": ANY,
                    "output_schema": ANY
                }),
            )],
        }
    }

    fn call(ctx: &Context, params: Params) -> Result<ManifestEntry, SolarError> {
        entry_of(ctx.registry(), &params.api).ok_or_else(|| {
            ctx.registry()
                .not_found(&params.api, Reason::ApiNotFound, "/api")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::context;

    #[test]
    fn an_api_describes_itself_down_to_its_schemas() {
        let entry = SolarDescribe::call(
            &context("solar.describe"),
            Params {
                api: "solar.ping".to_owned(),
            },
        )
        .unwrap();
        assert_eq!(entry.name, "solar.ping");
        assert_eq!(entry.params_schema["additionalProperties"], json!(false));
        assert!(!entry.spec.examples.is_empty());
        assert!(entry.spec.timeout_ms > 0);
    }

    #[test]
    fn describing_something_that_is_not_there_suggests_what_is() {
        let error = SolarDescribe::call(
            &context("solar.describe"),
            Params {
                api: "solar.pign".to_owned(),
            },
        )
        .unwrap_err();
        assert_eq!(error.reason(), Reason::ApiNotFound);
        assert!(
            error
                .details()
                .iter()
                .any(|d| d.expected.as_deref() == Some("solar.ping"))
        );
    }
}
