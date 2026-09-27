//! What this binary is: which commit, which compiler, which target.
//!
//! The values are captured by `build.rs` at compile time. Anything Git could not tell it,
//! because there was no repository or no Git, is the string `unknown` rather than a
//! plausible guess.

use schemars::JsonSchema;
use serde::Serialize;

/// The marker every unavailable piece of build metadata carries.
pub const UNKNOWN: &str = "unknown";

/// How this binary was built.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct BuildInfo {
    /// The full Git commit the sources were at, or `unknown`.
    pub git_commit: &'static str,
    /// The first twelve characters of that commit, or `unknown`.
    pub git_commit_short: &'static str,
    /// Whether tracked files differed from that commit when the build ran: `true`, `false`
    /// or `unknown`.
    pub git_dirty: &'static str,
    /// The compiler that built it, as `rustc --version` reports it.
    pub rustc_version: &'static str,
    /// The Cargo profile: `debug`, `release`, or another profile's name.
    pub profile: &'static str,
    /// The target triple this binary runs on.
    pub target: &'static str,
}

/// The metadata of this build.
#[must_use]
pub const fn build_info() -> BuildInfo {
    BuildInfo {
        git_commit: env!("SOLAR_GIT_COMMIT"),
        git_commit_short: env!("SOLAR_GIT_COMMIT_SHORT"),
        git_dirty: env!("SOLAR_GIT_DIRTY"),
        rustc_version: env!("SOLAR_RUSTC_VERSION"),
        profile: env!("SOLAR_BUILD_PROFILE"),
        target: env!("SOLAR_TARGET"),
    }
}

impl BuildInfo {
    /// Whether anything about this build could not be determined.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        ![
            self.git_commit,
            self.git_commit_short,
            self.git_dirty,
            self.rustc_version,
            self.target,
        ]
        .contains(&UNKNOWN)
    }

    /// The names of the members that are `unknown`.
    #[must_use]
    pub fn missing(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        for (name, value) in [
            ("git_commit", self.git_commit),
            ("git_commit_short", self.git_commit_short),
            ("git_dirty", self.git_dirty),
            ("rustc_version", self.rustc_version),
            ("profile", self.profile),
            ("target", self.target),
        ] {
            if value == UNKNOWN {
                missing.push(name);
            }
        }
        missing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_build_reports_a_target_and_a_profile() {
        let info = build_info();
        assert_ne!(info.target, UNKNOWN, "TARGET is always set by cargo");
        assert!(
            info.profile == "debug" || info.profile == "release" || info.profile == "test",
            "unexpected profile {}",
            info.profile
        );
    }

    #[test]
    fn missing_lists_exactly_what_is_unknown() {
        let info = BuildInfo {
            git_commit: UNKNOWN,
            git_commit_short: "abc",
            git_dirty: "false",
            rustc_version: "rustc 1.97.1",
            profile: "debug",
            target: "x86_64-pc-windows-msvc",
        };
        assert_eq!(info.missing(), vec!["git_commit"]);
        assert!(!info.is_complete());
    }
}
