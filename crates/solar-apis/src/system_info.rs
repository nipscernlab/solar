//! `system.info`: the machine SOLAR is running on, and nothing about who runs it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use solar_core::api::{
    ANY, Api, ApiSpec, DEFAULT_MAX_OUTPUT_BYTES, ErrorSpec, Example, SideEffect, Stability,
};
use solar_core::context::Context;
use solar_core::error::{ErrorDetail, SolarError};
use solar_core::reason::Reason;
use solar_core::status::Status;
use solar_core::warning::WarningCode;

/// The parameters of `system.info`, of which there are none.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Params {}

/// The output of `system.info`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Output {
    /// What Rust calls this system: `windows`, `linux`, `macos`, and so on.
    pub os: String,
    /// `windows` or `unix`.
    pub os_family: String,
    /// What the system calls itself: a distribution on Linux, `macOS`, `Windows`.
    /// `null` when it did not say, and then a warning says why.
    pub os_name: Option<String>,
    /// The release, as the system spells it: `24.04`, `15.0`, `10.0.26200`.
    pub os_release: Option<String>,
    /// The build, for the systems that count builds apart from releases.
    pub os_build: Option<String>,
    /// The processor architecture: `x86_64`, `aarch64`, and so on.
    pub arch: String,
    /// The width of a pointer in bits, which is how wide this build is.
    pub pointer_width: u32,
    /// How many threads can run at once, `null` when the system does not say.
    pub cpu_count: Option<u32>,
    /// The directory SOLAR is running in.
    pub current_dir: String,
    /// The path of the `solar` binary that is answering.
    pub executable: String,
    /// The directory for temporary files.
    pub temp_dir: String,
    /// What separates the parts of a path: a backslash or a slash.
    pub path_separator: String,
    /// What separates the entries of the `PATH` variable: a semicolon or a colon.
    pub path_list_separator: String,
    /// How many handlers overran their budget and are still running.
    ///
    /// A handler that overruns is abandoned rather than killed, so it keeps its thread
    /// until it finishes on its own. A healthy process reports zero. When this reaches
    /// the cap of section 10 of the contract, new calls are refused with
    /// `TOO_MANY_ABANDONED` until some of them finish.
    pub abandoned_workers: u32,
}

/// Reports the machine and the process.
#[derive(Debug)]
pub struct SystemInfo;

impl Api for SystemInfo {
    const NAME: &'static str = "system.info";
    const VERSION: &'static str = "1.2.0";
    type Params = Params;
    type Output = Output;

    fn spec() -> ApiSpec {
        ApiSpec {
            summary: "Reports the operating system, the processor and the process",
            description: "\
What a caller needs in order to know which machine it is talking to: the operating system \
with its name and release, its family, the architecture, the width of a pointer, how many \
threads run at once, the current directory, the path of the binary that is answering, the \
temporary directory, and the two separators that differ between Windows and the rest, \
which is what makes a path in a response usable without guessing. It also reports \
`abandoned_workers`, the handlers that overran their budget and are still running, which \
is zero in a healthy process.\n\n\
The release is read where each system keeps it, and never by starting a program: the \
os-release file on Linux, `SystemVersion.plist` on macOS, and `RtlGetVersion` on Windows, \
which reports the real version where the documented alternative reports a compatibility \
lie. When that source is missing or unreadable, `os_name`, `os_release` and `os_build` \
are null and the call warns with OS_RELEASE_UNAVAILABLE saying which source was tried. \
Nothing is guessed, and the call still succeeds.\n\n\
Nothing personal is reported beyond what those paths necessarily contain: a home \
directory usually holds a user name, and that is the only such thing here. No environment \
variables, no network names, no serial numbers.",
            errors: vec![ErrorSpec::of(Reason::EnvironmentUnavailable)],
            side_effects: vec![SideEffect::ReadsFilesystem],
            idempotent: true,
            stability: Stability::Experimental,
            since: "0.1.0",
            timeout_ms: 2_000,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            examples: vec![Example::subset(
                "plain",
                "The machine this call ran on",
                json!({}),
                json!({
                    "os": ANY,
                    "os_family": ANY,
                    "os_name": ANY,
                    "os_release": ANY,
                    "os_build": ANY,
                    "arch": ANY,
                    "pointer_width": ANY,
                    "cpu_count": ANY,
                    "current_dir": ANY,
                    "executable": ANY,
                    "temp_dir": ANY,
                    "path_separator": ANY,
                    "path_list_separator": ANY,
                    "abandoned_workers": ANY
                }),
            )],
        }
    }

    fn call(ctx: &Context, _params: Params) -> Result<Output, SolarError> {
        let current_dir = std::env::current_dir()
            .map_err(|failure| unavailable("the current directory", &failure.to_string()))?;
        let executable = std::env::current_exe()
            .map_err(|failure| unavailable("its own executable", &failure.to_string()))?;
        let cpu_count = std::thread::available_parallelism()
            .ok()
            .and_then(|count| u32::try_from(count.get()).ok());

        let release = match crate::os_release::read() {
            Ok(release) => release,
            Err(why) => {
                ctx.warn(
                    WarningCode::OsReleaseUnavailable,
                    format!("This system did not say which release it is: {why}."),
                );
                crate::os_release::Release::default()
            }
        };

        Ok(Output {
            os: std::env::consts::OS.to_owned(),
            os_family: std::env::consts::FAMILY.to_owned(),
            os_name: release.name,
            os_release: release.release,
            os_build: release.build,
            arch: std::env::consts::ARCH.to_owned(),
            pointer_width: usize::BITS,
            cpu_count,
            abandoned_workers: u32::try_from(solar_core::dispatch::abandoned_workers())
                .unwrap_or(u32::MAX),
            current_dir: current_dir.display().to_string(),
            executable: executable.display().to_string(),
            temp_dir: std::env::temp_dir().display().to_string(),
            path_separator: std::path::MAIN_SEPARATOR.to_string(),
            path_list_separator: if cfg!(windows) {
                ";".to_owned()
            } else {
                ":".to_owned()
            },
        })
    }
}

/// The error something SOLAR cannot read about its own process becomes.
fn unavailable(what: &str, failure: &str) -> SolarError {
    SolarError::new(
        Reason::EnvironmentUnavailable,
        format!("SOLAR could not read {what}: {failure}."),
    )
    .with_detail(
        ErrorDetail::new(Status::Unavailable)
            .field(what.to_owned())
            .expected("a readable path")
            .received(failure)
            .hint(
                "The usual cause is that the directory SOLAR was started in has been \
                     deleted. Start it somewhere that exists.",
            ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::context;

    #[test]
    fn the_machine_reports_itself_completely() {
        let output = SystemInfo::call(&context("system.info"), Params {}).unwrap();
        assert_eq!(output.os, std::env::consts::OS);
        assert_eq!(output.arch, std::env::consts::ARCH);
        assert!(output.os_family == "windows" || output.os_family == "unix");
        assert!(!output.current_dir.is_empty());
        assert!(!output.temp_dir.is_empty());
        assert!(output.cpu_count.unwrap_or(1) >= 1);
        assert!(output.pointer_width == 64 || output.pointer_width == 32);
        assert_eq!(
            output.abandoned_workers, 0,
            "a healthy process has abandoned nothing"
        );
    }

    #[test]
    fn the_release_is_reported_or_the_call_says_why_it_is_not() {
        let ctx = context("system.info");
        let output = SystemInfo::call(&ctx, Params {}).unwrap();
        let warned = ctx
            .warnings()
            .iter()
            .any(|w| w.code == WarningCode::OsReleaseUnavailable);

        if warned {
            assert_eq!(
                output.os_name, None,
                "a warned call reports nothing about the release"
            );
            assert_eq!(output.os_release, None);
            assert_eq!(output.os_build, None);
        } else {
            assert!(
                output.os_name.is_some(),
                "a system that answered must have a name"
            );
            assert!(
                output.os_release.is_some() || output.os_build.is_some(),
                "a system that answered must have a release or a build: {output:?}"
            );
        }
    }

    #[test]
    fn this_machine_answers_about_its_release_rather_than_warning() {
        // Every system the workspace supports has a place to read this from. A warning
        // here is a gap in one of the three readers, not a quirk of the machine.
        let ctx = context("system.info");
        let output = SystemInfo::call(&ctx, Params {}).unwrap();
        let warnings = ctx.warnings();
        assert!(
            warnings.is_empty(),
            "the release could not be read on this system: {warnings:?}"
        );
        assert!(output.os_release.is_some(), "{output:?}");
    }

    #[test]
    fn the_separators_are_the_ones_this_system_really_uses() {
        let output = SystemInfo::call(&context("system.info"), Params {}).unwrap();
        if cfg!(windows) {
            assert_eq!(output.path_list_separator, ";");
        } else {
            assert_eq!(output.path_list_separator, ":");
            assert_eq!(output.path_separator, "/");
        }
    }

    #[test]
    fn the_executable_it_reports_is_the_one_that_is_running() {
        let output = SystemInfo::call(&context("system.info"), Params {}).unwrap();
        let expected = std::env::current_exe().unwrap().display().to_string();
        assert_eq!(output.executable, expected);
    }
}
