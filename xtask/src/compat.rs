//! `cargo xtask compat <base>`: what changed in the manifest, and what version it needs.
//!
//! This is what `buf breaking` does for Protocol Buffers, applied to
//! `manifest/solar.manifest.json`. The manifest is the whole surface of SOLAR, so
//! comparing it with the manifest of an earlier commit says exactly what a caller written
//! against that commit would notice.
//!
//! Every difference is classified as **additive**, which needs a minor version of that
//! API, or **breaking**, which needs a major version. The task then checks the versions
//! really were bumped, and fails when they were not. CI runs it on every pull request,
//! which is the moment the decision is still cheap.
//!
//! What is compared is the manifest at `HEAD`, regenerated from the registry so that a
//! stale file cannot hide a change, against the manifest as it was at `<base>`.
//!
//! # References are resolved first
//!
//! The manifest shares its definitions: a schema says `$ref: "#/$defs/Reason"` rather
//! than repeating the enumeration. Comparing the text of two `$ref`s would miss a change
//! *inside* a shared definition, and would call moving a definition into `$defs` a
//! breaking change. Both manifests are therefore resolved before anything is compared,
//! so that a change to one shared definition is reported for every API that uses it.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

/// One difference between two manifests.
#[derive(Debug)]
struct Change {
    /// The API it is about, or `the manifest` for a change to the document itself.
    api: String,
    /// What changed, in one line.
    what: String,
    /// Whether a caller written against the base would notice.
    breaking: bool,
}

/// Compares the manifest at `HEAD` with the manifest at `base`.
///
/// # Errors
///
/// Returns the breaking changes that came without a major version, the additive changes
/// that came without a minor one, or why the comparison could not be made.
pub(crate) fn run(root: &Path, base: &str) -> Result<(), String> {
    let before = resolve_all(&manifest_at(root, base)?);
    let after = resolve_all(&current_manifest(root)?);

    let changes = differences(&before, &after);
    report(
        base,
        &changes,
        &apis_by_name(&before),
        &apis_by_name(&after),
    )
}

/// Every difference between two resolved manifests, classified.
///
/// Separated from the reading and the printing so that it can be tested against two
/// documents written by hand, which is how the rules below are pinned.
fn differences(before: &Value, after: &Value) -> Vec<Change> {
    let old_apis = apis_by_name(before);
    let new_apis = apis_by_name(after);
    let mut changes = Vec::new();

    for (name, old) in &old_apis {
        match new_apis.get(name) {
            None => changes.push(Change {
                api: name.clone(),
                what: "the API was removed or renamed".to_owned(),
                breaking: true,
            }),
            Some(new) => compare_api(name, old, new, &mut changes),
        }
    }
    for name in new_apis.keys() {
        if !old_apis.contains_key(name) {
            changes.push(Change {
                api: name.clone(),
                what: "a new API".to_owned(),
                breaking: false,
            });
        }
    }

    if before.get("schema_version") != after.get("schema_version") {
        let (was, now) = (
            text(before, "schema_version"),
            text(after, "schema_version"),
        );
        // Record 22: a consumer refuses a manifest whose **major** differs from the one it
        // was written against, and reads one whose minor moved. A minor bump is therefore
        // additive here, which is what makes it possible to add a member to the document.
        let breaking = major_of(&was) != major_of(&now);
        changes.push(Change {
            api: "the manifest".to_owned(),
            what: format!("the layout version went from {was} to {now}"),
            breaking,
        });
    }

    changes
}

/// The major of a semantic version, or the whole string when it has no dot.
///
/// A version this cannot read is treated as its own major, so an unreadable one differs
/// from everything and is reported as breaking rather than quietly ignored.
fn major_of(version: &str) -> &str {
    version.split_once('.').map_or(version, |(major, _)| major)
}

/// Compares one API with its earlier self.
fn compare_api(name: &str, old: &Value, new: &Value, changes: &mut Vec<Change>) {
    // Parameters: a member that disappears, changes type, or becomes required, breaks a
    // caller. A new optional member does not.
    compare_schema(
        name,
        "params",
        old.get("params_schema"),
        new.get("params_schema"),
        changes,
    );
    // Output: a member that disappears or changes type breaks a reader. A new one does
    // not, because a reader ignores what it does not know.
    compare_schema(
        name,
        "output",
        old.get("output_schema"),
        new.get("output_schema"),
        changes,
    );

    let old_errors = declared_errors(old);
    let new_errors = declared_errors(new);
    for failure in &new_errors {
        if !old_errors.contains(failure) {
            changes.push(Change {
                api: name.to_owned(),
                what: format!("the API may now return {failure}"),
                breaking: false,
            });
        }
    }
    for failure in &old_errors {
        if !new_errors.contains(failure) {
            // A caller may be matching on it; the API promised it and no longer does.
            changes.push(Change {
                api: name.to_owned(),
                what: format!("the API no longer declares {failure}"),
                breaking: true,
            });
        }
    }

    if text(old, "stability") != text(new, "stability") {
        changes.push(Change {
            api: name.to_owned(),
            what: format!(
                "stability went from {} to {}",
                text(old, "stability"),
                text(new, "stability")
            ),
            breaking: text(new, "stability") == "deprecated",
        });
    }
}

/// Compares two schemas of the same API, member by member.
fn compare_schema(
    api: &str,
    which: &str,
    old: Option<&Value>,
    new: Option<&Value>,
    changes: &mut Vec<Change>,
) {
    let (Some(old), Some(new)) = (old, new) else {
        return;
    };
    let old_members = properties(old);
    let new_members = properties(new);
    let old_required = required(old);
    let new_required = required(new);

    for (member, old_type) in &old_members {
        match new_members.get(member) {
            None => changes.push(Change {
                api: api.to_owned(),
                what: format!("the {which} member {member} was removed or renamed"),
                breaking: true,
            }),
            Some(new_type) if new_type != old_type => changes.push(Change {
                api: api.to_owned(),
                what: format!(
                    "the {which} member {member} changed type, from {old_type} to {new_type}"
                ),
                breaking: true,
            }),
            Some(_) => {}
        }
    }

    for member in new_members.keys() {
        if !old_members.contains_key(member) {
            let must_be_sent = new_required.contains(member);
            changes.push(Change {
                api: api.to_owned(),
                what: if must_be_sent {
                    format!("a new required {which} member, {member}")
                } else {
                    format!("a new optional {which} member, {member}")
                },
                // A new required parameter breaks every existing caller; a new output
                // member breaks nobody, required or not.
                breaking: must_be_sent && which == "params",
            });
        }
    }

    for member in &new_required {
        if old_members.contains_key(member) && !old_required.contains(member) {
            changes.push(Change {
                api: api.to_owned(),
                what: format!("the {which} member {member} became required"),
                breaking: which == "params",
            });
        }
    }
}

/// Prints what changed and decides whether the versions answer for it.
fn report(
    base: &str,
    changes: &[Change],
    old_apis: &BTreeMap<String, Value>,
    new_apis: &BTreeMap<String, Value>,
) -> Result<(), String> {
    if changes.is_empty() {
        println!("compat: the manifest is unchanged against {base}");
        return Ok(());
    }

    println!("compat: against {base}");
    for change in changes {
        let kind = if change.breaking {
            "BREAKING"
        } else {
            "additive"
        };
        println!("  {kind:<9} {:<18} {}", change.api, change.what);
    }

    // Every API that changed must carry a version that answers for the change.
    let mut unanswered = Vec::new();
    for (name, new) in new_apis {
        let Some(old) = old_apis.get(name) else {
            continue; // A new API is additive on its own, and starts at its own version.
        };
        let touched: Vec<&Change> = changes
            .iter()
            .filter(|change| change.api == *name)
            .collect();
        if touched.is_empty() {
            continue;
        }
        let breaking = touched.iter().any(|change| change.breaking);
        let (before, after) = (text(old, "version"), text(new, "version"));

        match compare_versions(&before, &after) {
            Bump::Major => {}
            Bump::Minor if !breaking => {}
            bump => unanswered.push(format!(
                "{name}: {} change, version went from {before} to {after} ({})",
                if breaking {
                    "a breaking"
                } else {
                    "an additive"
                },
                match bump {
                    Bump::None => "no bump",
                    Bump::Patch => "only a patch bump",
                    Bump::Minor => "only a minor bump",
                    Bump::Major => "a major bump",
                    Bump::Backwards => "the version went backwards",
                }
            )),
        }
    }

    println!();
    if unanswered.is_empty() {
        println!("compat: every change is answered by the version of its API");
        Ok(())
    } else {
        Err(format!(
            "{} change{} needs a version bump it did not get:\n  {}\n\nSection 10 of \
             docs/CONTRACT.md: a breaking change to the parameters or the output of an API \
             is a major bump of that API alone; anything added is a minor one.",
            unanswered.len(),
            if unanswered.len() == 1 { "" } else { "s" },
            unanswered.join("\n  ")
        ))
    }
}

/// How two semantic versions differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bump {
    None,
    Patch,
    Minor,
    Major,
    Backwards,
}

/// Compares two `major.minor.patch` strings.
fn compare_versions(before: &str, after: &str) -> Bump {
    let parse = |version: &str| -> (u64, u64, u64) {
        let mut parts = version
            .split('.')
            .map(|part| part.parse::<u64>().unwrap_or(0));
        (
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
        )
    };
    let (old_major, old_minor, old_patch) = parse(before);
    let (new_major, new_minor, new_patch) = parse(after);

    if (new_major, new_minor, new_patch) < (old_major, old_minor, old_patch) {
        Bump::Backwards
    } else if new_major > old_major {
        Bump::Major
    } else if new_minor > old_minor {
        Bump::Minor
    } else if new_patch > old_patch {
        Bump::Patch
    } else {
        Bump::None
    }
}

/// The manifest as it was at a commit.
fn manifest_at(root: &Path, base: &str) -> Result<Value, String> {
    let output = Command::new("git")
        .args(["show", &format!("{base}:manifest/solar.manifest.json")])
        .current_dir(root)
        .output()
        .map_err(|failure| format!("git could not be started: {failure}"))?;
    if !output.status.success() {
        return Err(format!(
            "the manifest at {base} could not be read: {}. Is {base} a commit, a tag or a \
             branch that this clone has?",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|failure| format!("the manifest at {base} is not valid JSON: {failure}"))
}

/// The manifest of the working tree, generated rather than read, so that a stale file
/// cannot hide a change.
fn current_manifest(_root: &Path) -> Result<Value, String> {
    let registry = solar_apis::registry().map_err(|problems| {
        let listed: Vec<String> = problems.iter().map(ToString::to_string).collect();
        format!("the registry does not build:\n  {}", listed.join("\n  "))
    })?;
    serde_json::to_value(solar_core::manifest::build(registry))
        .map_err(|failure| format!("the manifest could not be built: {failure}"))
}

/// How deep a chain of references is followed before it is called a cycle.
///
/// Nothing in the manifest is recursive today. The limit exists so that a manifest that
/// one day is cannot hang the check.
const DEEPEST_REFERENCE: usize = 32;

/// A manifest with every `$ref` replaced by what it points at.
///
/// A reference that cannot be resolved is left exactly as it is, so that a manifest from
/// an older layout, which had no shared `$defs`, still compares.
fn resolve_all(manifest: &Value) -> Value {
    let shared = manifest.get("$defs").cloned().unwrap_or(Value::Null);
    let mut resolved = manifest.clone();
    if let Some(apis) = resolved.get_mut("apis").and_then(Value::as_array_mut) {
        for api in apis.iter_mut() {
            for which in ["params_schema", "output_schema"] {
                let Some(schema) = api.get_mut(which) else {
                    continue;
                };
                // A manifest written before the definitions were shared keeps them inside
                // each schema. Both scopes are resolved, so that the two layouts compare
                // as what they are: the same schemas, written down differently.
                let defs = merge(&shared, schema.get("$defs"));
                let mut expanded = resolve(schema, &defs, 0);
                if let Some(object) = expanded.as_object_mut() {
                    object.remove("$defs");
                    object.remove("$schema");
                }
                *schema = expanded;
            }
        }
    }
    // Once every use is expanded the definitions are noise, and leaving them in would
    // report moving one into the shared block as a change of the document.
    if let Some(object) = resolved.as_object_mut() {
        object.remove("$defs");
    }
    resolved
}

/// The shared definitions with a schema's own laid over them.
fn merge(shared: &Value, own: Option<&Value>) -> Value {
    let mut all = shared.as_object().cloned().unwrap_or_default();
    if let Some(Value::Object(local)) = own {
        for (name, definition) in local {
            all.insert(name.clone(), definition.clone());
        }
    }
    Value::Object(all)
}

/// Replaces `$ref` with what it points at, recursively.
fn resolve(node: &Value, defs: &Value, depth: usize) -> Value {
    if depth > DEEPEST_REFERENCE {
        return node.clone();
    }
    match node {
        Value::Object(members) => {
            if let Some(target) = members.get("$ref").and_then(Value::as_str)
                && let Some(name) = target.strip_prefix("#/$defs/")
                && let Some(definition) = defs.get(name)
            {
                let mut expanded = resolve(definition, defs, depth + 1);
                // A sibling of `$ref`, such as a description, stays: 2020-12 keeps them,
                // and dropping one would hide a change to it.
                if let Some(object) = expanded.as_object_mut() {
                    for (key, value) in members {
                        if key != "$ref" {
                            object.insert(key.clone(), resolve(value, defs, depth + 1));
                        }
                    }
                }
                return expanded;
            }
            Value::Object(
                members
                    .iter()
                    .map(|(key, value)| (key.clone(), resolve(value, defs, depth + 1)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| resolve(item, defs, depth + 1))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The APIs of a manifest, by name.
fn apis_by_name(manifest: &Value) -> BTreeMap<String, Value> {
    manifest
        .get("apis")
        .and_then(Value::as_array)
        .map(|apis| {
            apis.iter()
                .filter_map(|api| {
                    api.get("name")
                        .and_then(Value::as_str)
                        .map(|name| (name.to_owned(), api.clone()))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The members of a schema, each with the type it declares, flattened one level.
fn properties(schema: &Value) -> BTreeMap<String, String> {
    schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|members| {
            members
                .iter()
                .map(|(name, member)| {
                    let kind = member.get("type").map_or_else(
                        || member.get("$ref").cloned().unwrap_or(Value::Null),
                        Clone::clone,
                    );
                    (name.clone(), kind.to_string())
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The members a schema requires.
fn required(schema: &Value) -> Vec<String> {
    schema
        .get("required")
        .and_then(Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// The `{status}/{reason}` pairs an API declares.
fn declared_errors(api: &Value) -> Vec<String> {
    api.get("errors")
        .and_then(Value::as_array)
        .map(|errors| {
            errors
                .iter()
                .map(|failure| format!("{}/{}", text(failure, "status"), text(failure, "reason")))
                .collect()
        })
        .unwrap_or_default()
}

/// A member of an object as text, or a dash when it is absent.
fn text(value: &Value, member: &str) -> String {
    value
        .get(member)
        .and_then(Value::as_str)
        .unwrap_or("-")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_version_comparison_knows_which_part_moved() {
        assert_eq!(compare_versions("1.0.0", "1.0.0"), Bump::None);
        assert_eq!(compare_versions("1.0.0", "1.0.1"), Bump::Patch);
        assert_eq!(compare_versions("1.0.0", "1.1.0"), Bump::Minor);
        assert_eq!(compare_versions("1.0.0", "2.0.0"), Bump::Major);
        assert_eq!(compare_versions("1.2.0", "1.1.0"), Bump::Backwards);
        assert_eq!(compare_versions("2.0.0", "1.9.9"), Bump::Backwards);
    }

    /// A schema of one object with the members given, and the ones named as required.
    fn schema(members: &[(&str, &str)], required: &[&str]) -> Value {
        let properties: serde_json::Map<String, Value> = members
            .iter()
            .map(|(name, kind)| ((*name).to_owned(), json!({"type": kind})))
            .collect();
        json!({"type": "object", "properties": properties, "required": required})
    }

    #[test]
    fn a_member_that_disappears_or_changes_type_is_breaking() {
        let mut changes = Vec::new();
        compare_schema(
            "a.b",
            "params",
            Some(&schema(&[("kept", "string"), ("gone", "string")], &[])),
            Some(&schema(&[("kept", "integer")], &[])),
            &mut changes,
        );
        assert_eq!(changes.len(), 2);
        assert!(
            changes.iter().all(|change| change.breaking),
            "both of these break a caller"
        );
    }

    #[test]
    fn a_new_optional_parameter_is_additive_and_a_required_one_is_not() {
        let mut changes = Vec::new();
        compare_schema(
            "a.b",
            "params",
            Some(&schema(&[], &[])),
            Some(&schema(&[("optional", "string")], &[])),
            &mut changes,
        );
        assert_eq!(changes.len(), 1);
        assert!(!changes[0].breaking);

        let mut changes = Vec::new();
        compare_schema(
            "a.b",
            "params",
            Some(&schema(&[], &[])),
            Some(&schema(&[("demanded", "string")], &["demanded"])),
            &mut changes,
        );
        assert_eq!(changes.len(), 1);
        assert!(
            changes[0].breaking,
            "a new required parameter breaks every existing caller"
        );
    }

    #[test]
    fn a_new_output_member_is_additive_even_when_it_is_required() {
        let mut changes = Vec::new();
        compare_schema(
            "a.b",
            "output",
            Some(&schema(&[], &[])),
            Some(&schema(&[("added", "string")], &["added"])),
            &mut changes,
        );
        assert_eq!(changes.len(), 1);
        assert!(
            !changes[0].breaking,
            "a reader ignores what it does not know"
        );
    }

    #[test]
    fn a_parameter_that_becomes_required_is_breaking() {
        let mut changes = Vec::new();
        compare_schema(
            "a.b",
            "params",
            Some(&schema(&[("maybe", "string")], &[])),
            Some(&schema(&[("maybe", "string")], &["maybe"])),
            &mut changes,
        );
        assert_eq!(changes.len(), 1);
        assert!(changes[0].breaking);
    }

    /// A manifest with one API whose output has a `thing` member of the given schema,
    /// plus whatever shared definitions are passed.
    fn manifest_with(defs: &Value, thing: &Value, apis: &[&str]) -> Value {
        let entries: Vec<Value> = apis
            .iter()
            .map(|name| {
                json!({
                    "name": name,
                    "version": "1.0.0",
                    "output_schema": {
                        "type": "object",
                        "properties": {"thing": thing.clone()},
                    },
                    "params_schema": {"type": "object", "properties": {}},
                })
            })
            .collect();
        json!({"schema_version": "2.0.0", "$defs": defs.clone(), "apis": entries})
    }

    #[test]
    fn a_minor_layout_bump_is_additive_and_a_major_one_is_breaking() {
        // Record 22: a consumer refuses a manifest whose major differs and reads one whose
        // minor moved, so adding a member to the document has to be possible.
        let before = json!({"schema_version": "2.0.0", "apis": []});

        let minor = json!({"schema_version": "2.1.0", "apis": []});
        let changes = differences(&before, &minor);
        assert_eq!(changes.len(), 1);
        assert!(!changes[0].breaking, "{}", changes[0].what);
        assert!(changes[0].what.contains("2.0.0"));
        assert!(changes[0].what.contains("2.1.0"));

        let major = json!({"schema_version": "3.0.0", "apis": []});
        let changes = differences(&before, &major);
        assert_eq!(changes.len(), 1);
        assert!(changes[0].breaking, "a new major is a new document");
    }

    #[test]
    fn a_version_that_cannot_be_read_is_its_own_major() {
        assert_eq!(major_of("2.1.0"), "2");
        assert_eq!(major_of("10.0.0"), "10");
        assert_eq!(major_of("nonsense"), "nonsense");
        assert_eq!(major_of(""), "");
    }

    #[test]
    fn moving_a_definition_into_the_shared_block_is_not_a_change_at_all() {
        // Before: the definition sits inside the schema, which is how schemars writes it.
        let before = json!({
            "schema_version": "1.0.0",
            "apis": [{
                "name": "a.b",
                "version": "1.0.0",
                "params_schema": {"type": "object", "properties": {}},
                "output_schema": {
                    "$schema": "https://json-schema.org/draft/2020-12/schema",
                    "type": "object",
                    "properties": {"thing": {"$ref": "#/$defs/Thing"}},
                    "$defs": {"Thing": {"type": "string", "enum": ["one", "two"]}},
                },
            }],
        });
        // After: the same definition, hoisted to the root of the document.
        let after = manifest_with(
            &json!({"Thing": {"type": "string", "enum": ["one", "two"]}}),
            &json!({"$ref": "#/$defs/Thing"}),
            &["a.b"],
        );

        let old_apis = apis_by_name(&resolve_all(&before));
        let new_apis = apis_by_name(&resolve_all(&after));
        let mut changes = Vec::new();
        compare_api("a.b", &old_apis["a.b"], &new_apis["a.b"], &mut changes);

        let described: Vec<&str> = changes.iter().map(|change| change.what.as_str()).collect();
        assert!(
            described.is_empty(),
            "the schema did not change, only where it is written: {described:?}"
        );
    }

    #[test]
    fn a_change_inside_a_shared_definition_is_reported_for_every_api_that_uses_it() {
        let used_by = ["a.b", "c.d", "e.f"];
        let before = manifest_with(
            &json!({"Thing": {"type": "string", "enum": ["one", "two"]}}),
            &json!({"$ref": "#/$defs/Thing"}),
            &used_by,
        );
        // One definition changes type. Every API that reaches it is affected, and the
        // text of the `$ref` is the same in both, so only resolving finds this.
        let after = manifest_with(
            &json!({"Thing": {"type": "integer"}}),
            &json!({"$ref": "#/$defs/Thing"}),
            &used_by,
        );

        let old_apis = apis_by_name(&resolve_all(&before));
        let new_apis = apis_by_name(&resolve_all(&after));
        let mut changes = Vec::new();
        for name in used_by {
            compare_api(name, &old_apis[name], &new_apis[name], &mut changes);
        }

        assert_eq!(
            changes.len(),
            3,
            "one report per API that uses it: {changes:?}"
        );
        for change in &changes {
            assert!(change.breaking, "a type that changed breaks a caller");
            assert!(change.what.contains("thing"), "{}", change.what);
            assert!(used_by.contains(&change.api.as_str()));
        }
    }

    #[test]
    fn rewording_a_description_is_not_a_change() {
        let before = manifest_with(
            &json!({}),
            &json!({"type": "string", "description": "the old words"}),
            &["a.b"],
        );
        let after = manifest_with(
            &json!({}),
            &json!({"type": "string", "description": "entirely different words"}),
            &["a.b"],
        );
        let old_apis = apis_by_name(&resolve_all(&before));
        let new_apis = apis_by_name(&resolve_all(&after));
        let mut changes = Vec::new();
        compare_api("a.b", &old_apis["a.b"], &new_apis["a.b"], &mut changes);
        assert!(changes.is_empty(), "prose is not a promise: {changes:?}");
    }

    #[test]
    fn a_reference_that_points_nowhere_is_left_as_it_is() {
        let manifest = manifest_with(&json!({}), &json!({"$ref": "#/$defs/Missing"}), &["a.b"]);
        let resolved = resolve_all(&manifest);
        assert_eq!(
            resolved["apis"][0]["output_schema"]["properties"]["thing"]["$ref"], "#/$defs/Missing",
            "an unresolvable reference is kept, so an older layout still compares"
        );
    }

    #[test]
    fn a_declared_error_that_disappears_is_breaking_and_a_new_one_is_not() {
        let old = json!({"errors": [{"status": "NOT_FOUND", "reason": "API_NOT_FOUND"}]});
        let new = json!({"errors": [{"status": "UNAVAILABLE", "reason": "SPAWN_FAILED"}]});
        let mut changes = Vec::new();
        compare_api("a.b", &old, &new, &mut changes);
        assert_eq!(changes.len(), 2);
        assert_eq!(changes.iter().filter(|change| change.breaking).count(), 1);
    }
}
