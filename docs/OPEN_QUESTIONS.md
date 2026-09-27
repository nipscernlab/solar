# Open questions

Everything here was decided without asking, because the brief said to take the most
conservative option and write down why. Each entry says what was chosen, what it rules
out, and what would make it worth revisiting.

**What is settled has moved.** A decision the architect has confirmed becomes a record in
[`adr/`](adr/) and leaves this file, so that this file is only ever the list of things
still open to being overruled. The eleven records there cover the transport, the
deviation from JSON-RPC, the shape of a response, the naming rule, stability, the
timeout, the worker thread, batches and cancellation.

**Every entry below has a proposal in [`../STATUS.md`](../STATUS.md)**, with its options
and what each one costs, so that they can be decided in one reading rather than one at a
time.

The entries below are grouped by where the decision shows up.

## The envelope

### The envelope is checked in a fixed order

`id`, then `jsonrpc`, then `method`, then `params`. A message that breaks several rules is
answered about the first one in that order.

**Why.** `id` comes first because every later error can then be addressed to the right
call. The order is stated in the contract so that two implementations agree on which error
a broken message deserves.

### A method name that breaks the naming rule is `INVALID_VALUE`, not `NOT_FOUND`

`Solar.Ping` is refused as a malformed name, with a hint that names are lower case, before
the registry is consulted at all.

**Why.** It cannot possibly be registered, and the caller's mistake is the shape of the
name rather than the name itself. The hint offers the lower case spelling, which is the
correction in nine cases out of ten.

### A value echoed in `received` is cut at 200 bytes

A long string is truncated with a `[...]` marker, and a large array or object is replaced
by a sentence describing it.

**Why.** An error about a 16 MiB request must not carry 16 MiB back. The type changes when
a composite is described, which is the cost of the rule; the alternative was to omit the
value, which tells the caller nothing.

## Dispatch

### A registry that does not build answers every call with the same error

Instead of refusing to start, the process serves `INTERNAL` / `INVARIANT_BROKEN` naming
every rule that was broken.

**Why.** The first principle is that every call gets a response. A caller that sees that
error learns more than one whose process exited before it could connect. The contract
tests catch the same problem long before a release.

### The panic hook is global and installed by the dispatcher

It records the location of a panic inside a call and stays quiet; a panic anywhere else is
forwarded to the hook that was already installed.

**Why.** A library that silences panics everywhere would hide test failures. Forwarding
keeps `cargo test` behaving exactly as it did.

## The session

### An oversized line is discarded up to the next newline

SOLAR stops buffering at 16 MiB, answers `RESOURCE_EXHAUSTED` with `id: null`, and then
reads and throws away the rest of that line.

**Why.** "Refuse without reading the rest" cannot mean "stop reading", because whatever
follows on that line would then be parsed as new messages. Nothing oversized is ever held
in memory, which is the part that matters, and the stream stays aligned so the next
message is answered normally.

### The session reads ahead, and still runs one call at a time

**Superseded on 27 September 2026 by record [11](adr/0011-cancelling-is-an-ordinary-call.md),
and kept here because what it says about ordering is still true.**

It used to say that one request was read, dispatched and answered before the next was
read. Cancellation made that impossible: a `solar.cancel` sent while a call was running
would have waited in the pipe until the call it was cancelling had finished.

A session now reads on a thread of its own, into a bounded queue. **Calls still run one at
a time, in the order they arrived**, so a client that never cancels sees its responses in
the order of its requests, which is what the old entry was protecting and is now a test.
What changed is only that reading no longer waits for running.

## The APIs

### `system.info` reports the release of the operating system

**Overruled by the architect on 26 September 2026.** It was left out of the first draft on
the grounds that reading it meant running `sw_vers` or `lsb_release`, and this stage
starts no external program. That was wrong: every system keeps it somewhere a process can
read directly.

- **Linux**, and anything else following the os-release specification: `/etc/os-release`,
  then `/usr/lib/os-release`, which is the fallback that specification names.
- **macOS**: `/System/Library/CoreServices/SystemVersion.plist`, for `ProductVersion` and
  `ProductBuildVersion`.
- **Windows**: `RtlGetVersion` in `ntdll`. `GetVersionEx`, its documented alternative,
  reports an older version to a program that carries no compatibility manifest, so it
  would have this build call Windows 11 something else.

A source that is missing or unreadable leaves `os_name`, `os_release` and `os_build` null
and adds an `OS_RELEASE_UNAVAILABLE` warning naming the source that was tried. Nothing is
guessed, and the call still succeeds. `system.info` went to 1.1.0 for it: members were
added and none changed, which is a minor bump under section 10 of the contract.

### Unsafe code is denied rather than forbidden, for exactly one function

`RtlGetVersion` is a foreign function, and calling it is `unsafe`. The workspace lint went
from `forbid` to `deny`, and `windows_version` in `crates/solar-apis/src/os_release.rs`
carries the only `allow(unsafe_code)` in the repository.

**Why.** `forbid` cannot be lifted anywhere, at all, which would have left starting a
program as the only way to ask Windows what it is. `grep -rn "allow(unsafe_code)" crates/`
finds the whole of the exception, and the call is six lines with the safety argument
written above it.

### `cpu_count` may be `null`

When the system does not report how many threads can run at once, the member is `null`
rather than `1`.

**Why.** `1` would be a guess that looks like a measurement.

## Naming and versions

### The manifest layout version is semantic

`schema_version` is `"1.0.0"`, not `1`. A consumer must refuse a manifest whose major
differs from the one it was written against.

**Why.** The same rule as everything else in the repository, and it leaves room to add a
member in a minor bump without every consumer refusing the file.

### Suggestions come from the Levenshtein distance, at most three of them

A candidate is offered when the distance is at most 3 and smaller than the candidate's own
length. Ties are broken alphabetically.

**Why.** The threshold catches a typed letter, a swapped pair and a missing word, and
refuses to guess when the caller wrote something else entirely. Alphabetical ties make the
same mistake produce the same error every time, which matters when the error is in a test.

### The declared minimum Rust version is 1.97

`rust-version = "1.97"` is the compiler this was built and tested with.

**Why.** Edition 2024 needs 1.85, and an older compiler may well work, but nothing here
has been tested on one. Declaring a version that was never tried is the kind of claim that
costs someone an afternoon.

## Examples and tests

### An example declares how it is compared, and `$any` stands for what cannot be reproduced

`exact` compares everything, `subset` compares what the example names, and the string
`"$any"` matches any value at that position.

**Why.** Examples are tests, and a test that cannot pass is worse than no test. A
timestamp, a path, a duration and a compiler version are different on every machine, and
an example that pretended otherwise would either fail everywhere or be quietly excluded.
`$any` keeps the shape checked while admitting what is not knowable in advance.

### Integration tests lift the lints that forbid panicking

`tests/` and `benches/` carry an `allow` for `unwrap`, `expect` and `panic`.

**Why.** A test reports failure by panicking. `clippy.toml` covers `#[cfg(test)]` modules
inside a crate, and an integration test is its own crate, so the allow is written at the
top of each file with the reason beside it.

## The command line interface

### A misuse of the command line exits 2, the same as `INVALID_ARGUMENT`

An unknown subcommand, a missing argument, an unreadable `--log` level.

**Why.** It is the same kind of mistake, and a script that checks for 2 should not have to
learn a second number for it.

### `solar manifest` indents on a terminal and prints one line into a pipe

`--pretty` forces the indented form.

**Why.** A person reading a 1100 line document wants it laid out; a program reading it
wants one line. Nothing else in the interface changes with the terminal.

### The mark is printed by `solar version` and nowhere else

On a terminal only, coloured only when `NO_COLOR` is absent, never in a pipe.

**Why.** `docs/brand/README.md` requires it, and the contract requires that
machine-readable output stay exact. `solar version` is the one human command where the
mark says something: which build is answering.

### The ASCII drawing of the mark is a switch, not a guess

`SOLAR_ASCII` with any value picks the 7-bit drawing.

**Why.** There is no reliable way to ask a terminal whether it can draw a half block.
Guessing from `TERM` or a code page would be wrong on somebody's machine, and the failure
would be a screen of mojibake. A documented switch is honest about what is not knowable.

## Scope of this stage

### No API runs an external program

`tools.detect`, the table of known tools and the process runner were written and then
removed, on the architect's instruction, so that this stage is only the core of the API.
They are in the history at commit `1029db4` and its parent, and the vocabulary they need,
`spawns_process` among the side effects, is still in the contract.

**Consequences.** `docs/CONTRACT.md` has no section about running external programs, and
the error catalogue has no reason for a program that cannot be started. Both come back
with the first API that needs them. The reasons removed were `UNKNOWN_TOOL`,
`TOOL_TABLE_INVALID`, `ACCESS_DENIED`, `PROGRAM_TIMEOUT` and `SPAWN_FAILED`, and the
warning codes were `TOOL_TABLE_OVERRIDDEN`, `VERSION_NOT_PARSED` and
`TOOL_EXITED_NON_ZERO`.

### Continuous integration runs on three platforms

The workflow builds, lints, tests, checks the manifest and builds a release on
`ubuntu-latest`, `windows-latest` and `macos-latest`, and a fourth job builds the API
documentation with `-D warnings`.

**State.** It ran on the first push and passed on all four jobs: run
[36287385822](https://github.com/nipscernlab/solar/actions/runs/36287385822), with Windows
the slowest at 6m24s. The work was done on Windows, so Linux and macOS were unverified
until that run.

## Stage 2

### Local paths are stripped with computed remap flags, not committed ones

`trim-paths`, the profile key that will one day do this, still needs `-Z` on the pinned
1.97, which was verified against the toolchain rather than assumed. The fallback is
`--remap-path-prefix`, and the two prefixes that leak, the home directory and the
repository root, differ per machine, so they cannot live in a committed configuration
file. They are computed in `xtask/src/flags.rs` and applied to every build xtask makes,
and CI exports the same flags for the builds it makes directly.

**The consequence recorded plainly:** a bare `cargo build --release` outside xtask, on a
developer's machine, still embeds that machine's paths. What is enforced is the artefact
that matters: `cargo xtask leak-check` builds the release binary with the flags and then
scans every byte of it for the home directory, the user name as a path segment, and the
repository root, in both slash spellings. CI runs it on the three systems. When
`trim-paths` stabilises, the flags move into the release profile and this entry closes.

### The documentation runner executes blocks, and no-run is the audited exception

Every fenced block tagged `bash`, `powershell` or `cmd` is executed by `cargo xtask
doc-run` in that shell. A block tagged `no-run` is skipped, its first line says why, and
today there are five: two that would mutate the working tree (`cargo xtask new-api`), and
three lists of commands that already run as their own CI steps, where executing them again
would only double the pipeline. `powershell` means Windows PowerShell 5.1, the shell whose
quoting the README documents, so those blocks run on the Windows job.

### The repository is written in British English

Licence, serialise, behaviour, catalogue. The first stage already wrote that way; this
makes it a rule. `typos` runs with `locale = "en-gb"`, so an American spelling is flagged
like any other typo, and `docs/STYLE.md` states the choice for prose that tools cannot
check.

### Pedantic lints are fixed or allowed at the site, never at the workspace

The brief says each allowed lint is listed at workspace level with its reason. Every
pedantic finding was instead either fixed or allowed exactly where it fires, with
`reason = "..."` on the attribute, which `clippy::allow_attributes_without_reason`, denied
workspace-wide, enforces mechanically. A workspace-level allow would silence a lint
everywhere, including the future places where it is right; a local allow with a mandatory
reason keeps the lint alive and the exception argued. The letter of the brief bends, its
intent, that nothing is silenced without a written reason, is enforced by machine.

### `cargo xtask ci` builds its nested commands in a target directory of their own

The one command is itself `target/debug/xtask.exe`. A nested `cargo test --workspace`
relinks that very file, and Windows refuses to replace a running executable: *failed to
remove file ... Access is denied*. The same happens to the documentation blocks that call
`cargo xtask manifest`.

Every nested build therefore uses `target/ci`, passed explicitly rather than through the
environment, since `std::env::set_var` is `unsafe` in edition 2024 and unsafe code is
denied. The cost is one extra build tree, cached like any other; the first run of
`xtask ci` after a change to the sources is slower than the second. CI itself runs the
cargo commands directly, where nothing is running from the tree, so it keeps the default
directory.

**One place it was wrong, found on 27 September 2026 and fixed.** The same environment
variable was also given to `cargo mutants`, which does not build in the real tree at all:
it copies the sources once per job and builds each copy. One shared target directory made
cargo reuse a test binary built in another copy, and a test binary carries the
`CARGO_MANIFEST_DIR` of the tree that compiled it, so `tests/docs.rs` went looking for the
contract in a directory that had already been deleted. Worse than the failure was what it
implied: a mutant could be judged by an artefact built from a different mutant. The
variable is gone from the mutation task, with a comment saying why it is not an oversight,
and the mutation score was measured again from scratch.

### `loom` is not adopted, and here is what it would have to model

The brief asks for `loom` if the reusable worker and the abandonment of an overrunning
call have shared state it can model, and for the reason if not. They do not, in the sense
that matters.

What the worker actually shares is one `std::sync::mpsc` channel pair between exactly two
threads, and one `Mutex<Vec<Warning>>` inside the context. There is no lock ordering,
because there is one lock. The interleavings that could go wrong are the ones inside
`mpsc` and `Mutex`, which are the standard library's to prove, not this repository's.

Using `loom` would mean making `dispatch` generic over its synchronisation primitives, or
duplicating it behind `cfg(loom)`, so that `loom::sync` could replace `std::sync`. That is
a real change to the shape of the code under test, and what it would prove is that the
standard library works.

What is tested instead is the behaviour that the concurrency exists for, in
`solar-core/tests/dispatch.rs`: a handler that panics is caught and the session survives,
a handler that overruns is abandoned inside its budget, the next call does not wait for
the abandoned one and does not receive its answer, and fifty calls in a row reuse one
worker and stay correct. Those tests would fail if the channel handling were wrong.

**Revisited on 27 September 2026, as this entry said to be.** Cancellation added exactly
what the trigger named: a second lock, the `Mutex<Inner>` of `SessionState`; atomics, the
one-way flag of `Cancellation` and the counter of abandoned workers; and a second thread
per session. The answer is still no, for a reason that has changed:

**Every transition of the session state happens under one lock.** A message is queued,
taken, cancelled or finished inside `SessionState`, and each of those is one critical
section; no lock is ever held while another is taken, so there is no ordering to get
wrong. What is left for `loom` to explore is the order in which those critical sections
run, and that order is small enough to enumerate: a cancellation arrives while its target
is queued, while it is running, or after it is finished. Each of the three is tested
deterministically in `solar-apis/tests/cancellation.rs`, with handlers that stop inside the
session until the test lets them go, which is stronger than repeating an unsynchronised
race and hoping to hit them. The unsynchronised race is also repeated, forty times, as a
second net.

**Revisit when** a transition of the session state happens outside that lock, or when the
two threads talk to each other through atomics rather than through it. Either makes the
interleavings SOLAR's own in a way enumeration cannot cover, and then `loom` earns the
change to the shape of the code.
