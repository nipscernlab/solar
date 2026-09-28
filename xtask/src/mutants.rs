//! `cargo xtask mutants`: whether the tests would notice if the code were wrong.
//!
//! Coverage says a line ran. Mutation testing changes the line, runs the suite, and sees
//! whether anything fails. A mutant that survives is a line nothing checks, which is a
//! missing test rather than a missing execution.
//!
//! **This runs in continuous integration and not on a laptop.** Decided on 27 September
//! 2026, after a local run filled the disk: `cargo mutants` copies the whole source tree
//! once per job and builds each copy, which was 14.7 GB of temporary directories for
//! eight jobs, on top of a `target/` that had grown to 23.8 GB. The weekly job on a
//! runner does the same work on a machine that is thrown away afterwards, so nothing is
//! lost by refusing to do it here.
//!
//! The refusal is a message rather than a missing command, and `SOLAR_MUTANTS_ANYWAY=1`
//! lifts it for somebody who has the disk and means it.

use std::path::Path;
use std::process::Command;

/// How many surviving mutants the shipped crates are allowed.
///
/// **Measured**, on 27 September 2026, by the weekly job on `ubuntu-latest`: 646 mutants,
/// 483 caught, 13 that hung the suite, which is the suite noticing, 107 that could not be
/// built, and **43 that survived**. The stage that followed killed what could be killed
/// here, and this is what the next run has to beat.
///
/// The number that stood here before was 98, and it was not a measurement: it came from a
/// run in which mutants were judged by the mutated crate's own tests rather than by the
/// workspace suite, and in which a shared target directory let one mutant be judged by an
/// artefact built from another. Both were fixed in stage three.
///
/// **A mutant inside a `cfg` block for another operating system cannot be judged by this
/// job**, which runs on Linux: the code is never compiled, so changing it changes nothing
/// and the mutant always survives. Eleven of the survivors are those. The architect
/// approved, on 27 September 2026, running the job on all three systems in stage five, so
/// that each of them is judged where it is compiled; `STATUS.md` records that decision.
///
/// It only ever goes down. Lower it whenever some are killed; never raise it.
pub(crate) const CEILING: usize = 43;

/// Runs the mutation suite.
///
/// # Errors
///
/// Returns that mutants survived, or why the run could not be made.
pub(crate) fn run(root: &Path, arguments: &[&str]) -> Result<(), String> {
    if !runs_here() {
        return Err(
            "mutation testing runs in continuous integration, not on a laptop. cargo              mutants copies the whole source tree once per job and builds every copy,              which filled this disk on 27 September 2026: 14.7 GB of temporary trees on              top of a target directory of 23.8 GB. The weekly job does the same work on              a runner that is thrown away afterwards, and the report is an artefact of              it. Set SOLAR_MUTANTS_ANYWAY=1 if you have the disk and mean it."
                .to_owned(),
        );
    }

    let output = root.join("target").join("mutants");
    let mut command = Command::new("cargo");
    command
        .args(["mutants", "--no-shuffle", "--output"])
        .arg(&output)
        .args(arguments)
        .current_dir(root);

    // No CARGO_TARGET_DIR here, on purpose, and it is not an oversight to be tidied up.
    // cargo-mutants copies the source tree once per job and builds each copy in that
    // copy's own target directory. One shared target directory makes cargo reuse a test
    // binary built in another copy, and a test binary carries the `CARGO_MANIFEST_DIR`
    // of the tree that compiled it: `tests/docs.rs` then reads the contract out of a
    // directory that has already been deleted, and a mutant can be judged by an artefact
    // built from a different mutant. The running xtask binary is safe without it,
    // because nothing here builds in the real tree.

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

/// Whether this machine is one where the mutation suite may run.
///
/// Continuous integration sets `CI`, which every runner does and no laptop does by
/// accident. `SOLAR_MUTANTS_ANYWAY` is the deliberate override.
fn runs_here() -> bool {
    let set = |name: &str| std::env::var(name).is_ok_and(|value| !value.is_empty());
    set("CI") || set("SOLAR_MUTANTS_ANYWAY")
}
