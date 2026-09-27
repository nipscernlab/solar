//! `cargo xtask load`: what a session really costs, under load, measured rather than hoped.
//!
//! Every figure the repository had before this was one call at a time on an idle machine,
//! which says what a round trip costs and nothing about what a session costs. This drives a
//! real `solar serve --stdio` through a real pipe, one request at a time, waiting for each
//! response before sending the next, which is what a client does.
//!
//! What is measured:
//!
//! - **throughput**, requests answered per second, over the whole run;
//! - **latency**, the round trip of one request, at the median, the 99th percentile and the
//!   worst case, in microseconds;
//! - **memory**, the resident set of the `solar` process at the start, at its peak and at
//!   the end, which is what says whether a session grows with the calls it answers.
//!
//! The driver's own cost is in every figure: this is the latency a client sees, not the
//! time dispatch spends. That is the honest number, and it is the one a caller can check.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// How many requests a run sends when nothing else is asked for.
const DEFAULT_REQUESTS: usize = 10_000;

/// How often the resident set is sampled, in requests.
///
/// Reading the resident set of another process costs a process of its own on Windows and
/// on macOS, which is milliseconds, thousands of times what a call costs. The sampling is
/// therefore rare, and the time it takes is measured and taken out of the wall clock, so
/// that the throughput reported is SOLAR's and not the sampler's.
const SAMPLE_EVERY: usize = 2_000;

/// How many requests a soak run sends: enough that a leak of a few bytes per call shows.
const SOAK_REQUESTS: usize = 1_000_000;

/// One run, and what came out of it.
struct Measured {
    requests: usize,
    failures: usize,
    elapsed: Duration,
    /// The round trip of every request, in microseconds, in the order they were sent.
    latencies: Vec<u64>,
    /// The resident set of the server, in kibibytes, sampled through the run.
    memory: Vec<u64>,
    /// What the sampling itself cost, which is not part of `elapsed`.
    sampling: Duration,
}

impl Measured {
    /// The value at a percentile, with the percentile given as a fraction of a hundred.
    fn percentile(sorted: &[u64], percent: f64) -> u64 {
        if sorted.is_empty() {
            return 0;
        }
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "an index into a list of at most a few million samples, where the \
                      rounding of the last digit does not change the figure reported"
        )]
        let index = ((sorted.len() - 1) as f64 * percent / 100.0).round() as usize;
        sorted.get(index).copied().unwrap_or_default()
    }

    /// Prints the figures, in the shape they are recorded in `STATUS.md`.
    fn report(&self, what: &str) {
        let mut sorted = self.latencies.clone();
        sorted.sort_unstable();

        #[allow(
            clippy::cast_precision_loss,
            reason = "a request count and a duration, both far below the precision of f64"
        )]
        let throughput = self.requests as f64 / self.elapsed.as_secs_f64();

        println!("load: {what}");
        println!("  requests         {}", self.requests);
        println!("  errors answered  {}", self.failures);
        println!("  wall clock       {:.3} s", self.elapsed.as_secs_f64());
        println!(
            "  sampling         {:.3} s, taken out of the wall clock above",
            self.sampling.as_secs_f64()
        );
        println!("  throughput       {throughput:.0} requests per second");
        println!("  latency p50      {} us", Self::percentile(&sorted, 50.0));
        println!("  latency p90      {} us", Self::percentile(&sorted, 90.0));
        println!("  latency p99      {} us", Self::percentile(&sorted, 99.0));
        println!(
            "  latency max      {} us",
            sorted.last().copied().unwrap_or_default()
        );

        if let (Some(first), Some(last)) = (self.memory.first(), self.memory.last()) {
            let peak = self.memory.iter().max().copied().unwrap_or_default();
            println!("  memory at start  {first} KiB");
            println!("  memory at peak   {peak} KiB");
            println!("  memory at end    {last} KiB");
            let growth = i64::try_from(*last).unwrap_or(i64::MAX)
                - i64::try_from(*first).unwrap_or(i64::MAX);
            println!("  memory growth    {growth} KiB over the run");
        }
    }
}

/// Runs the load measurement.
///
/// # Errors
///
/// Returns why the server could not be started, driven or measured.
pub(crate) fn run(root: &Path, arguments: &[&str]) -> Result<(), String> {
    let soak = arguments.contains(&"--soak");
    let requests = number_after(arguments, "--requests").unwrap_or(if soak {
        SOAK_REQUESTS
    } else {
        DEFAULT_REQUESTS
    });
    let mixed = arguments.contains(&"--mixed");
    let both = !soak && !mixed && !arguments.contains(&"--only-success");

    let binary = release_binary(root)?;

    let plain = drive(&binary, requests, false)?;
    plain.report(&format!("{requests} successful calls, one at a time"));

    if mixed || both {
        println!();
        let mixed_run = drive(&binary, requests, true)?;
        mixed_run.report(&format!(
            "{requests} calls, every other one an error, one at a time"
        ));
    }

    Ok(())
}

/// The release binary, which is what a measurement has to be made against.
fn release_binary(root: &Path) -> Result<PathBuf, String> {
    let name = if cfg!(windows) { "solar.exe" } else { "solar" };
    let path = root.join("target").join("release").join(name);
    if path.exists() {
        return Ok(path);
    }
    Err(format!(
        "{} is not there. Run `cargo build --release --locked -p solar-cli` first: a \
         measurement of a debug build measures nothing.",
        path.display()
    ))
}

/// Sends `requests` requests through one session, waiting for each response.
fn drive(binary: &Path, requests: usize, mixed: bool) -> Result<Measured, String> {
    let mut child = Command::new(binary)
        .arg("serve")
        .arg("--stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|failure| format!("{} could not be started: {failure}", binary.display()))?;

    let mut input = child
        .stdin
        .take()
        .ok_or_else(|| "the server has no standard input".to_owned())?;
    let mut output = BufReader::new(
        child
            .stdout
            .take()
            .ok_or_else(|| "the server has no standard output".to_owned())?,
    );

    // Twenty samples or so, however long the run: reading another process's resident set
    // costs a process of its own, and a sample every few hundred calls would measure the
    // sampler.
    let sample_every = SAMPLE_EVERY.max(requests / 20).max(1);
    let mut latencies = Vec::with_capacity(requests);
    let mut memory = Vec::new();
    let mut failures = 0usize;
    let mut line = String::new();

    // The first sample is taken before any call, so that growth is growth and not startup.
    let mut sampling = Duration::ZERO;
    let sampled = Instant::now();
    if let Some(resident) = resident_kib(&child) {
        memory.push(resident);
    }
    sampling += sampled.elapsed();

    // Only the sampling that happens between the first and the last request is inside the
    // wall clock, so only that is taken out of it.
    let mut inside = Duration::ZERO;
    let started = Instant::now();
    for index in 0..requests {
        let request = if mixed && index % 2 == 1 {
            // A method that does not exist, which is the cheapest error there is, and the
            // one every client meets first.
            format!("{{\"jsonrpc\":\"2.0\",\"id\":{index},\"method\":\"solar.nope\"}}\n")
        } else {
            format!(
                "{{\"jsonrpc\":\"2.0\",\"id\":{index},\"method\":\"solar.ping\",\"params\":{{\"message\":\"load\"}}}}\n"
            )
        };

        let sent = Instant::now();
        input
            .write_all(request.as_bytes())
            .map_err(|failure| format!("the request could not be written: {failure}"))?;
        input
            .flush()
            .map_err(|failure| format!("the request could not be flushed: {failure}"))?;

        line.clear();
        let read = output
            .read_line(&mut line)
            .map_err(|failure| format!("the response could not be read: {failure}"))?;
        if read == 0 {
            return Err(format!("the session ended after {index} requests"));
        }
        latencies.push(u64::try_from(sent.elapsed().as_micros()).unwrap_or(u64::MAX));

        if line.contains("\"error\"") {
            failures += 1;
        }

        if index % sample_every == 0 {
            let sampled = Instant::now();
            if let Some(resident) = resident_kib(&child) {
                memory.push(resident);
            }
            inside += sampled.elapsed();
        }
    }
    let elapsed = started.elapsed().saturating_sub(inside);
    sampling += inside;

    let sampled = Instant::now();
    if let Some(resident) = resident_kib(&child) {
        memory.push(resident);
    }
    sampling += sampled.elapsed();

    drop(input);
    let _ = child.wait();

    Ok(Measured {
        requests,
        failures,
        elapsed,
        latencies,
        memory,
        sampling,
    })
}

/// The resident set of the running server, in kibibytes, or `None` where it cannot be read.
///
/// Each system keeps this in its own place and none of them is portable, so each is read
/// where it is and a system that is not one of the three simply reports nothing rather than
/// reporting a number that means something else.
#[cfg(target_os = "linux")]
fn resident_kib(child: &Child) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{}/status", child.id())).ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|value| value.split_whitespace().next()?.parse().ok())
}

#[cfg(not(target_os = "linux"))]
fn resident_kib(child: &Child) -> Option<u64> {
    let id = child.id().to_string();
    let output = if cfg!(windows) {
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!("(Get-Process -Id {id}).WorkingSet64"),
            ])
            .output()
            .ok()?
    } else {
        Command::new("ps")
            .args(["-o", "rss=", "-p", &id])
            .output()
            .ok()?
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let value: u64 = text.trim().parse().ok()?;
    // Windows reports bytes; `ps` reports kibibytes already.
    Some(if cfg!(windows) { value / 1024 } else { value })
}

/// The number written after a flag, when it is there and is a number.
fn number_after(arguments: &[&str], flag: &str) -> Option<usize> {
    let at = arguments.iter().position(|argument| *argument == flag)?;
    arguments.get(at + 1)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_percentile_of_a_known_list_is_the_value_at_that_place() {
        // Nearest rank over the whole list: the percentile picks the sample at that
        // fraction of the distance from the first to the last. An odd list makes the
        // median unambiguous, which is the point of choosing 101 here.
        let sorted: Vec<u64> = (1..=101).collect();
        assert_eq!(Measured::percentile(&sorted, 0.0), 1, "the fastest call");
        assert_eq!(Measured::percentile(&sorted, 50.0), 51, "the true median");
        assert_eq!(Measured::percentile(&sorted, 99.0), 100);
        assert_eq!(
            Measured::percentile(&sorted, 100.0),
            101,
            "the slowest call"
        );
    }

    #[test]
    fn a_percentile_of_one_sample_is_that_sample() {
        assert_eq!(Measured::percentile(&[7], 50.0), 7);
        assert_eq!(Measured::percentile(&[7], 99.0), 7);
    }

    #[test]
    fn a_percentile_of_nothing_is_nothing_rather_than_a_panic() {
        assert_eq!(Measured::percentile(&[], 99.0), 0);
    }

    #[test]
    fn a_flag_is_read_only_when_a_number_follows_it() {
        assert_eq!(number_after(&["--requests", "25"], "--requests"), Some(25));
        assert_eq!(number_after(&["--requests"], "--requests"), None);
        assert_eq!(number_after(&["--requests", "lots"], "--requests"), None);
        assert_eq!(number_after(&["--mixed"], "--requests"), None);
    }
}
