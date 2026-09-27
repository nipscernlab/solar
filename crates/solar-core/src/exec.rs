//! Running an external program and reporting the run in full.
//!
//! Section 9 of the contract says what a caller gets back: the exact command, the exit
//! code, the duration, and both streams, truncated with their original size reported. This
//! module is the only place in SOLAR that starts a process.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;

use crate::error::{ErrorDetail, SolarError};
use crate::reason::Reason;
use crate::text::truncate;

/// How much of each stream is kept, in bytes.
pub const MAX_CAPTURE_BYTES: usize = 65_536;

/// How long to wait between two checks on a running child, at the longest.
const MAX_POLL_INTERVAL: Duration = Duration::from_millis(8);

/// Everything one run of an external program produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CommandOutcome {
    /// The exact argument vector, program first, as the operating system received it.
    pub command: Vec<String>,
    /// The exit status, `null` when the process was killed by a signal or by the timeout.
    pub exit_code: Option<i32>,
    /// Wall clock milliseconds from spawn to reap.
    pub duration_ms: u64,
    /// Whether the program was killed because it ran past its budget.
    pub timed_out: bool,
    /// Captured standard output, truncated to [`MAX_CAPTURE_BYTES`].
    pub stdout: String,
    /// Captured standard error, truncated to [`MAX_CAPTURE_BYTES`].
    pub stderr: String,
    /// Whether either stream was cut.
    pub truncated: bool,
    /// The size of standard output before truncation, in bytes.
    pub stdout_bytes: u64,
    /// The size of standard error before truncation, in bytes.
    pub stderr_bytes: u64,
}

impl CommandOutcome {
    /// Whether the program finished on its own with the status zero.
    #[must_use]
    pub fn succeeded(&self) -> bool {
        self.exit_code == Some(0)
    }

    /// The first non empty line of the output, standard output first.
    ///
    /// Programs disagree about which stream a version belongs on, so this looks at both.
    #[must_use]
    pub fn first_line(&self) -> Option<&str> {
        self.stdout
            .lines()
            .chain(self.stderr.lines())
            .map(str::trim)
            .find(|line| !line.is_empty())
    }
}

/// Runs a program and waits for it, at most for `budget`.
///
/// The child is killed when the budget runs out, and what it printed until then is still
/// reported. Standard input is closed, so a program that waits for input fails fast
/// instead of hanging until the budget is gone.
///
/// # Errors
///
/// Returns `PERMISSION_DENIED` / `ACCESS_DENIED` when the operating system refuses to
/// execute the program, and `UNAVAILABLE` / `SPAWN_FAILED` for any other refusal to start
/// it. A program that starts and then fails is not an error here: its exit code is
/// reported and the caller decides.
pub fn run(
    program: &Path,
    args: &[String],
    budget: Duration,
) -> Result<CommandOutcome, SolarError> {
    let mut command_line = Vec::with_capacity(args.len() + 1);
    command_line.push(program.display().to_string());
    command_line.extend(args.iter().cloned());

    let started = Instant::now();
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|failure| spawn_error(&command_line, &failure))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let out_reader = std::thread::spawn(move || drain(stdout));
    let err_reader = std::thread::spawn(move || drain(stderr));

    let mut timed_out = false;
    let mut interval = Duration::from_micros(200);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(_) => break None,
        }
        if started.elapsed() >= budget {
            timed_out = true;
            let _ = child.kill();
            break child.wait().ok();
        }
        std::thread::sleep(interval.min(budget.saturating_sub(started.elapsed())));
        interval = (interval * 2).min(MAX_POLL_INTERVAL);
    };

    let (out_kept, out_total) = out_reader.join().unwrap_or_default();
    let (err_kept, err_total) = err_reader.join().unwrap_or_default();
    let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

    let (stdout_text, out_cut) = decode(&out_kept);
    let (stderr_text, err_cut) = decode(&err_kept);

    Ok(CommandOutcome {
        command: command_line,
        exit_code: status.and_then(|status| status.code()),
        duration_ms,
        timed_out,
        stdout: stdout_text,
        stderr: stderr_text,
        truncated: out_cut
            || err_cut
            || out_total > out_kept.len() as u64
            || err_total > err_kept.len() as u64,
        stdout_bytes: out_total,
        stderr_bytes: err_total,
    })
}

/// Reads a pipe to its end, keeping only the first [`MAX_CAPTURE_BYTES`] bytes.
fn drain<R: Read>(reader: Option<R>) -> (Vec<u8>, u64) {
    let Some(mut reader) = reader else {
        return (Vec::new(), 0);
    };
    let mut kept: Vec<u8> = Vec::new();
    let mut total: u64 = 0;
    let mut buffer = [0u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                total += read as u64;
                if kept.len() < MAX_CAPTURE_BYTES {
                    let room = MAX_CAPTURE_BYTES - kept.len();
                    kept.extend_from_slice(buffer.get(..read.min(room)).unwrap_or_default());
                }
            }
            Err(_) => break,
        }
    }
    (kept, total)
}

/// Decodes captured bytes as UTF-8, replacing what is not, and cuts on a boundary.
fn decode(bytes: &[u8]) -> (String, bool) {
    let text = String::from_utf8_lossy(bytes);
    truncate(&text, MAX_CAPTURE_BYTES)
}

/// The error a refusal to start a program becomes.
fn spawn_error(command: &[String], failure: &std::io::Error) -> SolarError {
    let program = command.first().map_or("the program", String::as_str);
    let reason = match failure.kind() {
        std::io::ErrorKind::PermissionDenied => Reason::AccessDenied,
        _ => Reason::SpawnFailed,
    };
    SolarError::new(
        reason,
        format!("{program} could not be started: {failure}."),
    )
    .with_detail(
        ErrorDetail::new(reason.status())
            .field("command")
            .expected("a program that can be executed")
            .received(Value::Array(
                command.iter().cloned().map(Value::String).collect(),
            ))
            .hint(match reason {
                Reason::AccessDenied => {
                    "The file is there and this user may not run it. Check its permissions."
                }
                _ => {
                    "The file is there and the operating system refused to start it: a \
                         broken link, the wrong architecture, or a missing shared library."
                }
            }),
    )
}

/// Looks for a program on the `PATH`, the way the operating system would.
///
/// A name that already contains a separator is taken as a path and checked as it is.
/// Otherwise every entry of `PATH` is tried in order; on Windows every extension of
/// `PATHEXT` is tried for each entry, which is what makes `iverilog` find `iverilog.exe`.
///
/// The current directory is never searched, even on Windows where the process loader would:
/// detection must not depend on where SOLAR happens to be running from.
#[must_use]
pub fn find_on_path(program: &str) -> Option<PathBuf> {
    if program.is_empty() {
        return None;
    }
    if program.contains('/') || program.contains('\\') {
        let candidate = PathBuf::from(program);
        return is_executable(&candidate).then_some(candidate);
    }

    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        if directory.as_os_str().is_empty() {
            continue;
        }
        for name in candidate_names(program) {
            let candidate = directory.join(&name);
            if is_executable(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

/// The file names one program name can have on this system.
#[cfg(windows)]
fn candidate_names(program: &str) -> Vec<String> {
    let extensions = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_owned());
    let mut names = Vec::new();
    let already_has_extension = Path::new(program).extension().is_some_and(|found| {
        extensions.split(';').any(|known| {
            known
                .trim_start_matches('.')
                .eq_ignore_ascii_case(&found.to_string_lossy())
        })
    });
    if already_has_extension {
        names.push(program.to_owned());
    }
    for extension in extensions
        .split(';')
        .filter(|entry| !entry.trim().is_empty())
    {
        names.push(format!("{program}{}", extension.trim()));
    }
    names
}

/// The file names one program name can have on this system.
#[cfg(not(windows))]
fn candidate_names(program: &str) -> Vec<String> {
    vec![program.to_owned()]
}

/// Whether a path is a file this user can execute.
#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|data| data.is_file() && data.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Whether a path is a file this user can execute.
#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// A shell that exists on every system the workspace supports.
    fn shell(script: &str) -> (PathBuf, Vec<String>) {
        if cfg!(windows) {
            (
                PathBuf::from("cmd"),
                vec!["/C".to_owned(), script.to_owned()],
            )
        } else {
            (
                PathBuf::from("sh"),
                vec!["-c".to_owned(), script.to_owned()],
            )
        }
    }

    #[test]
    fn a_program_that_prints_is_reported_in_full() {
        let (program, args) = shell("echo hello");
        let outcome = run(&program, &args, Duration::from_secs(20)).unwrap();
        assert!(outcome.succeeded(), "{outcome:?}");
        assert_eq!(outcome.first_line(), Some("hello"));
        assert!(!outcome.truncated);
        assert!(!outcome.timed_out);
        assert_eq!(outcome.command.len(), 3);
        assert!(outcome.stdout_bytes >= 5);
    }

    #[test]
    fn a_non_zero_exit_is_reported_and_is_not_an_error_of_the_call() {
        let (program, args) = shell("exit 3");
        let outcome = run(&program, &args, Duration::from_secs(20)).unwrap();
        assert_eq!(outcome.exit_code, Some(3));
        assert!(!outcome.succeeded());
    }

    #[test]
    fn a_program_that_never_ends_is_killed_when_its_budget_is_gone() {
        let (program, args) = if cfg!(windows) {
            (
                PathBuf::from("ping"),
                vec!["-n".to_owned(), "20".to_owned(), "127.0.0.1".to_owned()],
            )
        } else {
            (
                PathBuf::from("sh"),
                vec!["-c".to_owned(), "sleep 20".to_owned()],
            )
        };
        let started = Instant::now();
        let outcome = run(&program, &args, Duration::from_millis(300)).unwrap();
        assert!(outcome.timed_out, "{outcome:?}");
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the budget was not enforced: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_program_that_does_not_exist_cannot_be_started() {
        let error = run(
            Path::new("solar-no-such-program-xyz"),
            &[],
            Duration::from_secs(5),
        )
        .unwrap_err();
        assert!(matches!(
            error.reason(),
            Reason::SpawnFailed | Reason::AccessDenied
        ));
        assert!(!error.details().is_empty());
    }

    #[test]
    fn a_stream_longer_than_the_limit_is_cut_and_its_real_size_reported() {
        let long = vec![b'x'; MAX_CAPTURE_BYTES * 2];
        let (kept, total) = drain(Some(Cursor::new(long)));
        assert_eq!(kept.len(), MAX_CAPTURE_BYTES);
        assert_eq!(total, (MAX_CAPTURE_BYTES * 2) as u64);
    }

    #[test]
    fn an_absent_stream_reads_as_empty() {
        let (kept, total) = drain(None::<Cursor<Vec<u8>>>);
        assert!(kept.is_empty());
        assert_eq!(total, 0);
    }

    #[test]
    fn bytes_that_are_not_utf8_are_replaced_rather_than_refused() {
        let (text, cut) = decode(&[b'o', b'k', 0xff]);
        assert!(text.starts_with("ok"));
        assert!(!cut);
    }

    #[test]
    fn the_shell_of_this_system_is_found_on_the_path() {
        let name = if cfg!(windows) { "cmd" } else { "sh" };
        let found = find_on_path(name).expect("the shell must be on the PATH");
        assert!(found.is_file(), "{found:?}");
        if cfg!(windows) {
            let text = found.to_string_lossy().to_lowercase();
            assert!(
                text.ends_with(".exe"),
                "{text} should have gained its extension"
            );
        }
    }

    #[test]
    fn a_name_nothing_answers_to_is_not_found() {
        assert!(find_on_path("solar-no-such-program-xyz").is_none());
        assert!(find_on_path("").is_none());
    }
}
