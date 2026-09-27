//! `cargo xtask ci`: what continuous integration runs, on this machine, in that order.
//!
//! The point is that a green run here means a green run there. Every step below is the
//! same command with the same flags as the corresponding step of
//! `.github/workflows/ci.yml`, and the workflow calls this task's siblings wherever a
//! step is xtask's own, so the two cannot drift apart without a test noticing.
//!
//! Nothing stops at the first failure: a run reports every step, so one command tells you
//! everything that is wrong rather than the first thing.

use std::path::Path;
use std::process::Command;
use std::time::Instant;

/// What one step did.
struct Outcome {
    name: &'static str,
    passed: bool,
    seconds: f64,
    note: Option<String>,
}

/// Runs the whole pipeline.
///
/// # Errors
///
/// Returns a sentence naming the steps that failed, after every step has run.
pub(crate) fn run(root: &Path) -> Result<(), String> {
    // The order is the order of .github/workflows/ci.yml, step for step.
    let outcomes = vec![
        cargo(root, "formatting", &["fmt", "--all", "--", "--check"]),
        tool(
            root,
            "toml formatting",
            "taplo",
            &["fmt", "--check"],
            "taplo-cli",
        ),
        tool(root, "spelling", "typos", &[], "typos-cli"),
        cargo(
            root,
            "lints",
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--locked",
                "--",
                "-D",
                "warnings",
            ],
        ),
        cargo(root, "tests", &["test", "--workspace", "--locked"]),
        documentation(root),
        step("manifest", || crate::manifest_check(root)),
        step("documentation runs", || crate::doc_run::run(root)),
        step("no local paths", || crate::leak_check::run(root)),
    ];

    report(&outcomes)
}

/// Prints the summary and turns it into the result of the task.
fn report(outcomes: &[Outcome]) -> Result<(), String> {
    let total: f64 = outcomes.iter().map(|outcome| outcome.seconds).sum();
    println!();
    println!("  step                        result    seconds");
    println!("  --------------------------  --------  -------");
    for outcome in outcomes {
        let result = if outcome.passed { "ok" } else { "FAILED" };
        println!(
            "  {:<26}  {result:<8}  {:>7.1}",
            outcome.name, outcome.seconds
        );
        if let Some(note) = &outcome.note {
            println!("      {note}");
        }
    }
    let failed: Vec<&str> = outcomes
        .iter()
        .filter(|outcome| !outcome.passed)
        .map(|outcome| outcome.name)
        .collect();
    println!();

    if failed.is_empty() {
        println!("ci: every step passed in {total:.1} s");
        Ok(())
    } else {
        Err(format!(
            "{} of {} steps failed: {}",
            failed.len(),
            outcomes.len(),
            failed.join(", ")
        ))
    }
}

/// Runs one cargo command as a step.
fn cargo(root: &Path, name: &'static str, arguments: &[&str]) -> Outcome {
    external(root, name, "cargo", arguments, None)
}

/// Runs one installed tool as a step, saying how to install it when it is absent.
fn tool(
    root: &Path,
    name: &'static str,
    program: &str,
    arguments: &[&str],
    crate_name: &str,
) -> Outcome {
    external(
        root,
        name,
        program,
        arguments,
        Some(format!(
            "{program} is not installed: cargo install {crate_name} --locked"
        )),
    )
}

/// The documentation build, which needs one environment variable.
fn documentation(root: &Path) -> Outcome {
    let started = Instant::now();
    let status = Command::new("cargo")
        .args(["doc", "--workspace", "--no-deps", "--locked", "--quiet"])
        .current_dir(root)
        .env("RUSTDOCFLAGS", "-D warnings")
        .env("RUSTFLAGS", crate::flags::remap_rustflags(root))
        .status();
    finish(
        "documentation",
        started,
        status.is_ok_and(|status| status.success()),
        None,
    )
}

/// Runs an external program as a step.
fn external(
    root: &Path,
    name: &'static str,
    program: &str,
    arguments: &[&str],
    missing: Option<String>,
) -> Outcome {
    let started = Instant::now();
    println!("ci: {name}");
    let outcome = Command::new(program)
        .args(arguments)
        .current_dir(root)
        .env("RUSTFLAGS", crate::flags::remap_rustflags(root))
        .status();

    match outcome {
        Ok(status) => finish(name, started, status.success(), None),
        Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => finish(
            name,
            started,
            false,
            missing.or_else(|| Some(format!("{program} not found"))),
        ),
        Err(failure) => finish(name, started, false, Some(failure.to_string())),
    }
}

/// Runs one of xtask's own tasks as a step.
fn step(name: &'static str, task: impl FnOnce() -> Result<(), String>) -> Outcome {
    let started = Instant::now();
    println!("ci: {name}");
    match task() {
        Ok(()) => finish(name, started, true, None),
        Err(why) => finish(name, started, false, Some(first_line(&why))),
    }
}

/// Assembles an outcome and says at once whether it passed.
fn finish(name: &'static str, started: Instant, passed: bool, note: Option<String>) -> Outcome {
    let seconds = started.elapsed().as_secs_f64();
    if !passed {
        println!("ci: {name} FAILED");
    }
    Outcome {
        name,
        passed,
        seconds,
        note,
    }
}

/// The first line of a message, for a summary table that must stay readable.
fn first_line(message: &str) -> String {
    message.lines().next().unwrap_or(message).trim().to_owned()
}
