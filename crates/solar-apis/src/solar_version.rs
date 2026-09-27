//! `solar.version`: which SOLAR is answering, built from what, by which compiler.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use solar_core::api::{ANY, Api, ApiSpec, Example, SideEffect, Stability};
use solar_core::build_info::{BuildInfo, build_info};
use solar_core::context::Context;
use solar_core::error::SolarError;
use solar_core::manifest::MANIFEST_SCHEMA_VERSION;
use solar_core::meta::{PROTOCOL, SOLAR_VERSION};
use solar_core::warning::WarningCode;

/// The parameters of `solar.version`, of which there are none.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Params {}

/// The output of `solar.version`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Output {
    /// The version of this SOLAR build.
    pub solar_version: String,
    /// The protocol it speaks.
    pub protocol: String,
    /// The layout version of the manifest it produces.
    pub manifest_schema_version: String,
    /// Which commit, which compiler, which target.
    pub build: BuildInfo,
}

/// Reports the version of SOLAR and how this binary was built.
#[derive(Debug)]
pub struct SolarVersion;

impl Api for SolarVersion {
    const NAME: &'static str = "solar.version";
    const VERSION: &'static str = "1.0.0";
    type Params = Params;
    type Output = Output;

    fn spec() -> ApiSpec {
        ApiSpec {
            summary: "Reports the version, the protocol and the build metadata",
            description: "\
Three versions travel together and mean different things. `solar_version` is this build. \
`protocol` is the shape of the messages, and it only changes when an existing message \
changes in a way that breaks a caller. `manifest_schema_version` is the layout of the \
manifest, and a consumer must refuse a manifest whose major differs from the one it was \
written against.\n\n\
`build` says where the binary came from: the Git commit, whether the working tree was \
clean at the time, the compiler, the profile and the target triple. A field that could not \
be determined, because the build had no Git repository or no Git, is the string `unknown` \
rather than a plausible guess, and the call then warns with BUILD_METADATA_INCOMPLETE. \
Quote all of it in a bug report.",
            errors: Vec::new(),
            side_effects: vec![SideEffect::None],
            idempotent: true,
            stability: Stability::Experimental,
            since: "0.1.0",
            timeout_ms: 1_000,
            examples: vec![Example::subset(
                "plain",
                "Everything this build is",
                json!({}),
                json!({
                    "protocol": "solar/1",
                    "manifest_schema_version": MANIFEST_SCHEMA_VERSION,
                    "solar_version": ANY,
                    "build": {"target": ANY, "profile": ANY, "rustc_version": ANY}
                }),
            )],
        }
    }

    fn call(ctx: &Context, _params: Params) -> Result<Output, SolarError> {
        let build = build_info();
        if !build.is_complete() {
            ctx.warn(
                WarningCode::BuildMetadataIncomplete,
                format!(
                    "This build could not record {}; the binary was built outside a Git \
                     working tree or without Git available.",
                    build.missing().join(", ")
                ),
            );
        }
        Ok(Output {
            solar_version: SOLAR_VERSION.to_owned(),
            protocol: PROTOCOL.to_owned(),
            manifest_schema_version: MANIFEST_SCHEMA_VERSION.to_owned(),
            build,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::context;

    #[test]
    fn the_three_versions_are_reported_together() {
        let ctx = context("solar.version");
        let output = SolarVersion::call(&ctx, Params {}).unwrap();
        assert_eq!(output.protocol, "solar/1");
        assert_eq!(output.solar_version, SOLAR_VERSION);
        assert_eq!(output.manifest_schema_version, MANIFEST_SCHEMA_VERSION);
        assert_ne!(output.build.target, "unknown");
    }

    #[test]
    fn an_incomplete_build_warns_rather_than_inventing_a_commit() {
        let ctx = context("solar.version");
        let output = SolarVersion::call(&ctx, Params {}).unwrap();
        let warned = ctx
            .warnings()
            .iter()
            .any(|warning| warning.code == WarningCode::BuildMetadataIncomplete);
        assert_eq!(
            warned,
            !output.build.is_complete(),
            "the warning and the metadata must tell the same story"
        );
    }
}
