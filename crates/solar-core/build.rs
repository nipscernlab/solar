//! Records build metadata that `solar.version` reports.
//!
//! Everything here degrades to the string `unknown` instead of failing the build: a source
//! tarball with no Git history, or a machine without Git, must still build SOLAR.

use std::process::Command;

fn main() {
    watch_git(std::path::Path::new("../../.git"));
    println!("cargo::rerun-if-env-changed=PROFILE");

    let commit = git(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());
    let short = git(&["rev-parse", "--short=12", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());
    let dirty = match git(&["status", "--porcelain", "--untracked-files=no"]) {
        Some(out) => {
            if out.trim().is_empty() {
                "false"
            } else {
                "true"
            }
        }
        None => "unknown",
    };

    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned());
    let rustc_version = Command::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .unwrap_or_else(|| "unknown".to_owned());

    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "unknown".to_owned());
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_owned());

    println!("cargo::rustc-env=SOLAR_GIT_COMMIT={commit}");
    println!("cargo::rustc-env=SOLAR_GIT_COMMIT_SHORT={short}");
    println!("cargo::rustc-env=SOLAR_GIT_DIRTY={dirty}");
    println!("cargo::rustc-env=SOLAR_RUSTC_VERSION={rustc_version}");
    println!("cargo::rustc-env=SOLAR_BUILD_PROFILE={profile}");
    println!("cargo::rustc-env=SOLAR_TARGET={target}");
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?;
    let trimmed = text.trim().to_owned();
    if trimmed.is_empty() && args.first() == Some(&"rev-parse") {
        return None;
    }
    Some(trimmed)
}

/// Asks cargo to rebuild when the commit changes.
///
/// Watching `HEAD` alone is not enough: committing on a branch leaves `HEAD` untouched and
/// moves the ref it points at, so the ref file is watched as well.
fn watch_git(git_dir: &std::path::Path) {
    let head = git_dir.join("HEAD");
    if !head.is_file() {
        return;
    }
    println!("cargo::rerun-if-changed={}", head.display());

    if let Ok(text) = std::fs::read_to_string(&head)
        && let Some(reference) = text.trim().strip_prefix("ref: ")
    {
        let path = git_dir.join(reference);
        if path.is_file() {
            println!("cargo::rerun-if-changed={}", path.display());
        }
        let packed = git_dir.join("packed-refs");
        if packed.is_file() {
            println!("cargo::rerun-if-changed={}", packed.display());
        }
    }
}
