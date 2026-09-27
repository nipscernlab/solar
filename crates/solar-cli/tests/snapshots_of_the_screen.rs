//! What a person sees, frozen.
//!
//! `solar list`, `solar describe` and `solar version` lay out a response for a human
//! reader, and that layout is as much a promise as the JSON is. These snapshots make a
//! change to it something somebody chose. What differs between two machines, the version
//! and the build metadata, is replaced before comparing.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test reports failure by panicking; integration tests are their own crate"
)]

use std::process::Command;

use insta::assert_snapshot;

/// The binary cargo just built.
const SOLAR: &str = env!("CARGO_BIN_EXE_solar");

/// Runs `solar` and returns what a person would read, with the machine's own facts
/// replaced by placeholders.
/// The labels whose value is a fact about this machine rather than about the layout.
const PER_MACHINE: [&str; 5] = ["commit", "dirty", "profile", "target", "compiler"];

fn screen(arguments: &[&str]) -> String {
    let output = Command::new(SOLAR)
        .args(arguments)
        .env_remove("SOLAR_LOG")
        .env_remove("NO_COLOR")
        .output()
        .expect("the binary must run");
    let text = String::from_utf8(output.stdout).expect("the output is UTF-8");
    let version = env!("CARGO_PKG_VERSION");

    let mut redacted = String::new();
    for line in text.lines() {
        let line = line.replace(version, "[version]");
        let label = line.split_whitespace().next().unwrap_or_default();
        if PER_MACHINE.contains(&label) {
            // Replace the value and keep the column, because the column is the layout.
            let value_at = line
                .char_indices()
                .skip(label.len())
                .find(|(_, character)| !character.is_whitespace())
                .map_or(line.len(), |(index, _)| index);
            redacted.push_str(&line[..value_at]);
            redacted.push_str("[build]");
        } else {
            redacted.push_str(&line);
        }
        redacted.push('\n');
    }
    redacted
}

#[test]
fn the_list_of_apis() {
    assert_snapshot!(screen(&["list"]));
}

#[test]
fn the_description_of_an_api() {
    assert_snapshot!(screen(&["describe", "solar.ping"]));
}

#[test]
fn the_description_of_the_one_api_with_no_parameters() {
    assert_snapshot!(screen(&["describe", "system.info"]));
}

#[test]
fn the_versions_and_the_build() {
    assert_snapshot!(screen(&["version"]));
}

#[test]
fn the_help_text() {
    assert_snapshot!(screen(&["--help"]));
}
