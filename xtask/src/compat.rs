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

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

/// One difference between two manifests.
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
    let before = manifest_at(root, base)?;
    let after = current_manifest(root)?;

    let old_apis = apis_by_name(&before);
    let new_apis = apis_by_name(&after);
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
        changes.push(Change {
            api: "the manifest".to_owned(),
            what: format!(
                "the layout version went from {} to {}",
                text(&before, "schema_version"),
                text(&after, "schema_version")
            ),
            breaking: true,
        });
    }

    report(base, &changes, &old_apis, &new_apis)
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
            let now_required = new_required.contains(member);
            changes.push(Change {
                api: api.to_owned(),
                what: if now_required {
                    format!("a new required {which} member, {member}")
                } else {
                    format!("a new optional {which} member, {member}")
                },
                // A new required parameter breaks every existing caller; a new output
                // member breaks nobody, required or not.
                breaking: now_required && which == "params",
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
