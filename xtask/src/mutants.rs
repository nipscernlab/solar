//! `cargo xtask mutants`: whether the tests would notice if the code were wrong.
//!
//! Coverage says a line ran. Mutation testing changes the line, runs the suite, and sees
//! whether anything fails. A mutant that survives is a line nothing checks, which is a
//! missing test rather than a missing execution.
//!
//! The full run takes a long time, which is why it is a weekly job and a command rather
//! than a step of every push. `--in-diff` narrows it to what a branch changed, which is
//! short enough to run before opening a pull request.

use std::path::Path;
use std::process::Command;

/// How many surviving mutants the shipped crates are allowed, measured on 27 September
/// 2026 and never raised.
///
/// A weekly job that is always red is a job nobody reads, and a full run today leaves 98
/// survivors out of 538 mutants: real gaps, but a backlog rather than a regression. The
/// ceiling makes the job green while the backlog shrinks and red the moment somebody adds
/// to it, which is the bargain the coverage floor makes as well.
///
/// Every survivor is a line that can be wrong without a test failing. Lower this number
/// whenever some are killed; never raise it.
pub(crate) const CEILING: usize = 98;

/// Runs the mutation suite.
///
/// # Errors
///
/// Returns that mutants survived, or why the run could not be made.
pub(crate) fn run(root: &Path, arguments: &[&str]) -> Result<(), String> {
    let output = root.join("target").join("mutants");
    let mut command = Command::new("cargo");
    command
        .args(["mutants", "--no-shuffle", "--output"])
        .arg(&output)
        .args(arguments)
        .current_dir(root)
        // Its own target directory, like every other nested build: the running xtask
        // binary must not be relinked underneath itself.
        .env(
            "CARGO_TARGET_DIR",
            crate::flags::target_dir(root).join("ci"),
        );

    let status = command.status().map_err(|failure| {
        format!(
            "cargo mutants could not be started: {failure}. Install it with              `cargo install cargo-mutants --locked`."
        )
    })?;

    // The exit code alone does not distinguish a mutant nothing noticed from one that
    // hung the suite, and those are different news: a hang is the suite noticing, loudly.
    // The outcomes file says which happened.
    let summary = read_outcomes(&output)?;
    println!(
        "mutants: {} caught, {} survived, {} hung the suite, {} could not be built",
        summary.caught, summary.missed, summary.timeout, summary.unviable
    );

    if summary.missed > CEILING {
        return Err(format!(
            "{} mutants survived, and the ceiling is {CEILING}. Those lines can be wrong              without a test failing, and there are more of them than there were. The list              is in {}/mutants.out/missed.txt.",
            summary.missed,
            output.display()
        ));
    }
    if summary.missed > 0 {
        println!(
            "mutants: {} survived, within the ceiling of {CEILING}. Each one is a line              nothing checks: {}/mutants.out/missed.txt. Lower the ceiling in              xtask/src/mutants.rs whenever you kill some.",
            summary.missed,
            output.display()
        );
        return Ok(());
    }
    if summary.caught == 0 && summary.timeout == 0 {
        return Err(format!(
            "no mutant was tested at all, which means the run did not work: exit code              {:?}. The log is in {}/mutants.out.",
            status.code(),
            output.display()
        ));
    }
    println!(
        "mutants: every mutant was caught; the report is in {}",
        output.display()
    );
    Ok(())
}

/// How a run of the mutation suite came out.
#[derive(Debug, Default)]
struct Summary {
    caught: usize,
    missed: usize,
    timeout: usize,
    unviable: usize,
}

/// Reads the outcome of a run from the file cargo-mutants writes.
fn read_outcomes(output: &Path) -> Result<Summary, String> {
    let path = output.join("mutants.out").join("outcomes.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|failure| format!("{} could not be read: {failure}", path.display()))?;
    let parsed: serde_json::Value = serde_json::from_str(&text)
        .map_err(|failure| format!("{} is not valid JSON: {failure}", path.display()))?;

    let mut summary = Summary::default();
    for outcome in parsed
        .get("outcomes")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        match outcome.get("summary").and_then(serde_json::Value::as_str) {
            // `Success` is the unmutated baseline, which is not a mutant.
            Some("CaughtMutant") => summary.caught += 1,
            Some("MissedMutant") => summary.missed += 1,
            // A mutant that makes the suite hang is a mutant the suite noticed.
            Some("Timeout") => summary.timeout += 1,
            Some("Unviable") => summary.unviable += 1,
            _ => {}
        }
    }
    Ok(summary)
}
