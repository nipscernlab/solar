//! `cargo xtask changelog <base>`: code and documentation move together.
//!
//! The rule, fixed for every project of the laboratory on 27 September 2026: every change
//! updates, in the same commit, every document it affects. An outdated document is a
//! defect, like a failing test.
//!
//! Most of that rule is judgement, and the pull request template lists it. One part of it
//! is mechanical, and this is that part: **a change under `crates/` comes with an entry in
//! `CHANGELOG.md`.** A machine cannot tell whether the entry is any good; it can tell that
//! somebody wrote one, and that is enough to stop the change that says nothing at all.

use std::path::Path;
use std::process::Command;

/// The directory whose changes need an entry.
const CODE: &str = "crates/";

/// The document that has to move with them.
const CHANGELOG: &str = "CHANGELOG.md";

/// Reports whether the code changed against `base` without the changelog changing too.
///
/// # Errors
///
/// Returns the files that changed without an entry, or why the comparison could not run.
pub(crate) fn run(root: &Path, base: &str) -> Result<(), String> {
    let changed = changed_files(root, base)?;

    if changed.is_empty() {
        println!("changelog: nothing changed against {base}");
        return Ok(());
    }

    let code: Vec<&String> = changed
        .iter()
        .filter(|path| path.starts_with(CODE))
        .collect();

    if code.is_empty() {
        println!(
            "changelog: {} file(s) changed against {base}, none of them under {CODE}",
            changed.len()
        );
        return Ok(());
    }

    if changed.iter().any(|path| path == CHANGELOG) {
        println!(
            "changelog: {} file(s) under {CODE} changed, and {CHANGELOG} moved with them",
            code.len()
        );
        return Ok(());
    }

    let listed: Vec<String> = code
        .iter()
        .take(10)
        .map(|path| format!("  {path}"))
        .collect();
    let more = if code.len() > 10 {
        format!("\n  ... and {} more", code.len() - 10)
    } else {
        String::new()
    };

    Err(format!(
        "these files changed against {base} and {CHANGELOG} did not:\n{}{more}\n\n\
         Code and documentation move together: every change updates, in the same commit, \
         every document it affects. Write the entry under `## [Unreleased]`, in the \
         section it belongs to, saying what a reader of SOLAR would notice.",
        listed.join("\n")
    ))
}

/// The files that differ between `base` and the working tree.
fn changed_files(root: &Path, base: &str) -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(["diff", "--name-only", base, "--"])
        .output()
        .map_err(|failure| format!("git could not be started: {failure}"))?;

    if !output.status.success() {
        return Err(format!(
            "git diff against {base} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_base_that_does_not_exist_is_reported_rather_than_ignored() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives inside the repository");
        let failure = run(root, "no-such-ref-exists-anywhere").expect_err("the ref is not there");
        assert!(
            failure.contains("no-such-ref-exists-anywhere"),
            "the message names the ref it could not find: {failure}"
        );
    }

    #[test]
    fn the_rule_is_about_the_code_directory_and_the_changelog() {
        // Spelled out here so that renaming either one fails a test rather than quietly
        // switching the check off.
        assert_eq!(CODE, "crates/");
        assert_eq!(CHANGELOG, "CHANGELOG.md");
    }
}
