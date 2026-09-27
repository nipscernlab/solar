//! The manifest: everything this build can do, in one document.
//!
//! The manifest is generated from the registry, never written by hand. `cargo xtask
//! manifest` writes it to `manifest/solar.manifest.json`, `solar.manifest` returns it over
//! the protocol, and a test fails when the file and the generator disagree.

use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;

use crate::api::ApiSpec;
use crate::meta::{PROTOCOL, SOLAR_VERSION};
use crate::registry::Registry;

/// The version of the manifest layout itself.
///
/// A consumer must reject a manifest whose major differs from the one it was written
/// against: a major bump means members moved or changed meaning.
pub const MANIFEST_SCHEMA_VERSION: &str = "1.0.0";

/// One API as it appears in the manifest.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct ManifestEntry {
    /// The method name.
    pub name: String,
    /// The semantic version of this API.
    pub version: String,
    /// Everything the API says about itself.
    #[serde(flatten)]
    pub spec: ApiSpec,
    /// JSON Schema 2020-12 of the parameters.
    pub params_schema: Value,
    /// JSON Schema 2020-12 of the output.
    pub output_schema: Value,
}

/// Every API this build answers to, with its schemas and its examples.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Manifest {
    /// The version of the manifest layout.
    pub schema_version: String,
    /// The protocol these APIs speak.
    pub protocol: String,
    /// The SOLAR version that produced this manifest.
    pub solar_version: String,
    /// The APIs, sorted by name.
    pub apis: Vec<ManifestEntry>,
}

/// Builds the entry of one API.
#[must_use]
pub fn entry_of(registry: &Registry, name: &str) -> Option<ManifestEntry> {
    let entry = registry.get(name)?;
    Some(ManifestEntry {
        name: entry.name().to_owned(),
        version: entry.version().to_owned(),
        spec: entry.spec().clone(),
        params_schema: entry.params_schema().clone(),
        output_schema: entry.output_schema().clone(),
    })
}

/// Builds the whole manifest.
#[must_use]
pub fn build(registry: &Registry) -> Manifest {
    let apis = registry
        .entries()
        .iter()
        .filter_map(|entry| entry_of(registry, entry.name()))
        .collect();
    Manifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_owned(),
        protocol: PROTOCOL.to_owned(),
        solar_version: SOLAR_VERSION.to_owned(),
        apis,
    }
}

/// Builds a manifest holding a single API, in the same shape as the whole document.
#[must_use]
pub fn build_one(registry: &Registry, name: &str) -> Option<Manifest> {
    let entry = entry_of(registry, name)?;
    Some(Manifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_owned(),
        protocol: PROTOCOL.to_owned(),
        solar_version: SOLAR_VERSION.to_owned(),
        apis: vec![entry],
    })
}

/// The exact text of `manifest/solar.manifest.json`.
///
/// Members are sorted, the indentation is two spaces and the file ends with a newline, so
/// that the file a generator writes and the file a repository holds can be compared byte
/// for byte on every platform.
#[must_use]
pub fn render(registry: &Registry) -> String {
    let manifest = build(registry);
    let value = serde_json::to_value(&manifest).unwrap_or(Value::Null);
    let mut text = serde_json::to_string_pretty(&value).unwrap_or_default();
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::RegistryBuilder;

    #[test]
    fn an_empty_registry_still_produces_a_valid_manifest() {
        let registry = RegistryBuilder::new().build().unwrap();
        let manifest = build(&registry);
        assert_eq!(manifest.schema_version, MANIFEST_SCHEMA_VERSION);
        assert_eq!(manifest.protocol, "solar/1");
        assert!(manifest.apis.is_empty());

        let text = render(&registry);
        assert!(text.ends_with("}\n"), "the file always ends with a newline");
        let parsed: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["solar_version"], SOLAR_VERSION);
    }

    #[test]
    fn rendering_is_stable_between_two_runs() {
        let registry = RegistryBuilder::new().build().unwrap();
        assert_eq!(render(&registry), render(&registry));
    }
}
