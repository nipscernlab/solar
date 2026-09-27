//! The runner for `conformance/`, which is the suite any client in any language replays.
//!
//! The files are the truth and this is one implementation of the runner. A case that
//! fails here would fail for a TypeScript or Python client too, which is the whole point
//! of keeping the cases out of Rust.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test reports failure by panicking; integration tests are their own crate"
)]

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;
use solar_core::api::MatchMode;
use solar_core::matching::matches;

/// One case, as it is written on disk.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    /// The name, which is also the file name.
    name: String,
    /// What the case demonstrates.
    description: String,
    /// The request, when it is valid JSON.
    #[serde(default)]
    request: Option<Value>,
    /// The request as a raw line, for a case about something malformed.
    #[serde(default)]
    request_line: Option<String>,
    /// The whole response envelope that must come back.
    response: Value,
    /// `exact` or `subset`, the two modes of section 8.2 of the contract.
    #[serde(rename = "match")]
    match_mode: String,
}

impl Case {
    /// The line this case sends.
    fn line(&self) -> String {
        match (&self.request, &self.request_line) {
            (Some(request), None) => request.to_string(),
            (None, Some(line)) => line.clone(),
            (Some(_), Some(_)) => {
                panic!(
                    "{}: a case has either request or request_line, not both",
                    self.name
                )
            }
            (None, None) => panic!("{}: a case needs a request or a request_line", self.name),
        }
    }

    /// How its response is compared.
    fn mode(&self) -> MatchMode {
        match self.match_mode.as_str() {
            "exact" => MatchMode::Exact,
            "subset" => MatchMode::Subset,
            other => panic!("{}: {other} is not a match mode", self.name),
        }
    }
}

fn conformance_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate lives two directories below the repository root")
        .join("conformance")
        .join("cases")
}

/// Every case on disk, sorted by name so that a failure is reported in a stable order.
fn cases() -> Vec<(PathBuf, Case)> {
    let directory = conformance_directory();
    let mut found: Vec<(PathBuf, Case)> = std::fs::read_dir(&directory)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", directory.display()))
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .map(|path| {
            let text = std::fs::read_to_string(&path).unwrap_or_else(|failure| {
                panic!("{} could not be read: {failure}", path.display())
            });
            let case: Case = serde_json::from_str(&text).unwrap_or_else(|failure| {
                panic!("{} is not a valid case: {failure}", path.display())
            });
            (path, case)
        })
        .collect();
    found.sort_by(|left, right| left.0.cmp(&right.0));
    assert!(
        !found.is_empty(),
        "there are no cases in {}",
        directory.display()
    );
    found
}

#[test]
fn every_case_is_well_formed() {
    for (path, case) in cases() {
        let file_name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default();
        assert_eq!(
            case.name,
            file_name,
            "{} is named {} inside",
            path.display(),
            case.name
        );
        assert!(
            !case.description.trim().is_empty(),
            "{} says nothing",
            case.name
        );
        assert!(
            case.response.is_object(),
            "{}: a response is an object",
            case.name
        );
        let _ = case.mode();
        let _ = case.line();
    }
}

#[test]
fn every_case_holds_against_this_build() {
    let dispatcher = solar_apis::dispatcher();
    let mut failures = Vec::new();

    for (_, case) in cases() {
        let line = case.line();
        let answered = dispatcher.handle_line(&line).to_line();
        let real: Value = serde_json::from_str(&answered)
            .unwrap_or_else(|failure| panic!("{}: the answer is not JSON: {failure}", case.name));

        if let Err(mismatch) = matches(&case.response, &real, case.mode()) {
            failures.push(format!("{}: {mismatch}\n      sent: {line}", case.name));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of the conformance cases do not hold:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

#[test]
fn the_suite_covers_the_cases_a_client_has_to_handle() {
    // A client that handles only the happy path is not a client. The suite is worth
    // having only if it demonstrates the refusals too, so it is checked for them.
    let names: Vec<String> = cases().into_iter().map(|(_, case)| case.name).collect();
    for expected in [
        "ping_bare",
        "notification_is_refused",
        "batch_is_unimplemented",
        "parse_error",
        "method_not_found_suggests_the_closest",
        "unknown_parameter_is_refused",
        "missing_required_parameter",
    ] {
        assert!(
            names.iter().any(|name| name == expected),
            "the suite has no case {expected}"
        );
    }
}
