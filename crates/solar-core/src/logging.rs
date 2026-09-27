//! Diagnostics on standard error, off unless `SOLAR_LOG` says otherwise.
//!
//! Standard output belongs to the protocol. Nothing in SOLAR writes a diagnostic there, and
//! this module is the only thing that writes to standard error, so the rule is easy to
//! check: a `println!` outside the command line interface is a bug.

use std::io::Write;
use std::sync::OnceLock;

use crate::clock;

/// How much SOLAR says about what it is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Say nothing. The default.
    Off,
    /// Only failures that a person should see.
    Error,
    /// Failures and things that look wrong.
    Warn,
    /// A line per call.
    Info,
    /// What dispatch decided and why.
    Debug,
    /// Everything, including the lines that come in and go out.
    Trace,
}

impl Level {
    /// Reads a level from the spelling `SOLAR_LOG` uses, in any case.
    #[must_use]
    pub fn parse(text: &str) -> Option<Level> {
        match text.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "0" => Some(Level::Off),
            "error" => Some(Level::Error),
            "warn" | "warning" => Some(Level::Warn),
            "info" => Some(Level::Info),
            "debug" => Some(Level::Debug),
            "trace" => Some(Level::Trace),
            _ => None,
        }
    }

    /// The spelling that appears in a log line.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Level::Off => "OFF",
            Level::Error => "ERROR",
            Level::Warn => "WARN",
            Level::Info => "INFO",
            Level::Debug => "DEBUG",
            Level::Trace => "TRACE",
        }
    }
}

static LEVEL: OnceLock<Level> = OnceLock::new();

/// The level this process logs at, read once from `SOLAR_LOG`.
///
/// An unreadable value is treated as `off`, because a logging setting is never a reason to
/// refuse to serve a call.
#[must_use]
pub fn level() -> Level {
    *LEVEL.get_or_init(|| {
        std::env::var("SOLAR_LOG")
            .ok()
            .and_then(|text| Level::parse(&text))
            .unwrap_or(Level::Off)
    })
}

/// Forces the level, ignoring `SOLAR_LOG`. Only the first call, wherever it comes from,
/// has any effect.
pub fn set_level(level: Level) {
    let _ = LEVEL.set(level);
}

/// Writes one line to standard error when the level allows it.
pub fn log(level_of_message: Level, message: &str) {
    if level_of_message == Level::Off || level_of_message > level() {
        return;
    }
    let line = format!(
        "solar {} {} {message}\n",
        level_of_message.as_str(),
        clock::now_rfc3339_micros()
    );
    let mut stderr = std::io::stderr().lock();
    let _ = stderr.write_all(line.as_bytes());
    let _ = stderr.flush();
}

/// Logs a failure.
pub fn error(message: &str) {
    log(Level::Error, message);
}

/// Logs something that looks wrong.
pub fn warn(message: &str) {
    log(Level::Warn, message);
}

/// Logs one line per call.
pub fn info(message: &str) {
    log(Level::Info, message);
}

/// Logs what dispatch decided.
pub fn debug(message: &str) {
    log(Level::Debug, message);
}

/// Logs the traffic itself.
pub fn trace(message: &str) {
    log(Level::Trace, message);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_spelling_of_a_level_is_read() {
        assert_eq!(Level::parse("off"), Some(Level::Off));
        assert_eq!(Level::parse("  TRACE "), Some(Level::Trace));
        assert_eq!(Level::parse("Warning"), Some(Level::Warn));
        assert_eq!(Level::parse("verbose"), None);
        assert_eq!(Level::parse(""), None);
    }

    #[test]
    fn the_levels_are_ordered_from_quiet_to_loud() {
        assert!(Level::Off < Level::Error);
        assert!(Level::Error < Level::Warn);
        assert!(Level::Warn < Level::Info);
        assert!(Level::Info < Level::Debug);
        assert!(Level::Debug < Level::Trace);
    }
}
