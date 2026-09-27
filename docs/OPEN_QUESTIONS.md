# Open questions and decisions taken alone

Everything here was decided without asking, because the brief said to take the most
conservative option and write down why. Each entry says what was chosen, what it rules
out, and what would make it worth revisiting. Anything the architect disagrees with is
cheap to change now and expensive to change after other institutions depend on it.

The entries are grouped by where the decision shows up.

## The envelope

### `params: null` is refused

**Confirmed by the architect on 26 September 2026.**

`params` may be absent, and when it is present it must be an object. `null` is present and
is not an object, so it gets `TYPE_MISMATCH` with a hint that says to omit the member.

**Why.** This is the specification, not only strictness. JSON-RPC 2.0, section 4.2,
requires `params`, when it is present, to be a structured value: an object or an array.
`null` is neither, so a message carrying it is already invalid before SOLAR has an opinion.
SOLAR then narrows the structured value further, to an object, because parameters are
always passed by name here; that narrowing is SOLAR's own and section 3 of the contract
states it.

**Why it was flagged.** Many clients do send `"params": null`, and accepting it later
would have been a compatible change. The architect confirmed the refusal.

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

### `error.data` has exactly four members

**Confirmed by the architect on 26 September 2026.**

`status`, `reason`, `details`, `meta`. A fifth member saying whether the failure is worth
retrying was considered and dropped.

**Why.** The architect's example shows four, and a caller that can pattern match on a
fixed shape is worth more than one convenience field. Retriability is documented per
status in `docs/ERRORS.md`, where an agent reads it once instead of on every error.

### A value echoed in `received` is cut at 200 bytes

A long string is truncated with a `[...]` marker, and a large array or object is replaced
by a sentence describing it.

**Why.** An error about a 16 MiB request must not carry 16 MiB back. The type changes when
a composite is described, which is the cost of the rule; the alternative was to omit the
value, which tells the caller nothing.

## Dispatch

### A handler that overruns is abandoned, not killed

**Confirmed by the architect on 26 September 2026.**

The response goes out at the deadline. The thread carries on until it finishes and its
result is thrown away.

**Why.** Rust has no safe way to kill a thread, and the alternatives are worse: no
timeout at all, or a cancellation token every handler has to remember to check. The
contract states it plainly so that nobody assumes the work was undone. A handler that
starts anything long lived is expected to give it a shorter budget of its own.

### One reusable worker thread per dispatching thread

A thread is started on the first call and reused. A call that times out drops it, so the
next call starts a fresh one.

**Why.** Measurement: starting a thread for every call cost about 70 µs of the 77 µs a
ping took. The same ping now costs 7.59 µs. An abandoned thread is never reused, because
its next answer would belong to the call that gave up on it.

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

### The session is strictly sequential

One request is read, dispatched and answered before the next is read.

**Why.** Responses come back in request order, so no caller has to correlate anything, and
there is no shared state to get wrong. Concurrency is a protocol change, not an
implementation detail, and it is not needed by any interface that exists today.

## The APIs

### `solar.manifest` with `api` keeps the shape of the whole document

**Confirmed by the architect on 26 September 2026.**

`{"api": "solar.ping"}` returns a manifest whose `apis` holds one entry, not a bare entry.
`solar.describe` is the call that returns the bare entry.

**Why.** One API, one response shape. That is the rigidity principle applied to the
answer rather than to the declaration: a caller that narrows the manifest parses exactly
what it parses when it does not. An API that needs the bare entry calls the API whose job
that is.

### Every API is `experimental`, and a test enforces it

**Confirmed by the architect on 26 September 2026.**

While `solar_version` is below `1.0.0`, an API that claims `stable` fails the build.

**Why.** A rule is better than a judgement per API. Nothing in SOLAR can be more stable
than SOLAR, and the first stable release is the moment to make that promise deliberately.

### `system.info` does not report the release of the operating system

It reports the system, the family, the architecture, the pointer width, the thread count,
three paths and the two separators. Not the version string.

**Why.** Reading that version means asking the operating system, which on Linux and macOS
means running `sw_vers` or `lsb_release`. This stage starts no external program. When
external programs come back, this is the first field to add.

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

### The file, the module and the struct follow from the method name

**Confirmed by the architect on 26 September 2026.**

`build.run_target` gives `build_run_target.rs`, `mod build_run_target` and
`struct BuildRunTarget`. No exceptions.

**Why.** Three things that must agree are derived from one, so a person, a tool or an
agent can work out any of them from any other without looking. The existing APIs were
renamed to obey it: `solar.ping` is `SolarPing`, not `Ping`.

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
