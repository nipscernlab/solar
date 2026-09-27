# Status

**Stage:** two, the engineering foundation. **Version:** 0.1.0. **Protocol:** `solar/1`.
**Written on:** 27 September 2026. **Machine everything was built and measured on:**
Windows 11 Home Single Language 26200, Intel Core i7-13620H, 16 hardware threads,
`rustc 1.97.1`, `x86_64-pc-windows-msvc`.

When this stage is finished, the only work left in this repository is adding APIs and
their tests. Everything else is in place and enforced by a machine.

## What one command does

```text
cargo xtask ci
  formatting            cargo fmt --all -- --check
  toml formatting       taplo fmt --check
  spelling              typos, British English
  lints                 clippy, pedantic, -D warnings
  tests                 cargo nextest run --workspace --locked
  doctests              cargo test --doc, which nextest does not run
  documentation         cargo doc with -D warnings
  manifest              the versioned manifest is what the generator produces
  compatibility         what changed against main, and whether the version answers
  documentation runs    every bash, powershell and cmd block, in its shell
  no local paths        the release binary names no user and no machine
  supply chain          cargo deny: advisories, licences, sources, duplicates
  coverage              the shipped crates, against a floor of 91% of lines
```

It runs every step even after one fails and ends with a table. A test holds it to the
workflow: every check is in both, in the same order, and no cargo command in CI is
missing `--locked`.

## The tools adopted, and why each one

| Tool | Version | What it answers |
| ---- | ------- | ---------------- |
| `cargo-nextest` | 0.9.146 | Runs each test in its own process, so one cannot take its neighbours down. A CI profile retries the two tests that measure wall clock time and reports a pass-on-retry as flaky rather than as a pass. |
| `insta` | 1.48 | Freezes what goes on the wire and what goes on the screen. Thirteen envelopes, the manifest, and the five human outputs. |
| `proptest` | 1.11 | Proves the first principle against inputs nobody thought of: any line at all produces exactly one well formed response. |
| `cargo-fuzz` | 0.13 | Three targets, nightly, weekly. Twelve million executions against the envelope parser in three minutes, no crashes. |
| `cargo-llvm-cov` | 0.9.1 | The coverage floor, enforced at 91% of lines. |
| `cargo-mutants` | 27.1 | Changes the code and sees whether a test fails. Coverage says a line ran; this says something checked it. |
| `cargo-deny` | 0.20.2 | Advisories, licences, sources, duplicate versions. |
| `typos` | 1.50.3 | British English, with the wire constants and serde's identifiers exempted by name. |
| `taplo` | 0.10 | TOML formatting. |
| `loom` | not adopted | `docs/OPEN_QUESTIONS.md` says what it would have to model: one channel between two threads, one mutex, no atomics of our own. |

## What was measured

| What | Figure |
| ---- | ------ |
| Tests | 228, of which 108 in `solar-core`, 67 in `solar-apis`, 40 in `solar-cli`, 13 in `xtask` |
| Coverage of the shipped crates | **91.33% of lines**, 91.63% of regions. The floor is 91 and only ever rises. |
| Conformance cases | 11, in plain JSON, replayable by a client in any language |
| Fuzzing, envelope parser | 12 008 867 executions in 181 s, no crashes |
| Release binary | 1 548 288 bytes with `debug = "line-tables-only"`, up from 1 461 760 without it |
| `solar call solar.ping`, whole process | 4.16 ms, down from 5.64 ms before the remap and line tables |
| One round trip in a session | 72.3 µs |
| Dispatch of one ping, in process | 7.59 µs |

**The mutation score.** One file was measured to the end: `crates/solar-core/src/text.rs`,
**42 mutants caught, 0 survived**, and one that hung the suite, which is the suite
noticing. Reaching that took four new tests and one simplification, which is finding 5
below.

The whole of the two library crates is 538 mutants, and measuring it on this laptop takes
hours, because each mutant rebuilds and reruns the suite. That is the weekly job's work,
and the first run that will report an honest number is the one after finding 8 below,
since the configuration was not being read before that. The number to expect is lower
than the coverage: coverage says 91% of lines ran, and mutation asks the harder
question.

## What the new checks found, on the day they were written

Each of these was a real defect, found by the thing built to find it.

1. **The README claimed a quoting form that does not work in `cmd.exe`.** Found by
   running the documentation. Every shell-tagged block is now executed by
   `cargo xtask doc-run`, in its own shell, in CI.
2. **`SOLAR_LOG=trace` wrote nothing in `solar call`.** Found by trying it. A call now
   logs what a session logs, and a test pins it.
3. **Windows PowerShell prepends a byte order mark** to the first line it writes to a
   native program, which cost every PowerShell session its first request. Found by
   driving `solar serve --stdio` from PowerShell. RFC 8259 allows a parser to ignore
   one, and now SOLAR does.
4. **The release binary embedded the home directory of whoever built it**, and on macOS
   it still did after the first fix, through the OSO entries that `ld64` writes. Found
   by `cargo xtask leak-check`, which scans the artefact rather than trusting the flags.
5. **Four lines of `text.rs` could be wrong without a test failing**, including the
   whole deletion term of the edit distance. Found by `cargo-mutants`. All four are now
   tested, and a fifth finding was an equivalent mutant, which was removed by deleting a
   redundant guard.
6. **`cargo xtask ci` could not run its own tests on Windows**, because the nested
   `cargo test` relinks the binary that is running. Found by running it. Nested builds
   now use a target directory of their own.
7. **A fresh clone could not run `cargo xtask ci`.** The documentation says
   `./target/release/solar`, and the documentation runner was building the binary into
   the nested target directory that `xtask ci` gives its children, so the path in the
   README was true only on a machine that had already built by hand. Found by cloning
   the repository into a temporary directory and running the one command, which is now
   how this stage is declared finished.
8. **The mutation configuration was never being read.** `cargo-mutants` looks for
   `.cargo/mutants.toml`, not `mutants.toml`, so the weekly run mutated the developer
   tools as well and reported 218 survivors that were mostly in `xtask`. Found by
   reading the first weekly run instead of trusting it.

## What runs when

| When | What |
| ---- | ---- |
| Every push and pull request | The thirteen steps above, on `ubuntu-latest`, `windows-latest` and `macos-latest`, plus a documentation job and a coverage job |
| Every pull request | The compatibility check against the base branch |
| Weekly, Monday 06:00 UTC | The latest stable compiler as an early warning; the declared minimum, proving `rust-version`; three fuzzing targets for three minutes each; the full mutation suite |

The weekly jobs can be run by hand from the Actions tab, and were, to prove they work
rather than to wait a week and hope.

## What Chrysthofer has to turn on

**Repository settings were not changed**, as the brief instructed. These are the ones to
set, at **Settings, Branches, Add branch ruleset** for `main`:

| Setting | Value |
| ------- | ----- |
| Require a pull request before merging | on |
| Required approvals | 1 |
| Require review from Code Owners | on, which makes `.github/CODEOWNERS` binding |
| Dismiss stale approvals when new commits are pushed | on |
| Require status checks to pass | on, and select `ubuntu-latest`, `windows-latest`, `macos-latest`, `documentation` and `coverage` |
| Require branches to be up to date before merging | on |
| Require conversation resolution before merging | on |
| Block force pushes | on |
| Restrict deletions | on |

Two things to know before turning them on. The first is that this stage was written by
pushing straight to `main`, so the rule starts applying to the next change, not to the
history. The second is that a ruleset applies to its author as well unless bypass is
granted, which is the point of having one.

Also worth enabling, at **Settings, Code security**: Dependabot alerts and the secret
scanning that GitHub offers for public repositories. `.github/dependabot.yml` already
asks for the weekly version updates; the alerts are a separate switch.

## What is left

**For the architect.** The nine decision records in `docs/adr/`, which are the settled
ones, and the entries still open in `docs/OPEN_QUESTIONS.md`. The manifest,
`manifest/solar.manifest.json`, is still the whole surface in one file and is still the
thing most worth a careful read.

**Not started, and out of scope by instruction.** Anything that runs an external
program: `tools.detect`, the table of known tools and the process runner, which are in
the history at `1029db4` and its parent. Releases, distribution, installers and signing.

**Known gaps, in the order they will start to hurt.**

- The mutation score of the two library crates has never been measured to the end, only
  `text.rs` has. The weekly job does it now that its configuration is read; the list of
  survivors it prints is a list of missing tests, and working through it is the next
  stage's cheapest way to buy confidence.
- The manifest is 1152 lines for five APIs, because every JSON Schema is inlined in
  full. It will not stay readable at fifty. A shared `$defs` section is the answer and
  is a change to `schema_version`.
- `cargo xtask ci` takes about ninety seconds on a warm tree and several minutes on a
  cold one, mostly in the documentation runner and the coverage step. If that becomes a
  reason not to run it, the fix is a `--fast` that skips those two, not running it less.
- There is no cancellation in `solar/1`, and no way for a caller to say it has stopped
  waiting.
- Batches are refused. If AURORA turns out to make many small calls in a burst, the
  round trip of 72 µs is the number to beat.
- Nothing measures a call under load: every figure here is one call at a time on an idle
  machine.
