//! The catalogue in `docs/ERRORS.md` and the tables in `docs/CONTRACT.md` are part of the
//! contract, so they are tested like code.
//!
//! Documentation that drifts is worse than no documentation: a caller trusts it. These
//! tests fail when a status, a reason or a warning exists in one place and not in the other,
//! when a reason is documented under the wrong status, or when a number in a table stops
//! matching the number the code uses.

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

use solar_core::reason::Reason;
use solar_core::status::Status;
use solar_core::warning::WarningCode;

/// Sections of `docs/ERRORS.md` that are prose rather than a status.
const PROSE_SECTIONS: [&str; 2] = ["How to read an error", "Warnings"];

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate lives two directories below the repository root")
        .to_path_buf()
}

fn read(relative: &str) -> String {
    let path = repository_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

/// Every `## heading` and `### heading` of a document, with its level.
fn headings(document: &str) -> Vec<(usize, String)> {
    document
        .lines()
        .filter_map(|line| {
            let level = line.bytes().take_while(|byte| *byte == b'#').count();
            if (2..=3).contains(&level) {
                let text = line.get(level..)?.trim().to_owned();
                Some((level, text))
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn every_status_has_a_section_of_its_own() {
    let errors = read("docs/ERRORS.md");
    let documented: Vec<String> = headings(&errors)
        .into_iter()
        .filter(|(level, _)| *level == 2)
        .map(|(_, t)| t)
        .collect();

    for status in Status::ALL {
        assert!(
            documented.iter().any(|heading| heading == status.as_str()),
            "docs/ERRORS.md has no `## {status}` section"
        );
    }

    for heading in &documented {
        assert!(
            PROSE_SECTIONS.contains(&heading.as_str())
                || Status::from_str_canonical(heading).is_some(),
            "docs/ERRORS.md has a `## {heading}` section that is not a canonical status"
        );
    }
}

#[test]
fn every_reason_is_documented_under_its_own_status() {
    let errors = read("docs/ERRORS.md");
    let mut section = String::new();
    let mut seen: Vec<String> = Vec::new();

    for (level, heading) in headings(&errors) {
        if level == 2 {
            section = heading;
            continue;
        }
        if section == "Warnings" {
            continue;
        }
        let reason = Reason::from_str_canonical(&heading).unwrap_or_else(|| {
            panic!("docs/ERRORS.md documents `### {heading}`, which is not a known reason")
        });
        assert_eq!(
            reason.status().as_str(),
            section,
            "{heading} is documented under {section} and belongs to {}",
            reason.status()
        );
        seen.push(heading);
    }

    for reason in Reason::ALL {
        assert!(
            seen.iter().any(|heading| heading == reason.as_str()),
            "docs/ERRORS.md has no `### {reason}` section"
        );
    }
}

#[test]
fn every_warning_code_is_documented() {
    let errors = read("docs/ERRORS.md");
    let mut in_warnings = false;
    let mut documented: Vec<String> = Vec::new();

    for (level, heading) in headings(&errors) {
        if level == 2 {
            in_warnings = heading == "Warnings";
            continue;
        }
        if in_warnings {
            documented.push(heading);
        }
    }

    for code in WarningCode::ALL {
        assert!(
            documented.iter().any(|heading| heading == code.as_str()),
            "docs/ERRORS.md has no `### {code}` section under Warnings"
        );
    }
    for heading in &documented {
        assert!(
            WarningCode::ALL.iter().any(|code| code.as_str() == heading),
            "docs/ERRORS.md documents the warning `{heading}`, which does not exist"
        );
    }
}

#[test]
fn the_anchor_every_error_points_at_really_exists() {
    let errors = read("docs/ERRORS.md");
    let anchors: Vec<String> = headings(&errors)
        .into_iter()
        .map(|(_, heading)| {
            format!(
                "docs/ERRORS.md#{}",
                heading.to_lowercase().replace(' ', "-")
            )
        })
        .collect();

    for status in Status::ALL {
        assert!(
            anchors.contains(&status.docs()),
            "{} points at an anchor that does not exist",
            status.docs()
        );
    }
}

#[test]
fn the_status_to_code_table_of_the_contract_matches_the_code() {
    let contract = read("docs/CONTRACT.md");
    for status in Status::ALL {
        let needle = format!("| `{}`", status.as_str());
        let row = contract
            .lines()
            .find(|line| line.starts_with(&needle))
            .unwrap_or_else(|| panic!("docs/CONTRACT.md has no table row for {status}"));
        let code = format!("`{}`", status.code());
        assert!(
            row.contains(&code),
            "docs/CONTRACT.md gives {status} a different code than {code}: {row}"
        );
    }
}

#[test]
fn the_exit_code_table_of_the_contract_matches_the_code() {
    let contract = read("docs/CONTRACT.md");
    for status in Status::ALL {
        let needle = format!("| `{}`", status.exit_code());
        let row = contract
            .lines()
            .find(|line| line.starts_with(&needle))
            .unwrap_or_else(|| {
                panic!(
                    "docs/CONTRACT.md has no exit code row for {}",
                    status.exit_code()
                )
            });
        assert!(
            row.contains(status.as_str()),
            "exit code {} is documented as something other than {status}: {row}",
            status.exit_code()
        );
    }
}

#[test]
fn the_contract_states_the_limit_the_code_enforces() {
    let contract = read("docs/CONTRACT.md");
    assert!(
        contract.contains(&solar_core::protocol::MAX_REQUEST_BYTES.to_string()),
        "docs/CONTRACT.md does not state the request size limit the code enforces"
    );
    assert!(contract.contains(solar_core::meta::PROTOCOL));
}

#[test]
fn the_contract_states_the_batch_limit_the_code_enforces() {
    let contract = read("docs/CONTRACT.md");
    let limit = solar_core::protocol::MAX_BATCH_ELEMENTS.to_string();
    assert!(
        contract.contains(&format!("**{limit}**")),
        "docs/CONTRACT.md does not state that a batch holds at most {limit} elements"
    );
}
