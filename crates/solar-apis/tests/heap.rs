//! What one call allocates, measured with `dhat` rather than guessed.
//!
//! Latency says how long a call takes and memory over a session says whether it leaks.
//! Neither says how much work a call does on the heap, which is what shows first when a
//! change makes SOLAR slower for a reason nobody meant. `dhat` counts every allocation
//! this binary makes, so the number here is the real one, including what `serde_json`
//! does inside.
//!
//! The ceiling is measured and then written down, exactly like the coverage floor and the
//! mutation ceiling: it is what was measured, rounded up to a round number, and a change
//! that goes past it fails this test and has to say why in its pull request.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test reports failure by panicking; integration tests are their own crate"
)]
#![allow(
    clippy::print_stdout,
    reason = "the measurement is the point of this test, so it is printed under               --nocapture; the channel rule protects the protocol, which a test never               speaks"
)]

/// Every allocation of this test binary goes through the profiler.
#[global_allocator]
static ALLOCATOR: dhat::Alloc = dhat::Alloc;

/// How many calls are measured. Enough that the profiler's own noise is a rounding error.
const CALLS: u64 = 10_000;

/// The most one `solar.ping` may allocate, in blocks.
///
/// Measured on 27 September 2026, in the test profile on Windows 11: **51 blocks** per
/// call. The ceiling is that measurement with room for the noise of another machine.
const BLOCKS_PER_CALL: u64 = 60;

/// The most one `solar.ping` may allocate, in bytes.
///
/// Measured on 27 September 2026, in the test profile on Windows 11: **5 278 bytes** per
/// call. The ceiling is that measurement with room for the noise of another machine.
///
/// This is the test profile, which is what this test can measure: `dhat` replaces the
/// global allocator of the binary it runs in, and the binary it runs in is a test. A
/// release build allocates the same number of times for the same reasons; what differs is
/// what the optimiser inlines, not what `serde_json` asks the allocator for.
const BYTES_PER_CALL: u64 = 6_000;

#[test]
fn one_ping_allocates_no_more_than_it_is_allowed_to() {
    let profiler = dhat::Profiler::builder().testing().build();
    let dispatcher = solar_apis::dispatcher();

    // One call first, so that whatever is built once is not counted per call: the worker
    // thread, the lazily built parts of the registry, the allocator's own arenas.
    let line = r#"{"jsonrpc":"2.0","id":1,"method":"solar.ping","params":{"message":"heap"}}"#;
    let warm = dispatcher.answer_line(line);
    assert!(warm.line.contains("\"pong\":true"), "{}", warm.line);

    let before = dhat::HeapStats::get();
    for _ in 0..CALLS {
        let answered = dispatcher.answer_line(line);
        // Used, so that nothing here can be optimised away.
        assert_eq!(answered.calls, 1);
    }
    let after = dhat::HeapStats::get();

    let blocks = (after.total_blocks - before.total_blocks) / CALLS;
    let bytes = (after.total_bytes - before.total_bytes) / CALLS;

    println!("solar.ping allocates {blocks} blocks and {bytes} bytes per call");

    assert!(
        blocks <= BLOCKS_PER_CALL,
        "solar.ping now allocates {blocks} blocks per call, and {BLOCKS_PER_CALL} is the \
         ceiling. Say in the pull request what the call does now that it did not do \
         before, and raise the ceiling on purpose or not at all."
    );
    assert!(
        bytes <= BYTES_PER_CALL,
        "solar.ping now allocates {bytes} bytes per call, and {BYTES_PER_CALL} is the \
         ceiling. Say in the pull request what the call does now that it did not do \
         before, and raise the ceiling on purpose or not at all."
    );

    // In testing mode the counts stay in the process, which is what this test reads. For
    // the full profile, and the viewer at https://nnethercote.github.io/dh_view, build
    // the profiler without `.testing()`: it then writes `dhat-heap.json` beside the run.
    drop(profiler);
}
