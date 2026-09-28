//! Diagnostics on standard error, off unless `SOLAR_LOG` says otherwise.
//!
//! Standard output belongs to the protocol. Nothing in SOLAR writes a diagnostic there, and
//! this module is the only thing that writes to standard error, so the rule is easy to
//! check: a `println!` outside the command line interface is a bug.

use std::cell::RefCell;
use std::io::Write;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

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

    /// Every level, quietest first, which is the order they compare in.
    pub const ALL: [Level; 6] = [
        Level::Off,
        Level::Error,
        Level::Warn,
        Level::Info,
        Level::Debug,
        Level::Trace,
    ];

    /// The number this level is stored as, so that it fits in an atomic.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Level::Off => 0,
            Level::Error => 1,
            Level::Warn => 2,
            Level::Info => 3,
            Level::Debug => 4,
            Level::Trace => 5,
        }
    }

    /// The level a number stands for. Anything unknown reads as `Off`, which is the
    /// setting that says nothing rather than the one that says everything.
    #[must_use]
    pub const fn from_code(code: u8) -> Level {
        match code {
            1 => Level::Error,
            2 => Level::Warn,
            3 => Level::Info,
            4 => Level::Debug,
            5 => Level::Trace,
            _ => Level::Off,
        }
    }

    /// The spelling `SOLAR_LOG` uses, in lower case, which is what an API reports.
    #[must_use]
    pub const fn as_lower(self) -> &'static str {
        match self {
            Level::Off => "off",
            Level::Error => "error",
            Level::Warn => "warn",
            Level::Info => "info",
            Level::Debug => "debug",
            Level::Trace => "trace",
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

/// How a diagnostic line is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// One line of English, for a person watching a terminal. The default.
    Human,
    /// One JSON object per line, for whatever is collecting them.
    Json,
}

impl Format {
    /// Reads a format from the spelling `SOLAR_LOG_FORMAT` uses, in any case.
    #[must_use]
    pub fn parse(text: &str) -> Option<Format> {
        match text.trim().to_ascii_lowercase().as_str() {
            "human" | "text" => Some(Format::Human),
            "json" => Some(Format::Json),
            _ => None,
        }
    }
}

/// The call a diagnostic line belongs to, so that every line written while serving one
/// can name it.
#[derive(Debug, Clone, Default)]
pub struct Call {
    /// The `id` of the request, as it appeared on the wire.
    pub request_id: Option<serde_json::Value>,
    /// The method being served.
    pub method: Option<String>,
}

thread_local! {
    /// The call this thread is serving, if it is serving one.
    static CURRENT_CALL: RefCell<Call> = RefCell::new(Call::default());
}

/// Runs `task` with every diagnostic line attributed to this call.
///
/// The attribution is per thread and is restored afterwards, so one call cannot leave
/// its identity behind for the next.
pub fn during_call<T>(call: Call, task: impl FnOnce() -> T) -> T {
    let previous = CURRENT_CALL.with(|current| current.replace(call));
    let outcome = task();
    CURRENT_CALL.with(|current| {
        *current.borrow_mut() = previous;
    });
    outcome
}

/// The call this thread is serving.
#[must_use]
pub fn current_call() -> Call {
    CURRENT_CALL.with(|current| current.borrow().clone())
}

static FORMAT: OnceLock<Format> = OnceLock::new();

/// The format this process writes, read once from `SOLAR_LOG_FORMAT`.
///
/// An unreadable value is treated as `human`, because a logging setting is never a
/// reason to refuse to serve a call.
#[must_use]
pub fn log_format() -> Format {
    *FORMAT.get_or_init(|| {
        std::env::var("SOLAR_LOG_FORMAT")
            .ok()
            .and_then(|text| Format::parse(&text))
            .unwrap_or(Format::Human)
    })
}

/// The level this process logs at, as a number, so that it can change while SOLAR runs.
///
/// [`NOT_READ`] means the environment has not been consulted yet. It is read once, on the
/// first call to [`level`], and after that this is simply the level: `solar.set_log_level`
/// writes here, and every line written afterwards is filtered by what it wrote.
static LEVEL: AtomicU8 = AtomicU8::new(NOT_READ);

/// The value of [`LEVEL`] before anything has read `SOLAR_LOG`.
const NOT_READ: u8 = u8::MAX;

/// The level this process logs at, read once from `SOLAR_LOG` and changeable afterwards.
///
/// An unreadable value in the environment is treated as `off`, because a logging setting
/// is never a reason to refuse to serve a call.
#[must_use]
pub fn level() -> Level {
    let stored = LEVEL.load(Ordering::Relaxed);
    if stored != NOT_READ {
        return Level::from_code(stored);
    }

    let from_environment = std::env::var("SOLAR_LOG")
        .ok()
        .and_then(|text| Level::parse(&text))
        .unwrap_or(Level::Off);

    // Two threads reading at once must agree on what the environment said, and the loser
    // takes the winner's answer rather than its own.
    match LEVEL.compare_exchange(
        NOT_READ,
        from_environment.code(),
        Ordering::SeqCst,
        Ordering::SeqCst,
    ) {
        Ok(_) => from_environment,
        Err(already) => Level::from_code(already),
    }
}

/// Sets the level for the rest of this process, and gives back what it was.
///
/// `--log` and `SOLAR_LOG` decide what a session starts at; this is what changes it while
/// the session runs, which is what `solar.set_log_level` does. It touches only where
/// diagnostics go, standard error, and never standard output.
pub fn set_level(level: Level) -> Level {
    // Reading first settles the environment, so that the previous level a caller is told
    // about is the one that was really in force rather than "not read yet".
    let previous = self::level();
    LEVEL.store(level.code(), Ordering::SeqCst);
    previous
}

/// Writes one line to standard error when the level allows it.
pub fn log(level_of_message: Level, message: &str) {
    log_with(level_of_message, message, None);
}

/// The same, with the duration of whatever is being reported.
pub fn log_with(level_of_message: Level, message: &str, duration_us: Option<u64>) {
    if level_of_message == Level::Off || level_of_message > level() {
        return;
    }
    let at = clock::now_rfc3339_micros();
    let call = current_call();

    let line = match log_format() {
        Format::Human => {
            let mut line = format!("solar {} {at}", level_of_message.as_str());
            if let Some(method) = &call.method {
                line.push(' ');
                line.push_str(method);
            }
            if let Some(id) = &call.request_id {
                use std::fmt::Write as _;
                let _ = write!(line, " #{id}");
            }
            line.push(' ');
            line.push_str(message);
            line.push('\n');
            line
        }
        Format::Json => {
            // Every member is always present, `null` where it does not apply, which is
            // the discipline the response envelope follows.
            let object = serde_json::json!({
                "time": at,
                "level": level_of_message.as_str().to_lowercase(),
                "request_id": call.request_id,
                "method": call.method,
                "duration_us": duration_us,
                "message": message,
            });
            format!("{object}\n")
        }
    };

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

/// Logs the traffic itself.
pub fn trace(message: &str) {
    log(Level::Trace, message);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_spelling_of_a_level_is_read() {
        // Every spelling, because each one is a promise to whoever sets SOLAR_LOG, and a
        // missing arm is a level that silently reads as off.
        for (spelling, level) in [
            ("off", Level::Off),
            ("none", Level::Off),
            ("0", Level::Off),
            ("error", Level::Error),
            ("warn", Level::Warn),
            ("warning", Level::Warn),
            ("info", Level::Info),
            ("debug", Level::Debug),
            ("trace", Level::Trace),
        ] {
            assert_eq!(Level::parse(spelling), Some(level), "{spelling}");
            assert_eq!(
                Level::parse(&format!("  {} ", spelling.to_ascii_uppercase())),
                Some(level),
                "{spelling}, in any case and with space around it"
            );
        }

        for nonsense in ["verbose", "", "  ", "1", "offf", "warnings"] {
            assert_eq!(Level::parse(nonsense), None, "{nonsense:?}");
        }
    }

    #[test]
    fn every_spelling_of_a_format_is_read() {
        for (spelling, format) in [
            ("human", Format::Human),
            ("text", Format::Human),
            ("json", Format::Json),
        ] {
            assert_eq!(Format::parse(spelling), Some(format), "{spelling}");
            assert_eq!(
                Format::parse(&format!(" {} ", spelling.to_ascii_uppercase())),
                Some(format),
                "{spelling}, in any case and with space around it"
            );
        }

        for nonsense in ["ndjson", "", "plain", "j son"] {
            assert_eq!(Format::parse(nonsense), None, "{nonsense:?}");
        }
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
