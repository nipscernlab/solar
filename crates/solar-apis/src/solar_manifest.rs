//! `solar.manifest`: everything this build can do, in one document.

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use solar_core::api::{ANY, Api, ApiSpec, ErrorSpec, Example, SideEffect, Stability};
use solar_core::context::Context;
use solar_core::error::SolarError;
use solar_core::manifest::{MANIFEST_SCHEMA_VERSION, Manifest, build, build_one};
use solar_core::reason::Reason;

/// The parameters of `solar.manifest`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// One method name, to get a manifest holding only that API. Absent means every API.
    #[serde(default)]
    pub api: Option<String>,
}

/// Returns the manifest, whole or narrowed to one API.
#[derive(Debug)]
pub struct SolarManifest;

impl Api for SolarManifest {
    const NAME: &'static str = "solar.manifest";
    const VERSION: &'static str = "1.1.0";
    type Params = Params;
    type Output = Manifest;

    fn spec() -> ApiSpec {
        ApiSpec {
            summary: "Returns the manifest of every API this build answers to",
            description: "\
The manifest is the whole surface of SOLAR in one document: every API, its version, what \
it does, what it may fail with, what it touches, its timeout, the JSON Schemas of its \
parameters and of its output, and its examples. It is generated from the registry and \
never written by hand, so it cannot describe something that is not there.\n\n\
With `api`, the answer keeps the same shape and `apis` holds that one entry, which is \
easier for a caller to handle than a second shape. `solar.describe` returns the bare entry \
instead, for when the document around it is in the way.\n\n\
The file `manifest/solar.manifest.json` in the repository is this exact document, and a \
test fails when the two disagree.",
            errors: vec![ErrorSpec::of(Reason::ApiNotFound)],
            side_effects: vec![SideEffect::None],
            idempotent: true,
            stability: Stability::Experimental,
            since: "0.1.0",
            timeout_ms: 2_000,
            examples: vec![
                Example::subset(
                    "everything",
                    "The whole manifest",
                    json!({}),
                    json!({
                        "schema_version": MANIFEST_SCHEMA_VERSION,
                        "protocol": "solar/1",
                        "solar_version": ANY,
                        "apis": ANY
                    }),
                ),
                Example::subset(
                    "one_api",
                    "A manifest narrowed to a single API",
                    json!({"api": "solar.ping"}),
                    json!({"apis": [{"name": "solar.ping", "version": ANY}]}),
                ),
            ],
        }
    }

    fn call(ctx: &Context, params: Params) -> Result<Manifest, SolarError> {
        match params.api {
            None => Ok(build(ctx.registry())),
            Some(name) => build_one(ctx.registry(), &name)
                .ok_or_else(|| ctx.registry().not_found(&name, Reason::ApiNotFound, "/api")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::context;

    #[test]
    fn the_whole_manifest_lists_every_registered_api() {
        let ctx = context("solar.manifest");
        let manifest = SolarManifest::call(&ctx, Params { api: None }).unwrap();
        assert_eq!(manifest.apis.len(), ctx.registry().len());
        assert_eq!(manifest.protocol, "solar/1");
        assert!(manifest.apis.iter().any(|entry| entry.name == "solar.ping"));
    }

    #[test]
    fn one_api_comes_back_in_the_same_shape_as_the_whole_document() {
        let ctx = context("solar.manifest");
        let manifest = SolarManifest::call(
            &ctx,
            Params {
                api: Some("solar.ping".to_owned()),
            },
        )
        .unwrap();
        assert_eq!(manifest.apis.len(), 1);
        assert_eq!(manifest.apis[0].name, "solar.ping");
        assert_eq!(manifest.schema_version, MANIFEST_SCHEMA_VERSION);
    }

    #[test]
    fn a_name_nothing_answers_to_is_not_found_and_the_closest_one_is_offered() {
        let ctx = context("solar.manifest");
        let error = SolarManifest::call(
            &ctx,
            Params {
                api: Some("solar.pign".to_owned()),
            },
        )
        .unwrap_err();
        assert_eq!(error.reason(), Reason::ApiNotFound);
        assert_eq!(error.details()[0].expected.as_deref(), Some("solar.ping"));
        assert_eq!(error.details()[0].field.as_deref(), Some("/api"));
    }
}
