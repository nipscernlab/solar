//! Records build metadata that `solar.version` reports.
//!
//! Everything here degrades to the string `unknown` instead of failing the build: a source
//! tarball with no Git history, or a machine without Git, must still build SOLAR.

use std::process::Command;

fn main() {
    let git_dir = std::path::Path::new("../../.git");
    if git_dir.join("HEAD").is_file() {
        println!("cargo::rerun-if-changed=../../.git/HEAD");
    }
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
