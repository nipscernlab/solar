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
        // clap prints the file name of the binary, which carries .exe on Windows only.
        //
        // `since` is left alone: it says which SOLAR an API first appeared in, which is
        // a fact about the API and not something that differs between two machines.
        // Redacting it would move every snapshot on a version bump and would hide the
        // one number in that line a reader is there for.
        let since = format!("since {version}");
        let line = line
            .replace("solar.exe", "solar")
            .replace(&since, "\u{1}")
            .replace(version, "[version]")
            .replace('\u{1}', &since);
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

#[test]
fn every_version_in_the_list_starts_in_the_same_column() {
    // The defect this pins down: the name column was a fixed eighteen, and
    // `solar.set_log_level` is nineteen, so its version sat one column to the right of
    // every other. A snapshot alone would have recorded the crooked layout as correct,
    // which is why this measures the columns rather than comparing the text.
    let listing = screen(&["list"]);

    let columns: Vec<usize> = listing
        .lines()
        .filter(|line| line.starts_with("  solar.") || line.starts_with("  system."))
        .map(|line| {
            let name_at = line.find(char::is_alphabetic).unwrap_or(0);
            let after_name = line[name_at..]
                .find(' ')
                .map_or(line.len(), |offset| name_at + offset);
            line[after_name..]
                .find(|c: char| !c.is_whitespace())
                .map_or(line.len(), |offset| after_name + offset)
        })
        .collect();

    assert!(columns.len() >= 2, "there are APIs to line up: {listing}");
    let first = columns[0];
    assert!(
        columns.iter().all(|column| *column == first),
        "every version starts in the same column, and these start in {columns:?}:\n{listing}"
    );
}

#[test]
fn the_longest_name_still_leaves_two_spaces_before_its_version() {
    // The column is sized from the longest name, so the longest row is the one that would
    // lose its separation if the width were ever computed as the name length alone.
    let listing = screen(&["list"]);
    let longest = listing
        .lines()
        .filter(|line| line.starts_with("  solar.") || line.starts_with("  system."))
        .max_by_key(|line| {
            line.split_whitespace()
                .next()
                .map_or(0, |name| name.chars().count())
        })
        .unwrap_or_default();

    let name = longest.split_whitespace().next().unwrap_or_default();
    let after_name = longest.find(name).unwrap_or(0) + name.len();
    let gap = longest[after_name..]
        .chars()
        .take_while(|c| *c == ' ')
        .count();
    assert!(
        gap >= 2,
        "the longest name keeps its gap: {gap} space(s) in {longest:?}"
    );
}
