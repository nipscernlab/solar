//! `cargo xtask ci` and `.github/workflows/ci.yml` must not drift apart.
//!
//! The brief for this repository says the one command runs exactly what CI runs. Nothing
//! can prove that in general, since one is a Rust program and the other is YAML, but the
//! two can be held to the same list of steps: every check named here appears in both, and
//! a step added to one without the other fails this test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test reports failure by panicking; integration tests are their own crate"
)]

use std::path::{Path, PathBuf};

/// The checks that make up the pipeline, each with the line that runs it in the workflow
/// and the name it carries in `xtask ci`.
///
/// The workflow needles are whole `run:` lines, because a looser needle matches the step
/// that installs a tool rather than the step that runs it.
const CHECKS: [(&str, &str, &str); 11] = [
    (
        "formatting",
        "run: cargo fmt --all -- --check",
        r#""formatting""#,
    ),
    (
        "toml formatting",
        "run: taplo fmt --check",
        r#""toml formatting""#,
    ),
    ("spelling", "run: typos", r#""spelling""#),
    (
        "lints",
        "run: cargo clippy --workspace --all-targets --locked -- -D warnings",
        r#""lints""#,
    ),
    (
        "tests",
        "run: cargo nextest run --workspace --locked",
        r#""tests""#,
    ),
    (
        "doctests",
        "run: cargo test --doc --workspace --locked",
        r#""doctests""#,
    ),
    (
        "documentation",
        "run: cargo doc --workspace --no-deps --locked",
        r#""documentation""#,
    ),
    ("manifest", "-- manifest --check", r#""manifest""#),
    ("compatibility", "-- compat ", r#""compatibility""#),
    (
        "documentation runs",
        "-- doc-run",
        r#""documentation runs""#,
    ),
    ("no local paths", "-- leak-check", r#""no local paths""#),
];

/// The checks that run in one job, in one order, so their order can be compared.
///
/// `documentation` is missing on purpose: it is a job of its own, which runs beside the
/// others rather than among them, so where it sits in the file says nothing.
const ORDERED: [&str; 10] = [
    "formatting",
    "toml formatting",
    "spelling",
    "lints",
    "tests",
    "doctests",
    "manifest",
    "compatibility",
    "documentation runs",
    "no local paths",
];

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives one directory below the repository root")
        .to_path_buf()
}

fn read(relative: &str) -> String {
    let path = repository_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

#[test]
fn every_check_is_in_both_the_workflow_and_the_one_command() {
    let workflow = read(".github/workflows/ci.yml");
    let command = read("xtask/src/ci.rs");

    for (name, in_workflow, in_command) in CHECKS {
        assert!(
            workflow.contains(in_workflow),
            "the workflow has no step running `{in_workflow}`, which `xtask ci` runs as {name}"
        );
        assert!(
            command.contains(in_command),
            "`xtask ci` has no step named {in_command}, which the workflow runs as {name}"
        );
    }
}

/// Where each ordered check sits in a file, by the needle at `which` in its row.
fn positions(text: &str, which: usize) -> Vec<usize> {
    ORDERED
        .iter()
        .map(|name| {
            let row = CHECKS
                .iter()
                .find(|check| check.0 == *name)
                .unwrap_or_else(|| panic!("{name} is not a known check"));
            let needle = if which == 1 { row.1 } else { row.2 };
            text.find(needle)
                .unwrap_or_else(|| panic!("{needle} is not in this file"))
        })
        .collect()
}

#[test]
fn the_one_command_runs_the_checks_in_the_order_the_workflow_does() {
    let workflow = read(".github/workflows/ci.yml");
    let command = read("xtask/src/ci.rs");

    let in_workflow = positions(&workflow, 1);
    let in_command = positions(&command, 2);

    let ordered = |positions: &[usize]| positions.windows(2).all(|pair| pair[0] < pair[1]);
    assert!(
        ordered(&in_workflow),
        "the workflow runs the checks in another order: {in_workflow:?}"
    );
    assert!(
        ordered(&in_command),
        "`xtask ci` runs the checks in another order: {in_command:?}"
    );
}

#[test]
fn every_cargo_command_of_the_workflow_is_locked() {
    let workflow = read(".github/workflows/ci.yml");
    for line in workflow.lines().map(str::trim) {
        let Some(rest) = line.strip_prefix("cargo ") else {
            continue;
        };
        // `cargo fmt` reads no manifest, and `cargo --version` builds nothing.
        if rest.starts_with("fmt") || rest.starts_with("--version") {
            continue;
        }
        assert!(
            line.contains("--locked"),
            "this line of the workflow is not --locked: {line}"
        );
    }
}
