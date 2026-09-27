//! `solar.set_log_level`: change how much SOLAR says about itself, without restarting it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use solar_core::api::{
    ANY, Api, ApiSpec, DEFAULT_MAX_OUTPUT_BYTES, ErrorSpec, Example, SideEffect, Stability,
};
use solar_core::context::Context;
use solar_core::error::{ErrorDetail, SolarError};
use solar_core::logging::{self, Level};
use solar_core::reason::Reason;
use solar_core::status::Status;

/// The parameters of `solar.set_log_level`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The level to log at from now on: `off`, `error`, `warn`, `info`, `debug` or
    /// `trace`. Read in any case, and `none` and `0` are accepted for `off` and
    /// `warning` for `warn`, exactly as `SOLAR_LOG` reads them.
    pub level: String,
}

/// The output of `solar.set_log_level`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Output {
    /// The level that was in force before this call.
    pub previous: String,
    /// The level in force now, which is what was asked for.
    pub current: String,
    /// Whether this call changed anything. Setting the level it already had is not an
    /// error; it simply answers with `false` here.
    pub changed: bool,
}

/// Changes the level of the diagnostics on standard error.
#[derive(Debug)]
pub struct SolarSetLogLevel;

impl Api for SolarSetLogLevel {
    const NAME: &'static str = "solar.set_log_level";
    const VERSION: &'static str = "1.0.0";
    type Params = Params;
    type Output = Output;

    fn spec() -> ApiSpec {
        ApiSpec {
            summary: "Changes how much SOLAR says about itself, for the rest of the process",
            description: "\
`SOLAR_LOG` and `--log` decide what a session starts at, and until now that was the only \
way to decide at all: an interface that wanted to show more had to restart SOLAR, losing \
the session and everything queued in it. This changes the level while the session runs.\n\n\
**It touches only where diagnostics go, which is standard error.** Standard output \
carries protocol and nothing else, whatever the level, which is a promise of the contract \
and a test. Nothing about a response changes: the same call answers the same way at \
`off` and at `trace`.\n\n\
The level applies to the whole process, not to one session, because there is one process \
per session and one standard error to write to. It stays until it is changed again or \
until the process ends; it does not write to the environment, and a new process reads \
`SOLAR_LOG` as it always did.\n\n\
Setting the level it already has is not an error: the call answers with `changed` false. \
A level that is not one of the six is refused with INVALID_VALUE, naming the six.",
            errors: vec![ErrorSpec::of(Reason::InvalidValue)],
            side_effects: vec![SideEffect::None],
            idempotent: true,
            stability: Stability::Experimental,
            since: "0.3.0",
            timeout_ms: 1_000,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            examples: vec![Example::exact(
                "turn_it_off",
                "Asking for silence, whatever it was set to before",
                json!({"level": "off"}),
                json!({"previous": ANY, "current": "off", "changed": ANY}),
            )],
        }
    }

    fn call(_ctx: &Context, params: Params) -> Result<Output, SolarError> {
        let Some(wanted) = Level::parse(&params.level) else {
            return Err(unreadable(&params.level));
        };

        let previous = logging::set_level(wanted);
        Ok(Output {
            previous: previous.as_lower().to_owned(),
            current: wanted.as_lower().to_owned(),
            changed: previous != wanted,
        })
    }
}

/// The error a level nobody can read deserves.
fn unreadable(given: &str) -> SolarError {
    let accepted: Vec<&str> = Level::ALL.iter().map(|level| level.as_lower()).collect();
    SolarError::new(Reason::InvalidValue, format!("{given:?} is not a level.")).with_detail(
        ErrorDetail::new(Status::InvalidArgument)
            .field("/level")
            .expected(format!("one of: {}", accepted.join(", ")))
            .received(given)
            .hint(
                "The spellings are the ones SOLAR_LOG accepts, in any case, and `none` \
                 and `0` also mean off while `warning` also means warn.",
            ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::context;

    /// The level is one number for the whole process, so these tests take their turn.
    static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn set(level: &str) -> Output {
        SolarSetLogLevel::call(
            &context("solar.set_log_level"),
            Params {
                level: level.to_owned(),
            },
        )
        .expect("a level that can be read")
    }

    #[test]
    fn it_reports_what_the_level_was_and_what_it_is() {
        let _turn = ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let started_at = logging::level();

        let answer = set("debug");
        assert_eq!(answer.previous, started_at.as_lower());
        assert_eq!(answer.current, "debug");
        assert_eq!(logging::level(), Level::Debug, "it really changed");

        let answer = set("off");
        assert_eq!(answer.previous, "debug");
        assert_eq!(answer.current, "off");
        assert!(answer.changed);

        logging::set_level(started_at);
    }

    #[test]
    fn setting_the_level_it_already_has_changes_nothing_and_says_so() {
        let _turn = ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let started_at = logging::level();

        set("warn");
        let answer = set("warn");
        assert_eq!(answer.previous, "warn");
        assert_eq!(answer.current, "warn");
        assert!(!answer.changed, "asking twice is not a change");

        logging::set_level(started_at);
    }

    #[test]
    fn every_spelling_solar_log_accepts_is_accepted_here() {
        let _turn = ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let started_at = logging::level();

        for (spelling, expected) in [
            ("off", "off"),
            ("none", "off"),
            ("0", "off"),
            ("ERROR", "error"),
            (" warning ", "warn"),
            ("Info", "info"),
            ("debug", "debug"),
            ("trace", "trace"),
        ] {
            assert_eq!(set(spelling).current, expected, "{spelling}");
        }

        logging::set_level(started_at);
    }

    #[test]
    fn a_level_nobody_can_read_is_refused_and_the_six_are_named() {
        let failure = SolarSetLogLevel::call(
            &context("solar.set_log_level"),
            Params {
                level: "verbose".to_owned(),
            },
        )
        .expect_err("verbose is not a level");

        assert_eq!(failure.reason(), Reason::InvalidValue);
        assert_eq!(failure.status(), Status::InvalidArgument);
        let detail = &failure.details()[0];
        assert_eq!(detail.field.as_deref(), Some("/level"));
        for level in Level::ALL {
            assert!(
                detail
                    .expected
                    .as_ref()
                    .is_some_and(|text| text.contains(level.as_lower())),
                "{} is not named in {:?}",
                level.as_lower(),
                detail.expected
            );
        }
    }
}
