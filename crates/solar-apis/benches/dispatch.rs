//! What one call costs inside the process, with the pipes taken out of the picture.
//!
//! `cargo bench -p solar-apis`. The numbers in the README come from here and from
//! `solar-cli/benches/binary.rs`, which measures the same work with the pipes put back.

// A benchmark is not production code, and criterion's own API panics on misuse anyway.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use solar_core::protocol::parse_request;

const PING: &str = r#"{"jsonrpc":"2.0","id":1,"method":"solar.ping","params":{"message":"hi"}}"#;
const UNKNOWN: &str = r#"{"jsonrpc":"2.0","id":1,"method":"solar.pign"}"#;
const BAD_PARAMS: &str = r#"{"jsonrpc":"2.0","id":1,"method":"solar.ping","params":{"nope":1}}"#;
const BROKEN: &str = "{not json";

fn benchmarks(c: &mut Criterion) {
    let dispatcher = solar_apis::dispatcher();

    let mut group = c.benchmark_group("dispatch");

    // The whole path: parse the line, find the API, read the parameters, run the handler
    // on its worker thread, serialise the answer.
    group.bench_function("ping", |b| {
        b.iter(|| black_box(dispatcher.handle_line(black_box(PING)).to_line()));
    });

    // The same path, stopping at the registry.
    group.bench_function("method_not_found", |b| {
        b.iter(|| black_box(dispatcher.handle_line(black_box(UNKNOWN)).to_line()));
    });

    // The same path, stopping at the parameters, which is the error a caller hits most.
    group.bench_function("unknown_parameter", |b| {
        b.iter(|| black_box(dispatcher.handle_line(black_box(BAD_PARAMS)).to_line()));
    });

    // The cheapest possible failure: nothing to parse.
    group.bench_function("broken_json", |b| {
        b.iter(|| black_box(dispatcher.handle_line(black_box(BROKEN)).to_line()));
    });

    group.finish();

    // The envelope on its own, to say how much of the cost above is the protocol layer.
    c.bench_function("parse_request", |b| {
        b.iter(|| black_box(parse_request(black_box(PING)).is_ok()));
    });
}

criterion_group!(benches, benchmarks);
criterion_main!(benches);
