//! `system.info`: the machine SOLAR is running on, and nothing about who runs it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use solar_core::api::{ANY, Api, ApiSpec, ErrorSpec, Example, SideEffect, Stability};
use solar_core::context::Context;
use solar_core::error::{ErrorDetail, SolarError};
use solar_core::reason::Reason;
use solar_core::status::Status;

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
}

/// Reports the machine and the process.
#[derive(Debug)]
pub struct SystemInfo;

impl Api for SystemInfo {
    const NAME: &'static str = "system.info";
    const VERSION: &'static str = "1.0.0";
    type Params = Params;
    type Output = Output;

    fn spec() -> ApiSpec {
        ApiSpec {
            summary: "Reports the operating system, the processor and the process",
            description: "\
What a caller needs in order to know which machine it is talking to: the operating system \
and its family, the architecture, the width of a pointer, how many threads run at once, \
the current directory, the path of the binary that is answering, the temporary directory, \
and the two separators that differ between Windows and the rest, which is what makes a \
path in a response usable without guessing.\n\n\
Everything here comes from the standard library and from this process. Nothing is asked \
of the operating system through a helper program, which is why the release of the \
operating system is not reported yet: on Unix that means running `sw_vers` or \
`lsb_release`, and `solar/1` starts no external program.\n\n\
Nothing personal is reported beyond what those paths necessarily contain: a home \
directory usually holds a user name, and that is the only such thing here. No environment \
variables, no network names, no serial numbers.",
            errors: vec![ErrorSpec::of(Reason::EnvironmentUnavailable)],
            side_effects: vec![SideEffect::ReadsFilesystem],
            idempotent: true,
            stability: Stability::Experimental,
            since: "0.1.0",
            timeout_ms: 2_000,
            examples: vec![Example::subset(
                "plain",
                "The machine this call ran on",
                json!({}),
                json!({
                    "os": ANY,
                    "os_family": ANY,
                    "arch": ANY,
                    "pointer_width": ANY,
                    "cpu_count": ANY,
                    "current_dir": ANY,
                    "executable": ANY,
                    "temp_dir": ANY,
                    "path_separator": ANY,
                    "path_list_separator": ANY
                }),
            )],
        }
    }

    fn call(_ctx: &Context, _params: Params) -> Result<Output, SolarError> {
        let current_dir = std::env::current_dir()
            .map_err(|failure| unavailable("the current directory", &failure.to_string()))?;
        let executable = std::env::current_exe()
            .map_err(|failure| unavailable("its own executable", &failure.to_string()))?;
        let cpu_count = std::thread::available_parallelism()
            .ok()
            .and_then(|count| u32::try_from(count.get()).ok());

        Ok(Output {
            os: std::env::consts::OS.to_owned(),
            os_family: std::env::consts::FAMILY.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
            pointer_width: usize::BITS,
            cpu_count,
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
