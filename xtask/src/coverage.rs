//! `cargo xtask coverage`: how much of the shipped code the tests really run.
//!
//! Coverage is not a score to admire. It is a list of the lines nobody has ever executed,
//! and the value of the number is that it cannot fall: [`FLOOR`] is what was measured
//! when this was written, rounded down, and CI refuses a change that goes below it.
//!
//! What is measured is the three crates that ship: `solar-core`, `solar-apis` and
//! `solar-cli`. `xtask` is left out on purpose: it is run by the pipeline rather than by
//! the tests, since `ci`, `doc-run` and `leak-check` *are* the pipeline. Measuring it
//! would count the harness as if it were the product, and would push the floor down
//! every time a task is added.
//!
//! Coverage says which lines ran. It does not say whether anything checked what they did,
//! which is what `cargo xtask mutants` is for.

use std::path::Path;
use std::process::Command;

/// The floor, in percent of lines, measured on 27 September 2026 and rounded down.
///
/// It only ever goes up. Raising it is a separate change, made when a measurement has
/// stayed comfortably above the current floor for a while.
///
/// Stage two measured 91.33% and set the floor at 91. Stage three measured **93.75%**,
/// and the floor stays at 91 on purpose: one measurement above it is not the same as a
/// habit of staying above it, and a floor raised on a single run is a floor that fails
/// the first time somebody adds a hard-to-test branch for a good reason. Raise it to 93
/// once a few stages have run above it.
pub(crate) const FLOOR: u32 = 91;

/// The crates that ship, and therefore the crates that are measured.
const MEASURED: [&str; 3] = ["solar-core", "solar-apis", "solar-cli"];

/// Runs the tests under instrumentation and reports the coverage.
///
/// # Errors
///
/// Returns why the run failed, or that the coverage fell below [`FLOOR`].
pub(crate) fn run(root: &Path, write_reports: bool) -> Result<(), String> {
    let target = crate::flags::target_dir(root).join("ci");
    let reports = root.join("target").join("coverage");
    if write_reports {
        std::fs::create_dir_all(&reports)
            .map_err(|failure| format!("{} could not be created: {failure}", reports.display()))?;
    }

    let mut arguments: Vec<String> = vec![
        "llvm-cov".to_owned(),
        "nextest".to_owned(),
        "--locked".to_owned(),
        "--ignore-filename-regex".to_owned(),
        "xtask".to_owned(),
        "--fail-under-lines".to_owned(),
        FLOOR.to_string(),
    ];
    for crate_name in MEASURED {
        arguments.push("--package".to_owned());
        arguments.push(crate_name.to_owned());
    }
    if write_reports {
        arguments.push("--lcov".to_owned());
        arguments.push("--output-path".to_owned());
        arguments.push(reports.join("lcov.info").display().to_string());
    } else {
        arguments.push("--summary-only".to_owned());
    }

    let status = Command::new("cargo")
        .args(&arguments)
        .current_dir(root)
        .env("CARGO_TARGET_DIR", &target)
        .status()
        .map_err(|failure| {
            format!(
                "cargo llvm-cov could not be started: {failure}. Install it \
                                    with `cargo install cargo-llvm-cov --locked`."
            )
        })?;

    if !status.success() {
        return Err(format!(
            "the coverage of the shipped crates is below the floor of {FLOOR}% of lines. \
             Either the change needs tests, or the floor is wrong and lowering it is a \
             decision, not a fix."
        ));
    }

    if write_reports {
        // The same run again for the report a person reads. `--no-run` reuses the
        // profile data just gathered, so nothing is executed twice.
        let html = Command::new("cargo")
            .args(["llvm-cov", "report", "--html", "--output-dir"])
            .arg(reports.join("html"))
            .args(["--ignore-filename-regex", "xtask"])
            .current_dir(root)
            .env("CARGO_TARGET_DIR", &target)
            .status()
            .map_err(|failure| format!("the HTML report could not be written: {failure}"))?;
        if !html.success() {
            return Err("the HTML report could not be written".to_owned());
        }
        println!("coverage: reports in {}", reports.display());
    }

    Ok(())
}
