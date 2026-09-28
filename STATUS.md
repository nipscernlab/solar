# Status

**Stage:** four, the architect's decisions and the three things ZENITH needed.
**Version:** 0.3.0. **Protocol:** `solar/1`. **Written on:** 27 September 2026. **Machine everything
was built and measured on unless another is named:** Windows 11 Home Single Language
26200, Intel Core i7-13620H, 16 hardware threads, `rustc 1.97.1`,
`x86_64-pc-windows-msvc`.

Stage two left an engineering foundation. Stage three closed what it left open: the lines
nothing checked, a manifest that would not stay readable, a session that could not be
cancelled, memory that was bounded nowhere, and figures that were all taken one call at a
time on an idle machine. It also fixed something that stage broke: a local mutation run
filled this machine's disk, and what a build of this repository costs is now measured and
bounded like everything else.

Stage four settled what had been decided alone, and gave ZENITH the three things it had
written down as missing. **No new functional API**: `solar.set_log_level` moves where the
diagnostics go and nothing else.

## What stage four added

| What | Where it is decided | Where it is enforced |
| ---- | ------------------- | --------------------- |
| **Twenty-seven decisions become records**, 12 to 38, and `docs/OPEN_QUESTIONS.md` holds nothing open | the architect, 27 September 2026 | `docs/adr/`, one file each, MADR |
| **`capabilities` in the manifest**: what the protocol accepts and every limit | contract 8.2 | a test compares each number with the constant that enforces it |
| **`solar.set_log_level`**: the level changes while a session runs | contract 2 | a test through the real binary: the diagnostics move, the responses do not |
| **A versioned recording format** | contract 12, [`docs/RECORDING.md`](docs/RECORDING.md) | a JSON Schema compiled out of the document, and a hand-written recording that replays |
| **The mutation score, measured in CI** | — | the ceiling in `xtask/src/mutants.rs`, set from the run rather than inherited |

## What stage three added

| What | Where it is decided | Where it is enforced |
| ---- | ------------------- | --------------------- |
| **Batches**, answered in the order they were sent | contract 3.2, record [0010](docs/adr/0010-batches-answer-in-order.md) | property tests, four conformance cases, a fuzz target |
| **`solar.cancel`**, an ordinary call, with a session that reads ahead | contract 9, record [0011](docs/adr/0011-cancelling-is-an-ordinary-call.md) | `crates/solar-apis/tests/cancellation.rs`, nine tests that drive the races on purpose |
| **Four declared limits** so that memory is bounded | contract 8.4, 9.6, 9.7, 10 | a test at each limit, and the soak run below |
| **Shared definitions in the manifest** | contract 8.1 | `cargo xtask compat` resolves `$ref` before comparing, and is tested for both directions |
| **`cargo xtask ci --fast`** | — | a test holds the full command to the workflow |
| **`cargo xtask load`**, and the figures under load | — | the numbers below, reproducible with one command |
| **`cargo xtask changelog`**, code and documentation move together | `AGENTS.md`, the fixed rule | a step of the one command and of CI |
| **`docs/TESTING_BY_HAND.md`** | — | every runnable block of it is run by `cargo xtask doc-run` |
| **A build that fits on a laptop**, after one filled this disk | the reasons are in `Cargo.toml`, `xtask/src/ci.rs` and `xtask/src/mutants.rs` | the figures below, measured before and after |

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
  changelog             a change under crates/ came with a CHANGELOG.md entry
  documentation runs    every bash, powershell and cmd block, in its shell
  no local paths        the release binary names no user and no machine
  supply chain          cargo deny: advisories, licences, sources, duplicates
  coverage              the shipped crates, against a floor of 91% of lines
```

It runs every step even after one fails and ends with a table. A test holds it to the
workflow: every check is in both, in the same order, and no cargo command in CI is
missing `--locked`.

`cargo xtask ci --fast` skips the documentation runner and the coverage step, which are
most of the wall clock, and says so in the summary. It is the loop between commits; the
full command is the gate before a push.

## The tools adopted, and why each one

| Tool | Version | What it answers |
| ---- | ------- | ---------------- |
| `cargo-nextest` | 0.9.146 | Runs each test in its own process, so one cannot take its neighbours down. A CI profile retries the two tests that measure wall clock time and reports a pass-on-retry as flaky rather than as a pass. |
| `insta` | 1.48 | Freezes what goes on the wire and what goes on the screen. Thirteen envelopes, the manifest, and the five human outputs. |
| `proptest` | 1.11 | Proves the first principle against inputs nobody thought of: any line at all produces exactly one well formed response. |
| `cargo-fuzz` | 0.13 | Four targets, nightly, weekly, the fourth being batches. Twelve million executions against the envelope parser in three minutes, no crashes. |
| `cargo-llvm-cov` | 0.9.1 | The coverage floor, enforced at 91% of lines. |
| `cargo-mutants` | 27.1 | Changes the code and sees whether a test fails. Coverage says a line ran; this says something checked it. |
| `cargo-deny` | 0.20.2 | Advisories, licences, sources, duplicate versions. |
| `typos` | 1.50.3 | British English, with the wire constants and serde's identifiers exempted by name. |
| `taplo` | 0.10 | TOML formatting. |
| `dhat` | 0.3.3 | What one call allocates, counted rather than guessed, with a ceiling. |
| `loom` | not adopted | Record [38](docs/adr/0038-loom-is-not-adopted.md) says what it would have to model, and was revisited when cancellation added a second lock. Every transition of the session state happens under one lock, and the three orderings that matter are tested deterministically. |

## What was measured

Every figure here was produced on this machine by a command anybody can run again. None
of them is a promise about another machine.

| What | Figure |
| ---- | ------ |
| Tests | **370**, all passing on Windows and on Linux |
| Conformance cases | **17**, in plain JSON, replayable by a client in any language |
| Coverage of the shipped crates | **94.67% of lines**, 94.74% of functions, 94.10% of regions, after the tests that killed the mutants. The floor is 91 and only ever rises. |
| Decision records | **38**, after the twenty-seven confirmed on 27 September 2026 |
| The manifest | **1 088 lines** for six APIs, with shared `$defs`. It was 1 152 lines for five before they were shared, which is 230 lines per API then and 181 now |
| Release binary | **1 782 272 bytes** on Windows, 9 486 144 on Linux, both with `debug = "line-tables-only"` |

### Latency, one call at a time

Unchanged from stage two, and reproduced here so that the load figures below have
something to sit beside. `cargo bench`, criterion, median of the confidence interval.

| What | Median |
| ---- | ------ |
| `solar call solar.ping`, the whole process | 4.16 ms |
| One round trip in a session | 72.3 µs |
| Dispatch of one ping, in process | 7.59 µs |

### Under load

`cargo xtask load`, which drives a real `solar serve --stdio` through a real pipe and
waits for each response before sending the next. The driver's own cost is inside every
figure, because that is what a client experiences. Reading another process's resident set
costs a process of its own on Windows, so the sampling is rare and its cost is measured
and taken out of the wall clock rather than counted as SOLAR's.

| Run | Throughput | p50 | p90 | p99 | Max | Resident memory |
| --- | ---------- | --- | --- | --- | --- | ---------------- |
| 10 000 pings | **15 095 per second** | 61 µs | 85 µs | 121 µs | 713 µs | 4 380 → 4 860 KiB |
| 10 000 calls, every other one an error | **19 253 per second** | 47 µs | 71 µs | 100 µs | 354 µs | 4 384 → 4 888 KiB |
| 1 000 000 pings, `--soak` | **13 418 per second** | 69 µs | 87 µs | 117 µs | 64 597 µs | 4 388 → peak 4 872 → end 4 832 KiB |

**Memory does not grow with the calls a session answers.** A million requests left the
resident set within half a mebibyte of where it started, and no higher than ten thousand
had left it. That is what the limits of sections 8.4, 9.6, 9.7 and 10 exist to guarantee,
and it is measured rather than argued.

Errors are answered **faster** than successes, which was not guessed: an error response is
smaller, and it stops before the handler runs.

The worst case of the soak run, 64 ms, is one sample in a million: the operating system
scheduling two processes, not SOLAR. The p99 of the same run is 117 µs.

**Cancellation cost half the throughput, and that was measured rather than discovered
later.** Before the session read ahead, the same 10 000 pings ran at **28 814 per second
with a p50 of 32 µs**; they now run at 15 095 with a p50 of 61 µs. Every message crosses a
thread boundary that did not exist before: the reader puts it in the queue, the runner is
woken, and the answer comes back. That is about 29 µs per call, and it is what buys a
session that can be cancelled while a call is running.

The obvious suspect was measured and cleared: the response size check of section 8.4
serialises `result.data` a second time, and removing it entirely raises the figure from
15 095 to 16 180 per second with the p50 unchanged at 60 µs, so it is about 7% of the
throughput and none of the latency. The thread handoff is the whole of the rest.

**Nothing was optimised for it**, because nothing measured says it is a problem: 15 000
calls a second is 66 µs each, and the interfaces that call SOLAR make tens of calls per
interaction rather than tens of thousands. The number is here so that the next person to
look does not have to rediscover where it went.

### What a build costs on disk

Measured on 27 September 2026, after a local mutation run filled the disk.

| What | Before | After |
| ---- | ------ | ----- |
| `target/debug` | **11.94 GB** | **1.33 GB** |
| `target/ci`, the nested tree of the one command | 8.61 GB, kept between runs | removed when `cargo xtask ci` finishes |
| `target/llvm-cov-target` | 1.82 GB, kept | removed with it |
| `target/` in all | **23.80 GB** | **3.07 GB**, after a full `cargo xtask ci`, the whole suite, a release build and a soak run. The budget is 20 GB |
| The temporary trees of a local mutation run | 14.7 GB for eight jobs | none: it runs in CI |

Three changes, each with its reason written where it is made:

- **The dev and test profiles carry `debug = "line-tables-only"`, and dependencies carry
  none.** A panic still names the file and the line, which is what a backtrace is read
  for. A debugger that needs variables gets `RUSTFLAGS="-Cdebuginfo=2"` for the one build
  that needs it. This is where the ten gigabytes were.
- **`cargo xtask ci` removes the trees it built in.** They are scaffolding, not a cache:
  rebuilding them costs about a minute and keeping them cost nine gigabytes between runs.
- **Mutation testing runs in continuous integration and not on a laptop.** `cargo mutants`
  copies the whole source tree once per job and builds every copy. The weekly job does the
  same work on a runner that is thrown away, so the check is not lost; `cargo xtask
  mutants` now says so and refuses, and `SOLAR_MUTANTS_ANYWAY=1` lifts the refusal for
  somebody who has the disk and means it.

**The footprint to stay under is 20 GB**, raised from 10 by the architect on
27 September 2026, and a full `cargo xtask ci` followed by a release build is what to
measure it with. Nothing that was done to fit under 10 is undone: the measured footprint
is 3.07 GB, and the extra room is for a later stage rather than for letting this one
grow.

### What one call allocates

`crates/solar-apis/tests/heap.rs`, with `dhat` as the global allocator of that test
binary, in the test profile:

| What | Figure |
| ---- | ------ |
| `solar.ping`, blocks per call | **51** |
| `solar.ping`, bytes per call | **5 278** |

Both are now ceilings, at 60 and 6 000, so a change that allocates more fails a test and
has to say why in its pull request. This is the same bargain as the coverage floor and
the mutation ceiling.

### The four limits, declared and enforced

| Limit | Value | What happens past it |
| ----- | ----- | --------------------- |
| The input queue of a session | **256 requests and 64 MiB** | `RESOURCE_EXHAUSTED` / `QUEUE_FULL`, answered at once, while `solar.cancel` is still accepted |
| The identifiers a session remembers | **1 024** | an older `id` is reported `unknown` rather than `already_finished` |
| Abandoned handlers alive at once | **64** | `UNAVAILABLE` / `TOO_MANY_ABANDONED` before the call starts; the count is `system.info.abandoned_workers` |
| The response of one API | **`max_output_bytes`, 8 MiB by default** | `RESOURCE_EXHAUSTED` / `OUTPUT_TOO_LARGE`, and the result is discarded |
| One batch | **64 elements** | `RESOURCE_EXHAUSTED` / `BATCH_TOO_LARGE`, refusing the batch as a whole |
| One request line | **16 MiB** | `RESOURCE_EXHAUSTED` / `MESSAGE_TOO_LARGE`, already in stage one |

Each one has a test at the limit and a test one past it.

### Mutation, measured at last

Coverage says a line ran. Mutation changes the line, runs the suite, and reports where
nothing failed. Stage two reported 98 survivors out of 538 mutants; stage three found that
number was not a measurement and fixed the two faults behind it. **Stage four ran the
weekly job by hand and read what it said.**

| | Stage two, not a measurement | Measured on 27 September 2026 |
| --- | --- | --- |
| Mutants generated | 538 | **646** |
| Caught, meaning a test failed | 359 | **483** |
| Hung the suite, which is the suite noticing | 4 | **13** |
| Could not be built, so not a mutant at all | 77 | **107** |
| **Survived** | 98 | **43** |
| Score, caught over viable, counting a hang as caught | 78.7% | **92.0%** |

It took 75 minutes on `ubuntu-latest` with four jobs, well inside the three hours the job
is given.

**What the two faults were**, both found in stage three and fixed before this run:

| | What was wrong | What it is now |
| --- | --- | --- |
| What judges a mutant | The mutated crate's own tests, which is `cargo mutants`' default. A line of `solar-core` checked by a `solar-cli` test read as a survivor. | The whole workspace suite, `test_workspace = true` |
| Where each mutant builds | One shared `CARGO_TARGET_DIR`, so a mutant could be judged by an artefact built from a different mutant | Each copied tree's own |

**What was done with the 43.** Twenty-nine were killed by tests of observable behaviour:
the boundary of `brief` on both of its branches, the last resort envelope that is built
without serde and has to escape by hand, the hint that offers a lower case name only when
that would help, what `is_success` and `protocol_name` answer, every condition that makes
a string one of the timestamps a replay ignores, a timestamp inside an array, each thing
serde can say and the reason it becomes, a pointer into a list against a member whose name
is a number, a type written as a list, the hint for an API that takes no parameters at
all, two definitions of one name in the manifest, what counts as being before the first
stable release, and the level a human log line names.

Two were dead code and are gone rather than tested: `logging::set_format` and
`logging::debug` had no callers at all, which is why nothing noticed them changing.

**Eleven cannot be judged by this job, and saying so is the honest answer.** They sit
inside `cfg` blocks for another operating system: the nine shapes `RtlGetVersion` could
return, the comparison that reads its status, and the fallback `read_here` for a system
that is neither Linux, macOS nor Windows. Linux never compiles those lines, so mutating
them changes nothing there. What was possible was done: the rule for reading a
`SystemVersion.plist` and the rule for turning three numbers into a Windows release are
now compiled and tested on every system, and only the foreign call itself is conditional.
A test that runs on Windows pins what that call may return.

One more is left standing and is worth naming: `logging::error` is reached only when a
handler panics, and no API that ships panics, so nothing in the suite can reach it. It is
not equivalent, and excluding it would be a lie about why it survives.

**The ceiling is 43**, the number measured, and the weekly job fails at 44. It only ever
goes down.

**What closes the rest, decided by the architect on 27 September 2026, for stage five:**
the weekly mutation job runs on `ubuntu-latest`, `windows-latest` and `macos-latest`, in
parallel, so that the eleven platform-specific mutants are judged on the system that
compiles them. This repository is public, so the standard runners cost nothing, and three
jobs in parallel take the wall clock of one.

It is scheduled rather than done: this stage measured the score and killed what it could,
and changing the shape of the job belongs with the stage that will read its three
reports.

## What ZENITH asked for, and what it got

ZENITH wrote three things down in the first section of its `docs/OPEN_QUESTIONS.md`, each
with what would close it. All three are closed, and each was built to what ZENITH wrote
rather than to what seemed reasonable here.

| What ZENITH wrote | What it got |
| ------------------ | ----------- |
| *"A member of the manifest, at its root, that declares the batch support and its limit, for example `"batch": {"max_elements": 64}`, with a minor bump of `schema_version`."* | `capabilities` at the root, with that member and three more: cancellation and the method that performs it, that notifications are refused, and every limit a caller must respect. `schema_version` 2.0.0 to **2.1.0**, minor, as asked. |
| *"An API such as `solar.set_log_level`, or a documented statement that the level is fixed for the life of a session."* | The API. ZENITH's workaround was to start SOLAR at `trace` and filter in the interface, paying for output it throws away; now it can ask. |
| *"A section of SOLAR's contract, or a document beside it, that specifies the recording, with a version."* | Contract section 12 and [`docs/RECORDING.md`](docs/RECORDING.md), with a JSON Schema for one line. A recording written by hand in that shape replays, which is the test that matters. |

**One thing was fixed because of this work rather than for it.** `cargo xtask compat`
classified every change of `schema_version` as breaking, which would have made adding a
member to the manifest impossible. Record 22 says a consumer refuses a manifest whose
major differs and reads one whose minor moved; compat now reads the major, and two tests
pin it in both directions.

## Linux, by hand rather than in CI

Nothing in this repository had run on Linux outside GitHub's runners. It has now, on this
machine, in **WSL with AlmaLinux 9.8 (Olive Jaguar)**, with the repository cloned into the
WSL file system at `~/solar` rather than under `/mnt/c`, as the brief asked.

Rust was not installed there. `rustup` put it in the user's home directory, with no
`sudo` and no system package:

```bash no-run
# no-run: it installs a toolchain, which CI already has
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path --default-toolchain none
```

`--default-toolchain none` leaves the choice to `rust-toolchain.toml`, which named 1.97.1
and was downloaded by the first `cargo` command inside the repository. **A C compiler was
already there**, `gcc` and `cc` from the distribution, so nothing needed installing and
nothing needed `sudo`.

| What | Result |
| ---- | ------ |
| `cargo build --release --locked` | **passed**, 26.3 s for the whole workspace |
| `cargo nextest run --workspace --locked` | **passed, 329 tests, 0 failures, 2.9 s**, including the cancellation races, the cap on abandoned handlers and the allocation ceiling |
| `solar call solar.ping` | **passed** |
| `solar call system.info` | **passed**, and it names the system correctly: `os` is `linux`, `os_name` is `AlmaLinux`, `os_release` is `9.8`, with no warnings, so `/etc/os-release` was read as the contract says |
| `cargo xtask ci`, the whole pipeline | **every step passed**, once the four tools it names were installed. The one failure worth recording is that `formatting` failed here and not on Windows, because a change had been made on Windows without `cargo fmt` after it: the pipeline caught on Linux what the laptop had not been asked |
| `cargo xtask leak-check` | **passed**: the 9 486 144 byte release binary is clean of all four needles |

The steps that needed a tool which was not installed said exactly which one and the line
to install it with, which is the behaviour they are supposed to have. They were then
installed into the same home directory, with no `sudo`:

```bash no-run
# no-run: it installs development tools, which CI installs its own way
cargo install typos-cli taplo-cli cargo-deny cargo-llvm-cov --locked
curl -sSLf https://get.nexte.st/latest/linux | tar zxf - -C "$HOME/.cargo/bin"
```

**Nothing needed `sudo` and nothing needed a system package**, so the brief's instruction
to stop and write the command down instead of installing it never had to be used. One
thing is worth recording for anyone repeating this: installing `rustup` with
`--default-toolchain none` means `cargo install` has no toolchain to use **outside** a
directory with a `rust-toolchain.toml`. Run it from inside the repository, or run
`rustup default 1.97.1` first.

## What the checks found, each one a real defect

Eleven so far, each found by the thing built to find it rather than by reading. The first
seven are stage two, kept here because they are the argument for the checks; then three
from stage three, and one from stage four.

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
7. **The mutation configuration was never being read.** `cargo-mutants` looks for
   `.cargo/mutants.toml`, not `mutants.toml`, so the weekly run mutated the developer
   tools as well and reported 218 survivors that were mostly in `xtask`. Found by
   reading the first weekly run instead of trusting it.
8. **The first load measurement measured the measuring.** `cargo xtask load` reported
   718 requests per second while also reporting a median latency of 32 microseconds, two
   figures that cannot both be true. Found by reading them together. Sampling the
   server's resident set costs a whole PowerShell process, about 0.7 s, and twenty
   samples were most of the wall clock. The sampling is now rare, timed, and taken out of
   the figure it would otherwise dominate; the real throughput is 28 814 per second.
9. **The mutation suite was measuring the wrong tree.** `cargo xtask mutants` gave
   `cargo mutants` a shared `CARGO_TARGET_DIR`, inherited from the rule that keeps a
   nested build from relinking the running xtask binary. `cargo mutants` never builds in
   the real tree: it copies the sources once per job. One shared target directory made
   cargo reuse a test binary compiled in another copy, and a test binary carries the
   `CARGO_MANIFEST_DIR` of the tree that compiled it, so `tests/docs.rs` read the
   contract from a directory that had already been deleted. The visible symptom was a
   failing baseline. The invisible one was worse: a mutant could be judged by an artefact
   built from a different mutant, so **the 98 survivors reported in stage two were not a
   measurement**. The variable is gone, with a comment saying why its absence is
   deliberate, and the score below was measured again from nothing.
10. **The testing guide invented an exit code.** It said a call with a misspelled
   parameter exits 3; `INVALID_ARGUMENT` exits 2. Found by `cargo xtask doc-run` on the
   first run of the guide, before anybody read it. This is exactly the defect the
   documentation runner exists for, in the document written to be trusted by two people
   who cannot check it themselves.

## What runs when

| When | What |
| ---- | ---- |
| Every push and pull request | The fourteen steps of the one command, on `ubuntu-latest`, `windows-latest` and `macos-latest`, plus a documentation job and a coverage job. On this machine the whole pipeline takes **305 s** on Windows and **126 s** on Linux, both green on 27 September 2026 |
| Every pull request | The compatibility check and the changelog check, both against the base branch |
| Weekly, Monday 06:00 UTC | The latest stable compiler as an early warning; the declared minimum, proving `rust-version`; four fuzzing targets for three minutes each; the full mutation suite |

11. **Two changelog entries were written into thin air.** The entry for
   `solar.set_log_level`, and the one for the manifest capabilities that merged before it,
   were added by replacing a line that existed on `main` but not yet on the branch being
   edited. The replacement matched nothing and changed nothing, silently, twice. Found by
   `cargo xtask changelog` on the second one; the first had slipped through because that
   check asks whether the file moved, and its `### Changed` entry had. **A check that asks
   whether a document moved cannot ask whether the right part of it moved**, which is why
   the rest of the rule is on the pull request template rather than in a command.

## What Chrysthofer has to turn on

**Everything below was turned on, on 27 September 2026.** It is kept here as the record of
what the repository requires, because a setting nobody wrote down is a setting nobody can
restore.

The branch ruleset on `main`:

| Setting | Value |
| ------- | ----- |
| Require a pull request before merging | on |
| Required approvals | 0, so that the architect can merge his own work without a second account |
| Require status checks to pass | on: `ubuntu-latest`, `windows-latest`, `macos-latest`, `documentation` and `coverage` |
| Block force pushes | on |
| Restrict deletions | on |

And, at **Settings, Code security**: Dependabot alerts, and secret scanning with push
protection.

**What that changed about the work.** Every change in stage four reached `main` through a
pull request, merged when the five checks were green: six of them, numbered 1 to 6. Two
were Dependabot's.

It also caught two things a direct push would not have. A branch that was behind `main`
compared its manifest against the wrong base and reported a member as removed, which is
exactly what the check is for; rebasing fixed it. And the changelog check failed on a
branch where the local pipeline had passed, because the local one compares against `main`
as it was when the branch started.

## Every open question, answered

**All twenty-seven were confirmed by the architect on 27 September 2026**, each as it was
proposed, and each is now a record in [`docs/adr/`](docs/adr/), numbered 12 to 38.
[`docs/OPEN_QUESTIONS.md`](docs/OPEN_QUESTIONS.md) holds nothing open.

The five that were worth reading carefully, and what was decided:

| The question | The decision | Record |
| ------------ | ------------ | ------ |
| The minimum Rust version | Stays at **1.97** until somebody is actually blocked. A weekly job proves the declared minimum, so the claim is checked rather than asserted. | [24](docs/adr/0024-the-minimum-rust-version-is-1-97.md) |
| `unsafe`, `deny` or `forbid` | Stays at **`deny`**, with the single documented exception for `RtlGetVersion`. A dependency such as `windows-sys` would trade one argued exception for somebody else's unsafe code. | [20](docs/adr/0020-unsafe-is-denied-not-forbidden.md) |
| The superseded entry for a sequential session | The entry **stays marked as superseded**, and the part still worth deciding, that calls run one at a time in the order they arrived, is now a record of its own. | [18](docs/adr/0018-calls-run-one-at-a-time-in-order.md) |
| `loom` | **Not adopted**, with the revisit trigger rewritten: when a transition of the session state happens outside the one lock, or when the two threads talk through atomics rather than through it. | [38](docs/adr/0038-loom-is-not-adopted.md) |
| Running external programs | **No API runs one.** Third-party tools come in a later stage, after this version reaches the laboratory. The vocabulary they need is already in the contract. | [31](docs/adr/0031-no-api-runs-an-external-program.md) |

The other twenty-two were confirmed as they stood: the order the envelope is checked in,
the status a malformed method name gets, the 200 byte cut on `received`, the registry that
answers rather than refusing to start, the panic hook that forwards, the oversized line
that is discarded to the newline, the release of the operating system and how it is read,
`cpu_count` that may be null, the semantic `schema_version`, the edit distance that offers
suggestions, how an example declares its comparison, the lints integration tests lift, the
exit code of a misuse, the two forms of `solar manifest`, where the mark is printed and how
its drawing is chosen, the three platforms CI runs on, the computed remap flags, the
documentation runner and its audited exceptions, British English, pedantic lints allowed at
the site, and the nested target directory of the one command.

## What is left

**For the architect.** The **thirty-eight** decision records in [`docs/adr/`](docs/adr/)
are the settled ones, and nothing is open.

**Not started, and out of scope by instruction.** Anything that runs an external program:
`tools.detect`, the table of known tools and the process runner, which are in the history
at `1029db4` and its parent. Record 31 says they come in a later stage, after this version
reaches the laboratory. Releases, distribution, installers and signing.

**Known gaps, in the order they will start to hurt.**

- The survivors listed above, each one a line nothing checks. The ceiling goes down as
  they fall and never up. Eleven of them wait on the three-system mutation job that the
  architect approved on 27 September 2026 for stage five.
- `system.info` reporting the release of macOS has never run on a Mac outside GitHub's
  runners. That is what [`docs/TESTING_BY_HAND.md`](docs/TESTING_BY_HAND.md) is for.
- The third thing ZENITH asked for is closed in SOLAR and open in ZENITH: the format is
  specified and validated here, and ZENITH still exports in its own format, specified in
  its `docs/DESIGN.md` section 12. Nothing here can close that half.
- A batch runs its elements one at a time, which is the contract. If a client ever needs
  a batch of sixty-four slow calls, sixty-four times the slowest call is the wait, and
  the answer is not concurrency but a smaller batch.
- Nothing measures what happens when two clients drive two sessions of the same binary at
  once, because nothing runs two sessions of the same binary at once. `solar serve` is
  one process per client.
