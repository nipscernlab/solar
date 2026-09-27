<!-- The mark and its rules live in docs/brand. -->
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/brand/svg/lockup-dark.svg">
  <img alt="SOLAR" src="docs/brand/svg/lockup-light.svg" height="72">
</picture>

**The central API of the Constellation project, from NIPS-CERN.**

Constellation is everything around the SAPHO processor: SOLAR, SAPHO itself, the bundle of
open source tools, and the documentation. SOLAR is the single boundary. Every interface,
the `solar` command line interface, the AURORA IDE, and artificial intelligence agents,
talks to SOLAR and to nothing else; everything behind it is an implementation detail.

Three promises hold the design together.

1. **Every call gets a response.** Success or failure, always. A handler that panics
   becomes an `INTERNAL` error, a handler that runs past its budget becomes
   `DEADLINE_EXCEEDED`, and the session carries on. No path ends in silence.
2. **The more context, the better.** Every error says what was expected, what arrived,
   where, why and what to do about it. Every response carries execution metadata. The
   audience includes agents, which cannot ask a colleague what an error meant.
3. **One template, no exceptions.** Every API is declared the same way, and what deviates
   is refused by a test rather than by a reviewer.

## Try it

```bash
cargo build --release
./target/release/solar list
./target/release/solar call solar.ping '{"message":"hi"}'
```

```json
{"jsonrpc":"2.0","id":1,"result":{"data":{"echo":"hi","pong":true,"received_at":"2026-09-27T21:29:14.129284Z"},"meta":{"request_id":1,"method":"solar.ping","api_version":"1.0.0","solar_version":"0.3.0","protocol":"solar/1","started_at":"2026-09-27T21:29:14.129134Z","duration_us":156,"os":"windows","arch":"x86_64"},"warnings":[]}}
```

Every other command in this file writes `solar` bare; put `target/release` on the `PATH`,
or install it once with `cargo install --path crates/solar-cli`.

That is the whole envelope, exactly as any other caller receives it. `solar call` exists to
show it.

## The protocol in one page

JSON-RPC 2.0 over standard input and output, one JSON message per line, the framing LSP and
MCP use. No HTTP, no port, no gRPC.

```text
--> {"jsonrpc": "2.0", "id": 1, "method": "solar.ping", "params": {"message": "hi"}}
<-- {"jsonrpc": "2.0", "id": 1, "result": {"data": {...}, "meta": {...}, "warnings": []}}
```

One deviation from JSON-RPC 2.0, deliberate and in the contract: **notifications are
refused.** A message without `id` gets an error with `id: null`, because every call gets a
response.

Batches work, and are stricter than the specification allows. A line holding an array of
requests gets one line back holding the array of responses, **in the order of the
requests**, each with its own `meta`, up to 64 elements:

```text
--> [{"jsonrpc":"2.0","id":1,"method":"solar.ping"},{"jsonrpc":"2.0","id":2,"method":"solar.version"}]
<-- [{"jsonrpc":"2.0","id":1,"result":{...}},{"jsonrpc":"2.0","id":2,"result":{...}}]
```

An error carries a canonical status, the same eleven Google and gRPC use, a finer grained
reason, and details that name the position, what was expected, what arrived, and what to do:

```bash
solar call solar.ping '{"mesage":"hi"}' || echo "exit $?"
```

```json
{"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"Invalid params for solar.ping: unknown field `mesage`, expected `message`.","data":{"status":"INVALID_ARGUMENT","reason":"UNKNOWN_FIELD","details":[{"field":"/mesage","expected":"one of: message","received":"hi","hint":"There is no mesage parameter. Did you mean message? A call that works: {\"message\":\"hi\"}.","docs":"docs/ERRORS.md#invalid_argument"}],"meta":{"request_id":1,"method":"solar.ping","api_version":"1.0.0","solar_version":"0.3.0","protocol":"solar/1","started_at":"2026-09-27T21:29:14.147749Z","duration_us":78,"os":"windows","arch":"x86_64"}}}}
```

Testing it by hand, from installing the toolchain to what to send back when something
fails, is [docs/TESTING_BY_HAND.md](docs/TESTING_BY_HAND.md).

The whole contract is [docs/CONTRACT.md](docs/CONTRACT.md), and it is normative. The error
catalogue is [docs/ERRORS.md](docs/ERRORS.md).

## The seven APIs

| API | What it does |
| --- | ------------- |
| `solar.ping` | Answers immediately. The call to measure with, and the one to reach for when nothing else answers. |
| `solar.cancel` | Asks the call carrying an `id` to stop, and reports what that did. The one call a session answers the moment it is read. |
| `solar.set_log_level` | Changes how much SOLAR says about itself on standard error, while the session runs. It moves nothing else. |
| `solar.version` | The version of SOLAR, the protocol, the manifest layout, and how this binary was built. |
| `solar.manifest` | Every API this build answers to, with schemas and examples, in one document. |
| `solar.describe` | One API, laid out the same way. The call to make before making any other. |
| `system.info` | The operating system with its release, the processor and the process. |

None of them runs an external program, writes a file or touches the network, and every one
says so in its own specification.

## The command line interface

`solar` is two things at once: the debugger of the protocol, and one of the interfaces of
SAPHO.

| Command | What it prints |
| ------- | --------------- |
| `solar call <method> [params]` | The whole response envelope, one line. `--pretty` indents it. |
| `solar serve --stdio` | A session: one response per request line, until input ends. |
| `solar list` | Every API with its summary. |
| `solar describe <method>` | One API, with its parameters, failures and examples. |
| `solar manifest [--api NAME]` | The manifest, indented on a terminal and on one line in a pipe. |
| `solar version` | The versions and the build. |

```bash
solar list
```

```text
SOLAR 0.3.0 speaks solar/1, and answers to 7 APIs:

  solar.cancel        1.0.0    Asks the call with this id to stop, and says what that did
  solar.describe      1.0.0    Describes one API, with its schemas and its examples
  solar.manifest      1.2.0    Returns the manifest of every API this build answers to
  solar.ping          1.0.0    Answers immediately, to prove SOLAR is there
  solar.set_log_level  1.0.0    Changes how much SOLAR says about itself, for the rest of the process
  solar.version       1.0.0    Reports the version, the protocol and the build metadata
  system.info         1.2.0    Reports the operating system, the processor and the process

  solar describe <method>   everything about one of them
  solar call <method>       the whole envelope, as an agent sees it
```

### Quoting, shell by shell

The parameters of `solar call` are one JSON object, and every shell rewrites quotes its own
way before the binary sees them. Each line below was run in its shell on this machine, and
the documentation runner in CI runs them again, in that shell, on every push.

In `bash` and `zsh`, single quotes pass the object through untouched:

```bash
solar call solar.ping '{"message":"hi"}'
echo '{"message":"hi"}' | solar call solar.ping -
```

In Windows PowerShell, plain double quotes are removed before the program sees them, so
the object arrives as `{message:hi}` and is refused. Escape them inside single quotes, or
pipe the object to `-`, which reads the parameters from standard input, where no shell
rewrites anything. The escaped form breaks the moment the JSON holds a space; the pipe
always works:

```powershell
solar call solar.ping '{\"message\":\"hi\"}'
'{"message":"hi there"}' | solar call solar.ping -
```

In `cmd.exe`, single quotes are not quotes at all. Wrap the object in double quotes and
escape the inner ones, or pipe it:

```cmd
solar call solar.ping "{\"message\":\"hi\"}"
echo {"message":"hi"}| solar call solar.ping -
```

`solar describe <method>` prints its examples in exactly these three forms, one line per
shell, so what is on screen is always pastable.

The exit code follows the status, so a script never has to read the JSON to know what
happened: `0` for success, `2` for `INVALID_ARGUMENT`, `3` for `NOT_FOUND`, and the rest in
section 14 of the contract.

```bash
solar describe solar.pign || echo "exit $?"
```

```text
solar: No API is registered under the name solar.pign.
  NOT_FOUND / API_NOT_FOUND
  /api: expected solar.ping
    Did you mean "solar.ping"? The edit distance is 2.
  see docs/ERRORS.md#not_found
exit 3
```

Diagnostics go to standard error and only when asked: `SOLAR_LOG=trace`, or `--log trace`.
Standard output carries protocol and nothing else, and a test enforces it.
`SOLAR_LOG_FORMAT=json` turns each diagnostic line into one JSON object with the time, the
level, the request id, the method, the duration and the message, for whatever is
collecting them.

### Recording a session, and playing it back

A bug report that says "it answered wrongly" is a conversation; a bug report with a
recording is a file somebody can replay.

```bash
solar serve --stdio --record /tmp/session.ndjson < /dev/null
solar replay /tmp/session.ndjson
```

The recording is NDJSON, one entry per line, with every line in and every line out and the
time each crossed. `solar replay` sends the requests again and compares the answers,
ignoring what differs between any two runs: the volatile members of `meta`, and
timestamps wherever they appear. It exits `0` when nothing differs and `5` when something
does, and prints both answers side by side.

## Adding an API

One file and one line.

```bash no-run
# no-run: this writes a file and edits the registry; the release checklist runs it instead
cargo xtask new-api build.run_target
```

That writes `crates/solar-apis/src/build_run_target.rs` from the template and registers it,
both in alphabetical order. The generated API compiles and passes the whole suite before
you touch it, so you start from something correct. Then fill in the parameters, the output,
the description and the examples, and regenerate the manifest:

```bash
cargo xtask manifest
```

The manifest is never written by hand, and `cargo xtask manifest --check`, which CI runs,
fails when the versioned file is not what the generator produces.

The full procedure is [docs/ADDING_AN_API.md](docs/ADDING_AN_API.md).

## Performance

Measured, not assumed. Every number below comes from `cargo bench` on one machine, and
none of them is a promise about yours.

**The machine:** Windows 11 Home Single Language 26200, 13th Gen Intel Core i7-13620H,
16 hardware threads, `rustc 1.97.1`, `x86_64-pc-windows-msvc`. Benchmarks run in the
`bench` profile, which inherits `release`: fat LTO, one codegen unit, `panic = "unwind"`.
Criterion reports a confidence interval; the middle figure is quoted.

| What | Median | What it covers |
| ---- | ------ | --------------- |
| `startup/call_ping` | **5.64 ms** | A whole run of `solar call solar.ping`: create the process, build the registry, answer, exit. Process creation on Windows dominates. |
| `session/roundtrip_ping` | **72.3 µs** | One request and one response through the two pipes of `solar serve --stdio`, in a session that is already up. |
| `dispatch/ping` | **7.59 µs** | The same call inside the process: parse the line, find the API, read the parameters, run the handler on its worker thread, serialise the answer. |
| `dispatch/method_not_found` | **3.78 µs** | The same, stopping at the registry, including the edit distance search for suggestions. |
| `dispatch/unknown_parameter` | **3.35 µs** | The same, stopping at the parameters, which is the error a caller hits most. |
| `dispatch/broken_json` | **1.65 µs** | A line that is not JSON at all. |
| `parse_request` | **1.39 µs** | The envelope on its own, with no dispatch. |

Reproduce them with `cargo bench`. The two process level numbers are in
`crates/solar-cli/benches/binary.rs` and the rest in `crates/solar-apis/benches/dispatch.rs`.

### Under load

A benchmark measures one call at a time on an idle machine, which says nothing about what
a session costs. `cargo xtask load` drives a real `solar serve --stdio` through a real
pipe, sending the next request only once the last response has been read, which is what a
client does. The driver's own cost is inside every figure.

| Run | Throughput | p50 | p99 | Max | Resident memory |
| --- | ---------- | --- | --- | --- | ---------------- |
| 10 000 pings | **15 095 per second** | 61 µs | 121 µs | 713 µs | 4 380 KiB to 4 860 KiB |
| 10 000 calls, every other one an error | **19 253 per second** | 47 µs | 100 µs | 354 µs | 4 384 KiB to 4 888 KiB |
| 1 000 000 pings, `cargo xtask load --soak` | **13 418 per second** | 69 µs | 117 µs | 64 597 µs | 4 388 KiB to 4 872 KiB, ending at 4 832 KiB |

**Memory does not grow with the calls a session answers.** A million requests left the
resident set where ten thousand left it, within half a mebibyte of where it started, which
is what the limits of sections 8.4, 9.6, 9.7 and 10 of the contract are for. Errors are
answered faster than successes because an error is a smaller response and stops earlier.

**Cancellation cost half of that throughput.** The same 10 000 pings ran at 28 814 per
second with a p50 of 32 µs before the session read ahead on a thread of its own. Every
message now crosses a thread boundary, which is about 29 µs per call, and it is what buys
a session that answers `solar.cancel` while a call is running. `STATUS.md` has the
measurement that says where the time went, including the part that is not the thread.

One `solar.ping` allocates **51 blocks and 5 278 bytes**, measured with `dhat` in
`crates/solar-apis/tests/heap.rs`, which holds a ceiling so that a change that allocates
more has to say why.

There is no async runtime. A session reads on one thread and runs calls on another, and
the calls run one at a time in the order they arrived, so a client that never cancels sees
its responses in the order of its requests.

## Layout

```text
Cargo.toml                      the workspace, the shared lints, the release profile
crates/solar-core/              protocol, errors, metadata, the Api template, registry, dispatch,
                                the session and what it knows while it runs
crates/solar-apis/              the APIs, one file each, and the single registration list
crates/solar-cli/               the solar binary
xtask/                          the one command, and every check it runs
manifest/solar.manifest.json    generated, versioned, checked by a test
AGENTS.md                       the single entry point, for a person or a coding agent
docs/CONTRACT.md                the contract, normative
docs/ERRORS.md                  the error catalogue
docs/ADDING_AN_API.md           the procedure for the next API
docs/TESTING_BY_HAND.md         how to check it by hand, for somebody new to Rust
docs/RECORDING.md               the format of a recorded session, with its schema
docs/OPEN_QUESTIONS.md          what was decided without asking and is still open
docs/adr/                       the decisions that are settled, one file each
docs/STYLE.md                   how the prose, the comments and the errors are written
docs/brand/                     the mark, its colours and its terminal form
conformance/                    request and response pairs any client can replay
fuzz/                           the fuzzing targets and their corpus
```

## Building and checking

```bash no-run
# no-run: every line below is its own CI step already; cargo bench takes minutes
cargo build --release              # the solar binary
cargo nextest run --workspace       # 228 tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo xtask manifest --check       # the manifest is not stale
cargo bench                        # the numbers in the table above
```

All of it, and more, is one command:

```bash no-run
# no-run: it is the whole pipeline, and CI runs the same steps as its own
cargo xtask ci
```

That runs exactly what CI runs, in the same order and with the same flags, and a test
holds the two together. CI runs on `ubuntu-latest`, `windows-latest` and `macos-latest`,
and weekly on the latest stable compiler, on the declared minimum, on the fuzzing
targets and on the mutation suite.

[`AGENTS.md`](AGENTS.md) is the entry point for anyone, or anything, about to change
SOLAR: the one path for adding an API and the rules the machines enforce.
[`CONTRIBUTING.md`](CONTRIBUTING.md) is the rest: setting up, the rules about
dependencies, and how to record a session for a bug report.

## Who wrote this

**NIPS-CERN**, the Núcleo de Instrumentação e Processamento de Sinais, at the Faculdade de
Engenharia of the Universidade Federal de Juiz de Fora in Brazil, and at CERN in
Switzerland. The group is part of the ATLAS collaboration and works on TileCal, the tile
calorimeter.

- **Chrysthofer Arthur Amaro Afonso**, technical coordinator of NIPS-CERN, architect of
  SOLAR and of Constellation. <chrysthofer.afonso@cern.ch>
- **Prof. Luciano Manhães de Andrade Filho**, head of the group and ATLAS Team Leader at
  UFJF.

[nipscern.com](https://nipscern.com) · [github.com/nipscernlab](https://github.com/nipscernlab)
· [gitlab.com/nips-cern](https://gitlab.com/nips-cern)

## Licence

The NIPS-CERN Licence, version 1.1. The [LICENSE](LICENSE) file is the base licence of the
laboratory, copied without a character changed; a product adds an annex after it and never
subtracts from it. SOLAR has no annex.
