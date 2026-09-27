//! What a call costs from outside the process: a whole run, and one round trip on a pipe.
//!
//! `cargo bench -p solar-cli`. These are the two numbers a caller actually feels: how long
//! `solar call` takes from nothing, and how long a request waits in a session that is
//! already up.

// A benchmark is not production code, and a benchmark that cannot set itself up has
// nothing to measure, so it panics.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::hint::black_box;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use criterion::{Criterion, criterion_group, criterion_main};

const SOLAR: &str = env!("CARGO_BIN_EXE_solar");
const PING: &str = r#"{"jsonrpc":"2.0","id":1,"method":"solar.ping","params":{"message":"hi"}}"#;

fn benchmarks(c: &mut Criterion) {
    // A whole run: create the process, build the registry, answer, exit.
    c.bench_function("startup/call_ping", |b| {
        b.iter(|| {
            let output = Command::new(SOLAR)
                .args(["call", "solar.ping", "{\"message\":\"hi\"}"])
                .output()
                .expect("the binary must run");
            assert!(output.status.success());
            black_box(output.stdout.len())
        });
    });

    // One round trip in a session that is already running: a line down a pipe and a line
    // back up another one.
    let mut child = Command::new(SOLAR)
        .args(["serve", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("the binary must run");
    let mut input = child.stdin.take().expect("stdin was piped");
    let mut output = BufReader::new(child.stdout.take().expect("stdout was piped"));

    c.bench_function("session/roundtrip_ping", |b| {
        b.iter(|| {
            writeln!(input, "{PING}").expect("the session must accept the request");
            input.flush().expect("the request must reach the session");
            let mut line = String::new();
            output
                .read_line(&mut line)
                .expect("the session must answer");
            black_box(line.len())
        });
    });

    drop(input);
    let _ = child.wait();
}

criterion_group!(benches, benchmarks);
criterion_main!(benches);
