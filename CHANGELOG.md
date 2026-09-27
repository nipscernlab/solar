# Changelog

Every notable change to SOLAR, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the versions follow
[semantic versioning](https://semver.org/spec/v2.0.0.html).

The protocol has a version of its own, `solar/1`, which is not this one. A change to the
protocol is stated here in its own line.

## [Unreleased]

### Added

- **A documented, versioned recording format**, the third of the three things ZENITH asked
  for. A recording now begins with a header naming `solar_recording`, its format version,
  and [`docs/RECORDING.md`](docs/RECORDING.md) is the normative specification, with a JSON
  Schema for one line that a test compiles out of the document and validates real
  recordings against. `solar replay` refuses a major version it does not know, saying
  which version the file declares, which one it reads and what to do; a file with no
  header, written before the format was versioned, is read as 1.0.0 and says so. A
  recording written by hand in that shape replays, which is what makes the document worth
  following. Contract section 12.
- **`capabilities` in the manifest**, at its root, which is the first of the three things
  ZENITH asked SOLAR for: whether batches are accepted and how many elements one may hold,
  whether cancellation exists and which method performs it, whether notifications are
  accepted, and every limit a caller has to respect. Every number is the constant the code
  enforces, compared with it by a test, so a limit cannot move without the manifest moving.
  `schema_version` went to **2.1.0** and `solar.manifest` to **1.2.0**, both minor: a
  consumer written against 2.0.0 reads this document by ignoring what it does not know.
  Contract section 8.2.
- **`solar.set_log_level`**, the second of the three: it takes a level, answers with the
  previous one, the current one and whether anything changed, and moves nothing but where
  the diagnostics go. `SOLAR_LOG` and `--log` still decide what a session starts at, so an
  interface that wants to show more no longer has to restart SOLAR and lose the session.
  The level is now an atomic read once from the environment rather than a value set once,
  which is what makes it changeable. Contract section 2.
- Twenty-seven decision records, 12 to 38, for everything that had been decided alone and
  written in `docs/OPEN_QUESTIONS.md` while the first three stages were built. The
  architect confirmed all of them on 27 September 2026, each as it was proposed. Record 18
  is new rather than moved: it is the part of the superseded entry for a strictly
  sequential session that is still a decision, which is that calls run one at a time in
  the order they arrived. `docs/OPEN_QUESTIONS.md` now holds nothing open.

### Changed

- `cargo xtask compat` reads the **major** of `schema_version` rather than the whole
  string: record 22 says a consumer refuses a manifest whose major differs and reads one
  whose minor moved, so a minor bump is additive. It had classified every change of that
  version as breaking, which would have made adding a member to the document impossible.
- `actions/checkout` to 7.0.1 and `actions/upload-artifact` to 7.0.1, both pinned by
  commit SHA with the version in a comment. Neither release changes anything this
  repository relies on: the workflows use no `pull_request_target` or `workflow_run`, and
  no upload sets `archive: false`.

## [0.2.0] - 2026-09-27

Batches, cancellation, and memory that is bounded by a number rather than by hope. Every
change in this release is additive: a client written against 0.1.0 sees no difference
unless it asks for one, the protocol stays `solar/1`, and no API changed shape.

### Added

- The `solar/1` protocol: JSON-RPC 2.0 over standard input and output, one message per
  line, with one deliberate deviation: notifications are refused, because every call gets
  a response.
- Batches, section 6 of JSON-RPC 2.0, and stricter than it: a line holding an array of
  requests answers with one line holding the array of responses **in the order of the
  requests**, each with its own `meta`, up to 64 elements. An empty array, a larger one,
  and two elements carrying the same `id` are refused as a whole with `BATCH_EMPTY`,
  `BATCH_TOO_LARGE` and `DUPLICATE_ID`. A client that sends no batch sees no difference.
  Record [0010](docs/adr/0010-batches-answer-in-order.md).
- `solar.cancel`, with `{id}`, which asks the call carrying that `id` to stop and reports
  `cancelled_while_queued`, `cancellation_requested`, `already_finished` or `unknown`. A
  session now reads its input on a thread of its own, so a cancellation is answered while
  a call is running; calls still run one at a time, in order, and **every request gets
  exactly one response**, its result or `CANCELLED`. A handler reads its token through
  `Context::is_cancelled`. Record
  [0011](docs/adr/0011-cancelling-is-an-ordinary-call.md).
- Four declared limits, so that memory is bounded and not only hoped to be: the input
  queue holds at most 256 requests and 64 MiB and refuses more with `QUEUE_FULL` while
  still answering `solar.cancel`; a session remembers the 1024 most recent answered
  identifiers; at most 64 abandoned handlers may be alive, after which a call is refused
  with `TOO_MANY_ABANDONED`, and `system.info` 1.2.0 reports the count as
  `abandoned_workers`; and every API declares `max_output_bytes`, 8 MiB by default, above
  which dispatch refuses the response with `OUTPUT_TOO_LARGE`.
- An `id` that belongs to an unanswered call is refused for a new request with
  `ID_IN_FLIGHT`, since cancellation targets a call by `id`.
- `cargo xtask load`, which drives a real session through 10 000 requests, and
  `--soak`, which sends a million, reporting throughput, the latency percentiles and the
  resident memory of the server. The figures are in the README.
- `crates/solar-apis/tests/heap.rs`, which counts what one `solar.ping` allocates with
  `dhat` and holds a ceiling, so a change that allocates more has to say why.
- A `solar.cancel` inside a batch waits its turn like any other element, which section
  9.1 now says in as many words and a test holds: only one sent on its own is answered
  where it is read.
- The rule for choosing between `/etc/os-release` and `/usr/lib/os-release` is no longer
  compiled only on Linux: the paths still are, the rule is not, and five tests run it on
  every system. It was judged by no test on Windows or on a Mac before.
- Tests for the limits themselves: a line of exactly 16 MiB is read and one byte more is
  not, on both paths through the reader, and an oversized line reports the length it
  really was. The queue bounds and the window of remembered identifiers are compared with
  the numbers the contract states, and cancelling a queued call leaves the others in
  flight.
- Tests for lines that nothing checked, found by mutation testing: the value of
  `DEFAULT_MAX_OUTPUT_BYTES`, `BuildInfo::is_complete` in both directions, the calendar
  before the epoch checked against an obvious implementation written beside it,
  `Context::elapsed`, `Context::remaining` and `Context::session`, the registry a built
  dispatcher hands out, and the rule for printing a backtrace, which was split from the
  environment it reads so that every combination could be tested.
- The dev and test profiles carry `debug = "line-tables-only"` and dependencies carry no
  debug information at all, which took `target/debug` from 11.94 GB to 1.33 GB. A panic
  still names the file and the line; a debugger that needs variables gets
  `RUSTFLAGS="-Cdebuginfo=2"` for the one build that needs it.
- `cargo xtask ci` removes the nested trees it built in, `target/ci` and
  `target/llvm-cov-target`, and says how much that freed. They were 10.4 GB between runs.
- `cargo xtask mutants` refuses to run outside continuous integration, where the weekly
  job already runs it, because `cargo mutants` copies the source tree once per job and
  builds every copy: 14.7 GB of temporary trees on this machine. `SOLAR_MUTANTS_ANYWAY=1`
  lifts the refusal.
- `cargo xtask changelog [<base>]`, and the same step in CI: a change under `crates/`
  comes with an entry here. It is the part a machine can check of the fixed rule that
  code and documentation move together, which `AGENTS.md` states and the pull request
  template lists in full.
- `solar-core`: the envelope, the eleven canonical statuses and the reason catalogue, the
  `Api` template, the registry that checks it, dispatch with a worker thread per call that
  turns a panic into `INTERNAL` and an overrun budget into `DEADLINE_EXCEEDED`, the NDJSON
  session loop with a 16 MiB line limit, and the manifest generator.
- Five APIs: `solar.ping`, `solar.version`, `solar.manifest`, `solar.describe` and
  `system.info`.
- `system.info` 1.2.0 reports `abandoned_workers`, the handlers that overran their budget
  and are still running, which is zero in a healthy process.
- `system.info` 1.1.0 reports the release of the operating system in `os_name`,
  `os_release` and `os_build`, read where each system keeps it and never by starting a
  program: the os-release file on Linux, `SystemVersion.plist` on macOS, `RtlGetVersion`
  on Windows. A source that cannot be read leaves the three null and adds an
  `OS_RELEASE_UNAVAILABLE` warning naming what was tried.
- The `solar` binary: `call`, `serve --stdio`, `list`, `describe`, `manifest` and
  `version`, with an exit code per status.
- `cargo xtask manifest` and `cargo xtask new-api <name>`.
- `docs/CONTRACT.md`, `docs/ERRORS.md`, `docs/ADDING_AN_API.md` and
  `docs/OPEN_QUESTIONS.md`.
- `docs/TESTING_BY_HAND.md`, written for somebody who has never used Rust: installing the
  toolchain, building, running every check with one command, the calls worth making by
  hand with what each should print, what only a Mac can confirm, and exactly what to send
  back when something fails. The interactive half is ZENITH's guide, which it points at
  rather than repeats.
- Continuous integration on Linux, Windows and macOS, with the toolchain pinned by
  `rust-toolchain.toml`, every cargo command `--locked`, a weekly early-warning job on the
  latest stable and a weekly proof of the declared minimum.
- The documentation runs: every `bash`, `powershell` and `cmd` block of the Markdown is
  executed by `cargo xtask doc-run` in that shell, in CI, on every push.
- `cargo xtask leak-check`: the release binary is scanned byte by byte and embeds no home
  directory, no user name and no repository root. Release builds carry
  `debug = "line-tables-only"`, so a crash report names the file and the line.
- `solar call` logs at `SOLAR_LOG` levels exactly as a session does; it used to stay
  silent.
- The hints of `solar list` and `solar describe` are pastable in the shell they name:
  `bash`, Windows PowerShell and `cmd.exe` disagree about quotes, so an example with
  parameters is printed once per shell.
- `cargo xtask ci` runs what CI runs, in the same order and with the same flags, and a
  test holds the two together. `cargo xtask compat <base>` classifies every change to the
  manifest as additive or breaking and checks that the version of each API answers for
  it; CI runs it on every pull request.
- The test suite runs under `cargo-nextest`. Snapshots with `insta` freeze every error
  envelope, the manifest and the human output. Property tests with `proptest` prove that
  any line at all produces exactly one well formed response. `conformance/` holds
  request and response pairs that a client in any language can replay, which was checked
  from Python. Three `cargo-fuzz` targets run weekly on nightly.
- A coverage floor of 91% of lines on the shipped crates, enforced by `cargo-llvm-cov`,
  and a weekly mutation run with `cargo-mutants`.
- `cargo-deny` for advisories, licences, sources and duplicates; Dependabot weekly;
  every third-party action pinned by commit SHA; `SECURITY.md`.
- `SOLAR_LOG_FORMAT=json` writes each diagnostic line as one JSON object naming the call.
  `solar serve --stdio --record <file>` writes a session down and `solar replay <file>`
  plays it back, ignoring what differs between two runs. A panic writes its backtrace
  when `RUST_BACKTRACE` is set or the level is debug or finer.
- `docs/STYLE.md`, `docs/adr/` with the nine settled decisions, `CONTRIBUTING.md`, a pull
  request template and `CODEOWNERS`.

[Unreleased]: https://github.com/nipscernlab/solar/compare/v0.2.0...main
[0.2.0]: https://github.com/nipscernlab/solar/releases/tag/v0.2.0
[0.1.0]: https://github.com/nipscernlab/solar/commits/main
