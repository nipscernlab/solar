//! The template is enforced here, not in review.
//!
//! Section 12 of `docs/CONTRACT.md` names each of these tests against the rule it keeps.
//! An API that breaks one of them fails the build, whoever wrote it and however plausible
//! it looked. That is the point: the rules are the same for a person and for an agent.

// A test reports failure by panicking, so the lints that forbid it in production code are
// lifted here. Integration tests are their own crate, which is why `clippy.toml` does not
// cover them.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test reports failure by panicking; integration tests are their own crate, \n              so clippy.toml does not cover them"
)]

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use solar_core::api::{MatchMode, SideEffect, Stability};
use solar_core::manifest;
use solar_core::matching::matches;
use solar_core::registry::{Registry, is_semver};
use solar_core::status::Status;
use solar_core::text::is_valid_method_name;

fn registry() -> &'static Registry {
    match solar_apis::registry() {
        Ok(registry) => registry,
        Err(problems) => {
            let listed: Vec<String> = problems.iter().map(ToString::to_string).collect();
            panic!("the registry does not build:\n  {}", listed.join("\n  "));
        }
    }
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate lives two directories below the repository root")
        .to_path_buf()
}

#[test]
fn names_follow_the_naming_rule() {
    for entry in registry().entries() {
        assert!(
            is_valid_method_name(entry.name()),
            r"{} does not match ^[a-z]+(\.[a-z]+(_[a-z]+)*)+$",
            entry.name()
        );
    }
}

#[test]
fn names_are_unique() {
    let mut names: Vec<&str> = registry().names().collect();
    let total = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), total, "two APIs share a name");
}

#[test]
fn specs_are_complete() {
    for entry in registry().entries() {
        let name = entry.name();
        let spec = entry.spec();
        assert!(
            is_semver(entry.version()),
            "{name}: version {} is not x.y.z",
            entry.version()
        );
        assert!(
            is_semver(spec.since),
            "{name}: since {} is not x.y.z",
            spec.since
        );
        assert!(!spec.summary.trim().is_empty(), "{name} has no summary");
        assert!(
            !spec.summary.ends_with('.'),
            "{name}: a summary is not a sentence"
        );
        assert!(
            spec.summary.len() <= 80,
            "{name}: the summary is longer than one line"
        );
        assert!(
            spec.description.trim().len() >= 80,
            "{name}: the description is too short to be useful"
        );
        assert!(spec.timeout_ms > 0, "{name} has no timeout");
        assert!(!spec.examples.is_empty(), "{name} has no example");
        assert!(
            !spec.side_effects.is_empty(),
            "{name} declares no side effects"
        );
        assert!(
            !(spec.side_effects.contains(&SideEffect::None) && spec.side_effects.len() > 1),
            "{name}: none is exclusive"
        );
        for example in &spec.examples {
            assert!(
                example.params.is_object(),
                "{name}/{}: params are not an object",
                example.name
            );
            assert!(
                !example.description.trim().is_empty(),
                "{name}/{}: the example says nothing",
                example.name
            );
        }
    }
}

#[test]
fn everything_is_experimental_before_1_0() {
    let major = solar_core::meta::SOLAR_VERSION
        .split('.')
        .next()
        .unwrap_or("0");
    if major != "0" {
        return;
    }
    for entry in registry().entries() {
        assert_eq!(
            entry.spec().stability,
            Stability::Experimental,
            "{} claims more than SOLAR {} can promise",
            entry.name(),
            solar_core::meta::SOLAR_VERSION
        );
    }
}

#[test]
fn declared_errors_are_canonical_and_documented() {
    let catalogue = std::fs::read_to_string(repository_root().join("docs/ERRORS.md"))
        .expect("docs/ERRORS.md must be readable");

    for entry in registry().entries() {
        for declared in &entry.spec().errors {
            assert_eq!(
                declared.status,
                declared.reason.status(),
                "{}: {} does not belong to {}",
                entry.name(),
                declared.reason,
                declared.status
            );
            assert!(
                Status::from_str_canonical(declared.status.as_str()).is_some(),
                "{}: {} is not a canonical status",
                entry.name(),
                declared.status
            );
            assert!(
                catalogue.contains(&format!("### {}", declared.reason)),
                "{}: {} is not documented in docs/ERRORS.md",
                entry.name(),
                declared.reason
            );
        }
    }
}

#[test]
fn params_schemas_reject_unknown_fields() {
    for entry in registry().entries() {
        let schema = entry.params_schema();
        assert_eq!(
            schema.get("additionalProperties"),
            Some(&Value::Bool(false)),
            "{}: the parameter type needs #[serde(deny_unknown_fields)]",
            entry.name()
        );
    }
}

#[test]
fn schemas_are_2020_12() {
    const META: &str = "https://json-schema.org/draft/2020-12/schema";
    for entry in registry().entries() {
        for (which, schema) in [
            ("params", entry.params_schema()),
            ("output", entry.output_schema()),
        ] {
            assert_eq!(
                schema.get("$schema").and_then(Value::as_str),
                Some(META),
                "{}: the {which} schema is not JSON Schema 2020-12",
                entry.name()
            );
            jsonschema::validator_for(schema).unwrap_or_else(|failure| {
                panic!(
                    "{}: the {which} schema is not usable: {failure}",
                    entry.name()
                )
            });
        }
    }
}

#[test]
fn examples_validate_against_params_schema() {
    for entry in registry().entries() {
        let validator = jsonschema::validator_for(entry.params_schema())
            .unwrap_or_else(|failure| panic!("{}: {failure}", entry.name()));
        for example in &entry.spec().examples {
            let problems: Vec<String> = validator
                .iter_errors(&example.params)
                .map(|e| format!("{e}"))
                .collect();
            assert!(
                problems.is_empty(),
                "{}/{}: the example parameters do not fit the schema: {}",
                entry.name(),
                example.name,
                problems.join("; ")
            );
        }
    }
}

#[test]
fn examples_match_real_execution() {
    let dispatcher = solar_apis::dispatcher();

    for entry in registry().entries() {
        let validator = jsonschema::validator_for(entry.output_schema())
            .unwrap_or_else(|failure| panic!("{}: {failure}", entry.name()));

        for example in &entry.spec().examples {
            let request = json!({
                "jsonrpc": "2.0",
                "id": example.name,
                "method": entry.name(),
                "params": example.params,
            });
            let response = dispatcher.handle_line(&request.to_string());
            let envelope: Value =
                serde_json::from_str(&response.to_line()).unwrap_or_else(|failure| {
                    panic!("{}: the response is not JSON: {failure}", entry.name())
                });

            let result = envelope.get("result").unwrap_or_else(|| {
                panic!(
                    "{}/{}: the example failed: {}",
                    entry.name(),
                    example.name,
                    envelope["error"]["message"]
                )
            });
            let data = &result["data"];

            let problems: Vec<String> = validator
                .iter_errors(data)
                .map(|failure| format!("{failure}"))
                .collect();
            assert!(
                problems.is_empty(),
                "{}/{}: the real output does not fit its own schema: {}",
                entry.name(),
                example.name,
                problems.join("; ")
            );

            if let Err(mismatch) = matches(&example.response, data, example.match_mode) {
                panic!("{}/{}: {mismatch}", entry.name(), example.name);
            }

            assert_eq!(
                envelope["result"]["meta"]["api_version"],
                entry.version(),
                "{}: meta does not name the version that ran",
                entry.name()
            );
        }
    }
}

#[test]
fn an_exact_example_would_catch_a_changed_field() {
    // Guards the guard: a mode of `exact` must really refuse an extra member.
    let expected = json!({"pong": true});
    let real = json!({"pong": true, "unexpected": 1});
    assert!(matches(&expected, &real, MatchMode::Exact).is_err());
    assert!(matches(&expected, &real, MatchMode::Subset).is_ok());
}

#[test]
fn manifest_is_not_stale() {
    let path = repository_root().join("manifest/solar.manifest.json");
    let versioned = std::fs::read_to_string(&path).unwrap_or_else(|failure| {
        panic!(
            "{} could not be read ({failure}). Run `cargo xtask manifest`.",
            path.display()
        )
    });
    let versioned = versioned.replace("\r\n", "\n");
    let generated = manifest::render(registry());

    if versioned == generated {
        return;
    }

    let difference = versioned
        .lines()
        .zip(generated.lines())
        .enumerate()
        .find(|(_, (left, right))| left != right)
        .map_or_else(
            || {
                format!(
                    "the file has {} lines and the generator produces {}",
                    versioned.lines().count(),
                    generated.lines().count()
                )
            },
            |(line, (left, right))| {
                format!(
                    "line {}:\n    file:      {left}\n    generator: {right}",
                    line + 1
                )
            },
        );

    panic!(
        "{} is stale: {difference}\n\nRun `cargo xtask manifest` and commit the result.",
        path.display()
    );
}

#[test]
fn the_manifest_describes_every_registered_api_and_nothing_else() {
    let document = manifest::build(registry());
    let names: Vec<&str> = document
        .apis
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    let registered: Vec<&str> = registry().names().collect();
    assert_eq!(names, registered);
    assert_eq!(document.protocol, solar_core::meta::PROTOCOL);
}
