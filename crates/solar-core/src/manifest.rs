//! The manifest: everything this build can do, in one document.
//!
//! The manifest is generated from the registry, never written by hand. `cargo xtask
//! manifest` writes it to `manifest/solar.manifest.json`, `solar.manifest` returns it over
//! the protocol, and a test fails when the file and the generator disagree.
//!
//! # Shared definitions
//!
//! `schemars` gives every type a self-contained schema, with the types it refers to
//! repeated in a `$defs` of its own. Across five APIs that meant the same seven
//! definitions written twice, and it would mean the same definitions written fifty times
//! across fifty APIs.
//!
//! The manifest therefore hoists every definition into one `$defs` at its root, and the
//! schema of each API keeps the `$ref` that `schemars` already wrote, `#/$defs/Name`,
//! which now resolves against the manifest rather than against the schema. The schemas
//! are fragments of one document: [`standalone_schema`] puts one back together for a
//! consumer that wants a schema on its own, and section 8 of the contract says so.

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
pub const MANIFEST_SCHEMA_VERSION: &str = "2.1.0";

/// The dialect every schema in the manifest is written in.
///
/// It is stated once, at the root, because the schemas below it are fragments of this
/// document rather than schema resources of their own, and in JSON Schema 2020-12
/// `$schema` belongs to a resource root.
pub const SCHEMA_DIALECT: &str = "https://json-schema.org/draft/2020-12/schema";

/// Where a shared definition lives, as a `$ref` reads it.
const DEFS: &str = "$defs";

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

/// What the protocol this build speaks accepts, and every limit a caller must respect.
///
/// A manifest used to say what the APIs are and nothing about the protocol carrying them,
/// so a client could not tell whether batches were accepted without trying one. SOLAR
/// 0.1.0 and 0.2.0 answer under the same protocol name, `solar/1`, and differ in exactly
/// that. This section is how a client finds out by reading rather than by experiment.
///
/// Every number here is the constant the code enforces, not a copy of it: a test compares
/// the two, so a limit that changes without the manifest changing fails the suite.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Capabilities {
    /// Whether a line may hold an array of requests, and how many it may hold.
    pub batch: BatchCapability,
    /// Whether a call in flight can be asked to stop, and what does the asking.
    pub cancellation: CancellationCapability,
    /// Whether a message without `id` is accepted. It never is: section 3.1.
    pub notifications: NotificationCapability,
    /// The limits a caller has to respect, in the units the contract states them in.
    pub limits: Limits,
}

/// What this build does with a line holding an array of requests.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct BatchCapability {
    /// Whether an array of requests is answered rather than refused.
    pub accepted: bool,
    /// The most elements one batch may hold. A larger one is refused as a whole with
    /// `RESOURCE_EXHAUSTED` / `BATCH_TOO_LARGE`.
    pub max_elements: u64,
    /// Whether the responses come back in the order of the requests. SOLAR promises this;
    /// JSON-RPC 2.0 does not, so a client that speaks to both has to ask.
    pub ordered: bool,
}

/// What this build does when a caller stops waiting for a call.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct CancellationCapability {
    /// Whether a call in flight can be asked to stop at all.
    pub accepted: bool,
    /// The method that does the asking, `null` when cancellation is not accepted.
    pub method: Option<String>,
}

/// What this build does with a message that carries no `id`.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct NotificationCapability {
    /// Always false in `solar/1`: every call gets a response, so a message without an
    /// `id` is refused rather than acted on silently.
    pub accepted: bool,
}

/// Every limit a caller has to respect, in one place.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Limits {
    /// The longest request line, in bytes, not counting the newline.
    pub max_request_bytes: u64,
    /// The most unanswered requests a session holds before it refuses more.
    pub max_queued_requests: u64,
    /// The most bytes of unanswered request text a session holds.
    pub max_queued_bytes: u64,
    /// How many answered identifiers a session remembers, for `solar.cancel` to report
    /// `already_finished` rather than `unknown`.
    pub remembered_request_ids: u64,
    /// The most handlers that may be alive after overrunning their budget, before a new
    /// call is refused with `UNAVAILABLE` / `TOO_MANY_ABANDONED`.
    pub max_abandoned_workers: u64,
    /// The largest response an API may produce unless it declares a smaller one. Each
    /// API declares its own in `max_output_bytes`.
    pub default_max_output_bytes: u64,
}

/// What this build accepts and what it refuses, read from the code that enforces it.
#[must_use]
pub fn capabilities() -> Capabilities {
    Capabilities {
        batch: BatchCapability {
            accepted: true,
            max_elements: crate::protocol::MAX_BATCH_ELEMENTS as u64,
            ordered: true,
        },
        cancellation: CancellationCapability {
            accepted: true,
            method: Some(crate::session::CANCEL_METHOD.to_owned()),
        },
        notifications: NotificationCapability { accepted: false },
        limits: Limits {
            max_request_bytes: crate::protocol::MAX_REQUEST_BYTES as u64,
            max_queued_requests: crate::session::MAX_QUEUED_REQUESTS as u64,
            max_queued_bytes: crate::session::MAX_QUEUED_BYTES as u64,
            remembered_request_ids: crate::session::REMEMBERED_IDS as u64,
            max_abandoned_workers: crate::dispatch::MAX_ABANDONED_WORKERS as u64,
            default_max_output_bytes: crate::api::DEFAULT_MAX_OUTPUT_BYTES,
        },
    }
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
    /// The dialect every schema below is written in.
    pub schema_dialect: String,
    /// What the protocol accepts, and every limit a caller must respect.
    pub capabilities: Capabilities,
    /// The definitions the schemas share, by name. A `$ref` of `#/$defs/Name` in any
    /// schema of this document means the entry called `Name` here.
    #[serde(rename = "$defs")]
    pub defs: serde_json::Map<String, Value>,
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

/// Builds the whole manifest, with every definition hoisted into one `$defs`.
#[must_use]
pub fn build(registry: &Registry) -> Manifest {
    let apis: Vec<ManifestEntry> = registry
        .entries()
        .iter()
        .filter_map(|entry| entry_of(registry, entry.name()))
        .collect();
    assemble(apis)
}

/// Builds a manifest holding a single API, in the same shape as the whole document.
#[must_use]
pub fn build_one(registry: &Registry, name: &str) -> Option<Manifest> {
    Some(assemble(vec![entry_of(registry, name)?]))
}

/// Puts the document together: hoists the definitions, then names the versions.
///
/// Only the definitions the given APIs actually use come along, so a manifest narrowed
/// to one API does not carry the definitions of the others.
fn assemble(mut apis: Vec<ManifestEntry>) -> Manifest {
    let mut defs = serde_json::Map::new();
    for entry in &mut apis {
        hoist(&mut entry.params_schema, &mut defs);
        hoist(&mut entry.output_schema, &mut defs);
    }
    Manifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_owned(),
        protocol: PROTOCOL.to_owned(),
        solar_version: SOLAR_VERSION.to_owned(),
        schema_dialect: SCHEMA_DIALECT.to_owned(),
        capabilities: capabilities(),
        defs,
        apis,
    }
}

/// Moves the `$defs` of one schema into the shared map, and drops its `$schema`.
///
/// The `$ref`s inside the schema are left exactly as `schemars` wrote them, because
/// `#/$defs/Name` means the same thing once the definition sits at the root of the
/// document the schema is part of.
///
/// A definition already in the map under the same name must be identical, since two
/// schemas generated from the same Rust type always are. If it ever is not, the second
/// one is kept under a name of its own rather than silently overwriting the first.
fn hoist(schema: &mut Value, defs: &mut serde_json::Map<String, Value>) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    object.remove("$schema");

    let Some(own) = object.remove(DEFS).and_then(|d| match d {
        Value::Object(map) => Some(map),
        _ => None,
    }) else {
        return;
    };

    let mut renamed: Vec<(String, String)> = Vec::new();
    for (name, definition) in own {
        match defs.get(&name) {
            Some(existing) if *existing == definition => {}
            Some(_) => {
                // Two different types with the same name. Nothing generates this today,
                // and silently keeping one of them would be the worst possible answer.
                let mut unique = format!("{name}_2");
                let mut attempt = 2;
                while defs.contains_key(&unique) {
                    attempt += 1;
                    unique = format!("{name}_{attempt}");
                }
                renamed.push((name.clone(), unique.clone()));
                defs.insert(unique, definition);
            }
            None => {
                defs.insert(name, definition);
            }
        }
    }
    for (from, to) in renamed {
        repoint(schema, &format!("#/$defs/{from}"), &format!("#/$defs/{to}"));
    }
}

/// Rewrites every `$ref` equal to `from` into `to`, however deep it sits.
fn repoint(node: &mut Value, from: &str, to: &str) {
    match node {
        Value::Object(members) => {
            if members.get("$ref").and_then(Value::as_str) == Some(from) {
                members.insert("$ref".to_owned(), Value::String(to.to_owned()));
            }
            for (_, member) in members.iter_mut() {
                repoint(member, from, to);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|item| repoint(item, from, to)),
        _ => {}
    }
}

/// One schema of the manifest, put back together as a document of its own.
///
/// The schemas in the manifest are fragments: their `$ref`s point at the `$defs` of the
/// manifest. This gives back a schema a validator can compile on its own, by putting the
/// dialect and the definitions back. Every definition comes along, because working out
/// which ones a schema reaches is not worth the risk of missing one.
///
/// `which` is `params_schema` or `output_schema`.
#[must_use]
pub fn standalone_schema(manifest: &Manifest, api: &str, which: &str) -> Option<Value> {
    let entry = manifest.apis.iter().find(|entry| entry.name == api)?;
    let fragment = match which {
        "params_schema" => &entry.params_schema,
        "output_schema" => &entry.output_schema,
        _ => return None,
    };
    let mut schema = fragment.clone();
    let object = schema.as_object_mut()?;
    object.insert(
        "$schema".to_owned(),
        Value::String(manifest.schema_dialect.clone()),
    );
    if !manifest.defs.is_empty() {
        object.insert(DEFS.to_owned(), Value::Object(manifest.defs.clone()));
    }
    Some(schema)
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
    fn the_capabilities_are_the_numbers_the_code_enforces() {
        // Not a copy of the constants: the point of this test is that a limit which moves
        // without the manifest moving fails here rather than misleading a client.
        let capabilities = capabilities();

        assert!(capabilities.batch.accepted);
        assert!(capabilities.batch.ordered, "section 3.2 promises the order");
        assert_eq!(
            capabilities.batch.max_elements,
            crate::protocol::MAX_BATCH_ELEMENTS as u64
        );

        assert!(capabilities.cancellation.accepted);
        assert_eq!(
            capabilities.cancellation.method.as_deref(),
            Some(crate::session::CANCEL_METHOD)
        );

        assert!(
            !capabilities.notifications.accepted,
            "the one deliberate deviation of section 3.1"
        );

        let limits = &capabilities.limits;
        assert_eq!(
            limits.max_request_bytes,
            crate::protocol::MAX_REQUEST_BYTES as u64
        );
        assert_eq!(
            limits.max_queued_requests,
            crate::session::MAX_QUEUED_REQUESTS as u64
        );
        assert_eq!(
            limits.max_queued_bytes,
            crate::session::MAX_QUEUED_BYTES as u64
        );
        assert_eq!(
            limits.remembered_request_ids,
            crate::session::REMEMBERED_IDS as u64
        );
        assert_eq!(
            limits.max_abandoned_workers,
            crate::dispatch::MAX_ABANDONED_WORKERS as u64
        );
        assert_eq!(
            limits.default_max_output_bytes,
            crate::api::DEFAULT_MAX_OUTPUT_BYTES
        );
    }

    #[test]
    fn the_capabilities_a_client_reads_name_the_method_that_cancels() {
        // A client is told which method cancels rather than having to know the name, so
        // that a later protocol can move it without breaking a reader of the manifest.
        let registry = RegistryBuilder::new().build().unwrap();
        let manifest = build(&registry);
        let value = serde_json::to_value(&manifest).unwrap();

        assert_eq!(
            value["capabilities"]["cancellation"]["method"],
            serde_json::json!("solar.cancel")
        );
        assert_eq!(value["capabilities"]["batch"]["max_elements"], 64);
        assert_eq!(value["capabilities"]["notifications"]["accepted"], false);
        assert_eq!(
            value["capabilities"]["limits"]["max_request_bytes"],
            16_777_216
        );
    }

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

    /// A schema as `schemars` writes one: self-contained, with its own dialect and its
    /// own definitions.
    fn self_contained() -> Value {
        serde_json::json!({
            "$schema": SCHEMA_DIALECT,
            "type": "object",
            "properties": {"thing": {"$ref": "#/$defs/Thing"}},
            "$defs": {"Thing": {"type": "string"}},
        })
    }

    #[test]
    fn hoisting_moves_the_definitions_and_leaves_the_references_alone() {
        let mut schema = self_contained();
        let mut defs = serde_json::Map::new();
        hoist(&mut schema, &mut defs);

        assert!(
            schema.get("$defs").is_none(),
            "the schema keeps no definitions of its own"
        );
        assert!(
            schema.get("$schema").is_none(),
            "nor a dialect: the document states it once"
        );
        assert_eq!(
            schema["properties"]["thing"]["$ref"], "#/$defs/Thing",
            "the reference is untouched"
        );
        assert_eq!(defs["Thing"], serde_json::json!({"type": "string"}));
    }

    #[test]
    fn the_same_definition_from_two_schemas_is_stored_once() {
        let mut defs = serde_json::Map::new();
        let (mut first, mut second) = (self_contained(), self_contained());
        hoist(&mut first, &mut defs);
        hoist(&mut second, &mut defs);

        assert_eq!(
            defs.len(),
            1,
            "two schemas from the same type share one definition"
        );
        assert_eq!(second["properties"]["thing"]["$ref"], "#/$defs/Thing");
    }

    #[test]
    fn two_different_definitions_under_one_name_are_both_kept() {
        let mut defs = serde_json::Map::new();
        let mut first = self_contained();
        hoist(&mut first, &mut defs);

        // The same name, a different definition: nothing generates this today, and
        // overwriting one of them silently would be the worst possible answer.
        let mut second = serde_json::json!({
            "type": "object",
            "properties": {"thing": {"$ref": "#/$defs/Thing"}},
            "$defs": {"Thing": {"type": "integer"}},
        });
        hoist(&mut second, &mut defs);

        assert_eq!(defs.len(), 2, "both definitions are kept");
        assert_eq!(defs["Thing"], serde_json::json!({"type": "string"}));
        assert_eq!(defs["Thing_2"], serde_json::json!({"type": "integer"}));
        assert_eq!(
            second["properties"]["thing"]["$ref"], "#/$defs/Thing_2",
            "and the second schema points at its own"
        );
        assert_eq!(
            first["properties"]["thing"]["$ref"], "#/$defs/Thing",
            "while the first still points at the first"
        );
    }

    #[test]
    fn a_schema_that_shares_nothing_survives_hoisting_unchanged() {
        let mut defs = serde_json::Map::new();
        let mut plain = serde_json::json!({"type": "object", "properties": {}});
        hoist(&mut plain, &mut defs);
        assert!(defs.is_empty());
        assert_eq!(
            plain,
            serde_json::json!({"type": "object", "properties": {}})
        );

        // Something that is not an object at all is left alone rather than panicking.
        let mut odd = Value::Bool(true);
        hoist(&mut odd, &mut defs);
        assert_eq!(odd, Value::Bool(true));
    }

    #[test]
    fn repointing_finds_every_reference_however_deep() {
        let mut node = serde_json::json!({
            "a": {"$ref": "#/$defs/Old"},
            "b": [{"c": {"$ref": "#/$defs/Old"}}, {"$ref": "#/$defs/Other"}],
        });
        repoint(&mut node, "#/$defs/Old", "#/$defs/New");
        assert_eq!(node["a"]["$ref"], "#/$defs/New");
        assert_eq!(node["b"][0]["c"]["$ref"], "#/$defs/New");
        assert_eq!(
            node["b"][1]["$ref"], "#/$defs/Other",
            "and leaves the others alone"
        );
    }

    #[test]
    fn a_schema_is_put_back_together_with_the_dialect_and_the_definitions() {
        let registry = RegistryBuilder::new().build().unwrap();
        let mut manifest = build(&registry);
        manifest
            .defs
            .insert("Thing".to_owned(), serde_json::json!({"type": "string"}));
        manifest.apis.push(ManifestEntry {
            name: "a.b".to_owned(),
            version: "1.0.0".to_owned(),
            spec: crate::api::ApiSpec {
                summary: "s",
                description: "d",
                errors: Vec::new(),
                side_effects: vec![crate::api::SideEffect::None],
                idempotent: true,
                stability: crate::api::Stability::Experimental,
                since: "0.1.0",
                timeout_ms: 1,
                max_output_bytes: crate::api::DEFAULT_MAX_OUTPUT_BYTES,
                examples: Vec::new(),
            },
            params_schema: serde_json::json!({"$ref": "#/$defs/Thing"}),
            output_schema: serde_json::json!({"type": "object"}),
        });

        let standalone = standalone_schema(&manifest, "a.b", "params_schema").unwrap();
        assert_eq!(standalone["$schema"], SCHEMA_DIALECT);
        assert_eq!(
            standalone["$defs"]["Thing"],
            serde_json::json!({"type": "string"})
        );
        assert_eq!(
            standalone["$ref"], "#/$defs/Thing",
            "the fragment itself is unchanged"
        );

        assert!(standalone_schema(&manifest, "no.such_api", "params_schema").is_none());
        assert!(standalone_schema(&manifest, "a.b", "neither_schema").is_none());
    }

    #[test]
    fn a_manifest_of_one_api_carries_only_what_that_api_uses() {
        // An empty registry has no definitions at all, which is the case that proves the
        // block is built from the APIs rather than from everything that exists.
        let registry = RegistryBuilder::new().build().unwrap();
        let manifest = build(&registry);
        assert!(manifest.defs.is_empty());
        assert_eq!(manifest.schema_dialect, SCHEMA_DIALECT);
    }

    #[test]
    fn rendering_is_stable_between_two_runs() {
        let registry = RegistryBuilder::new().build().unwrap();
        assert_eq!(render(&registry), render(&registry));
    }
}
