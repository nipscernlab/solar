//! Development tasks, run as `cargo xtask <task>`.
//!
//! Two tasks, both of them about keeping the repository honest:
//!
//! - `manifest` regenerates `manifest/solar.manifest.json` from the registry, and
//!   `manifest --check` says whether it is stale without writing anything.
//! - `new-api <name>` writes a new API from the template and registers it.
//!
//! Nothing here is a dependency of the `solar` binary.

// xtask is a developer tool, not a protocol server. Standard output is where it talks, so
// the rule that keeps stdout clear of everything but protocol does not apply here.
#![allow(
    clippy::print_stdout,
    reason = "xtask is a developer tool that talks on standard output; the channel rule \n              protects the protocol, which xtask never speaks"
)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod changelog;
mod ci;
mod compat;
mod coverage;
mod doc_run;
mod flags;
mod leak_check;
mod mutants;
mod new_api;

/// The exit code of a task that failed.
const FAILED: u8 = 1;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let task = arguments.first().map_or("help", String::as_str);
    let rest: Vec<&str> = arguments.iter().skip(1).map(String::as_str).collect();

    let outcome = match task {
        "manifest" => manifest(rest.contains(&"--check")),
        "ci" => ci::run(&root(), rest.contains(&"--fast")),
        "coverage" => coverage::run(&root(), rest.contains(&"--report")),
        "mutants" => mutants::run(&root(), &rest),
        "changelog" => match rest.first() {
            Some(base) if !base.starts_with('-') => changelog::run(&root(), base),
            _ => changelog::run(&root(), "main"),
        },
        "compat" => match rest.first() {
            Some(base) if !base.starts_with('-') => compat::run(&root(), base),
            // `main` is what a pull request is compared against by default.
            _ => compat::run(&root(), "main"),
        },
        "doc-run" => doc_run::run(&root(), &flags::target_dir(&root())),
        "leak-check" => leak_check::run(&root(), &flags::target_dir(&root())),
        "new-api" => match rest.first() {
            Some(name) if !name.starts_with('-') => new_api::run(&root(), name),
            _ => Err(
                "new-api needs a method name, for example `cargo xtask new-api build.run`"
                    .to_owned(),
            ),
        },
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => Err(format!(
            "there is no task called {other}. Run `cargo xtask help`."
        )),
    };

    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("xtask: {message}");
            ExitCode::from(FAILED)
        }
    }
}

fn print_help() {
    eprintln!(
        "\
cargo xtask <task>

  manifest            regenerate manifest/solar.manifest.json from the registry
  manifest --check    report whether the versioned manifest is stale, write nothing
  ci                  everything CI runs, in the same order, with a summary
  ci --fast           the same without the documentation runner and the coverage step,
                      which are most of the wall clock; the full command gates a push
  changelog [<base>]  whether the code changed against <base>, default main, without
                      CHANGELOG.md moving with it
  compat [<base>]     what changed in the manifest against <base>, default main, and
                      whether the version of each API answers for it
  coverage [--report] how much of the shipped code the tests run, against the floor;
                      --report also writes lcov and HTML into target/coverage
  mutants [args...]   change the code and see whether a test fails; --in-diff origin/main
                      narrows it to what this branch touched
  doc-run             run every shell-tagged block of the documentation in its shell
  leak-check          prove the release binary embeds no local path
  new-api <name>      write a new API from the template and register it
  help                this text"
    );
}

/// The root of the repository, found from where this crate lives.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

/// Reports whether the versioned manifest is what the generator produces.
///
/// # Errors
///
/// Returns why it is stale, or why the registry could not be built.
pub(crate) fn manifest_check(_root: &Path) -> Result<(), String> {
    manifest(true)
}

/// Regenerates the manifest, or reports that it is stale.
fn manifest(check_only: bool) -> Result<(), String> {
    let registry = solar_apis::registry().map_err(|problems| {
        let listed: Vec<String> = problems.iter().map(ToString::to_string).collect();
        format!("the registry does not build:\n  {}", listed.join("\n  "))
    })?;

    let generated = solar_core::manifest::render(registry);
    let path = root().join("manifest").join("solar.manifest.json");
    let current = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");

    if current == generated {
        println!(
            "manifest: up to date ({} APIs, {})",
            registry.len(),
            path.display()
        );
        return Ok(());
    }

    if check_only {
        return Err(format!(
            "{} is stale. Run `cargo xtask manifest` and commit the result.",
            path.display()
        ));
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|failure| format!("{} could not be created: {failure}", parent.display()))?;
    }
    std::fs::write(&path, generated.as_bytes())
        .map_err(|failure| format!("{} could not be written: {failure}", path.display()))?;
    println!(
        "manifest: written ({} APIs, {})",
        registry.len(),
        path.display()
    );
    Ok(())
}
