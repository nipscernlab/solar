//! `cargo xtask leak-check`: no binary carries the name of whoever built it.
//!
//! The check builds the release binary with the remap flags of [`crate::flags`] and then
//! reads every byte of it looking for the two local paths those flags are meant to
//! remove: the home directory and the repository root. Scanning the artifact is the only
//! honest form of this check; a list of flags proves nothing about what the linker kept.

use std::path::Path;
use std::process::Command;

use crate::flags;

/// Builds the release binary clean and proves it.
///
/// # Errors
///
/// Returns what leaked and where it sat, or why the check could not run.
pub(crate) fn run(root: &Path) -> Result<(), String> {
    let rustflags = flags::remap_rustflags(root);
    let status = Command::new("cargo")
        .args([
            "build",
            "--release",
            "--locked",
            "--package",
            "solar-cli",
            "--quiet",
        ])
        .current_dir(root)
        .env("RUSTFLAGS", &rustflags)
        .status()
        .map_err(|failure| format!("cargo could not be started: {failure}"))?;
    if !status.success() {
        return Err("the release build failed, so there is nothing to check".to_owned());
    }

    let binary = root
        .join("target")
        .join("release")
        .join(format!("solar{}", std::env::consts::EXE_SUFFIX));
    let bytes = std::fs::read(&binary)
        .map_err(|failure| format!("{} could not be read: {failure}", binary.display()))?;

    let mut needles: Vec<(String, &'static str)> = Vec::new();
    if let Some(home) = flags::home_directory() {
        needles.push((home.clone(), "the home directory"));
        // The user name alone, as a path segment, catches a path that dodged the prefix
        // remap through a different spelling of the directories above it.
        if let Some(name) = Path::new(&home).file_name().and_then(|name| name.to_str()) {
            needles.push((format!("\\{name}\\"), "the user name as a path segment"));
            needles.push((format!("/{name}/"), "the user name as a path segment"));
        }
    }
    needles.push((root.display().to_string(), "the repository root"));

    let mut leaks = Vec::new();
    for (needle, what) in &needles {
        for variant in spellings(needle) {
            if let Some(at) = find(&bytes, variant.as_bytes()) {
                let context = String::from_utf8_lossy(
                    &bytes[at.saturating_sub(20)..(at + variant.len() + 40).min(bytes.len())],
                )
                .into_owned();
                leaks.push(format!("{what} ({variant}) at byte {at}: ...{context}..."));
                break;
            }
        }
    }

    if leaks.is_empty() {
        println!(
            "leak-check: {} is clean of {} needles ({} bytes scanned)",
            binary.display(),
            needles.len(),
            bytes.len()
        );
        Ok(())
    } else {
        Err(format!(
            "the release binary embeds local paths:\n  {}\nThe remap flags are computed \
             in xtask/src/flags.rs; something is building without them.",
            leaks.join("\n  ")
        ))
    }
}

/// A path in the spellings Windows tools produce: as given, and slash flipped.
fn spellings(path: &str) -> Vec<String> {
    let flipped: String = path
        .chars()
        .map(|character| match character {
            '\\' => '/',
            '/' => '\\',
            other => other,
        })
        .collect();
    if flipped == path {
        vec![path.to_owned()]
    } else {
        vec![path.to_owned(), flipped]
    }
}

/// The position of a byte needle in a haystack, `None` when it is not there.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
