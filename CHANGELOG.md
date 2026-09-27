# Changelog

Every notable change to SOLAR, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the versions follow
[semantic versioning](https://semver.org/spec/v2.0.0.html).

The protocol has a version of its own, `solar/1`, which is not this one. A change to the
protocol is stated here in its own line.

## [Unreleased]

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

[Unreleased]: https://github.com/nipscernlab/solar/commits/main
