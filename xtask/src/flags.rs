//! The build flags xtask applies to every cargo invocation it makes.
//!
//! `trim-paths` is what will strip local paths from a binary once it is stable; on the
//! pinned 1.97 it still needs `-Z`, so the fallback is `--remap-path-prefix`. The two
//! prefixes that leak are per machine, the home directory and the repository root, so
//! they cannot live in a committed configuration file: they are computed here, at build
//! time, and `leak-check` verifies the result instead of trusting the list.

use std::path::Path;

/// The neutral spelling the home directory becomes.
pub(crate) const HOME_STANDIN: &str = "/users/anon";

/// The neutral spelling the repository root becomes.
pub(crate) const ROOT_STANDIN: &str = "/solar";

/// Where cargo puts what it builds, honouring `CARGO_TARGET_DIR`.
///
/// Every build xtask makes reads this, so that a task which sets the variable, as
/// `xtask ci` must, still finds the artefacts it asked for.
pub(crate) fn target_dir(root: &Path) -> std::path::PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map_or_else(|| root.join("target"), std::path::PathBuf::from)
}

/// The home directory of whoever is building, in the spelling paths really use.
pub(crate) fn home_directory() -> Option<String> {
    std::env::var("USERPROFILE")
        .ok()
        .or_else(|| std::env::var("HOME").ok())
        .filter(|home| !home.is_empty())
}

/// The `RUSTFLAGS` for a build whose binary must not name this machine.
///
/// The environment variable replaces, rather than extends, the flags in any cargo
/// configuration, which is exactly right here: what this returns is the whole policy.
pub(crate) fn remap_rustflags(root: &Path) -> String {
    let mut flags = Vec::new();
    if let Some(home) = home_directory() {
        flags.push(format!("--remap-path-prefix={home}={HOME_STANDIN}"));
    }
    flags.push(format!(
        "--remap-path-prefix={}={ROOT_STANDIN}",
        root.display()
    ));
    if cfg!(target_os = "macos")
        && let Some(home) = home_directory()
    {
        // Mach-O keeps its debug information as OSO entries naming the object files the
        // linker read, absolute paths that rustc's remap never sees. ld64's `-oso_prefix`
        // strips one prefix from them, and the home directory covers both the target
        // directory and the registry cache.
        flags.push(format!("-Clink-arg=-Wl,-oso_prefix,{home}/"));
    }
    flags.join(" ")
}
