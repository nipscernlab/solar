# Status

**Stage:** the core of the API. **Version:** 0.1.0. **Protocol:** `solar/1`.
**Written on:** 27 September 2026. **Machine it was built and measured on:** Windows 11
Home Single Language 26200, Intel Core i7-13620H, `rustc 1.97.1`.

## What is ready

**The contract.** `docs/CONTRACT.md` is normative and complete for `solar/1`: the NDJSON
framing, the envelope with its two deliberate deviations, the fixed status to code table,
the shape of `meta`, how an API is specified, how an example is matched, timeouts and
panics, the version scheme, the conformance table and the exit codes.
`docs/ERRORS.md` is the closed catalogue of eleven statuses, sixteen reasons and one
warning code.

**`solar-core`.** The envelope, parsed by hand so that a malformed request gets an error
worth reading. The status and reason catalogues, with the status derived from the reason
so the two cannot disagree. The `Api` trait and `ApiSpec`. A registry that checks every
rule of the template and reports all of them at once instead of the first. Dispatch, which
validates parameters, runs each handler on a reusable worker thread, turns a panic into
`INTERNAL` and an overrun budget into `DEADLINE_EXCEEDED`, and assembles `meta`. The NDJSON
session loop with the 16 MiB line limit. The manifest generator. RFC 3339 timestamps,
Levenshtein suggestions and the name rule, all without a dependency.

**Five APIs.** `solar.ping`, `solar.version`, `solar.manifest`, `solar.describe` and
`system.info`. Each one declares its errors, its side effects, its idempotency, its
timeout and its examples, and each example is replayed against a real execution by the
test suite.

**The `solar` binary.** `call`, `serve --stdio`, `list`, `describe`, `manifest` and
`version`, with an exit code per status and the mark from `docs/brand` on `solar version`
when it is looking at a terminal.

**`cargo xtask`.** `manifest` regenerates the versioned manifest, `manifest --check` says
whether it is stale, and `new-api <name>` writes a new API from the template and registers
it, both lines in alphabetical order. The generated API compiles and passes the whole
suite before anyone touches it. This was verified end to end: a throwaway
`build.run_target` was generated, passed every contract test, tripped the manifest drift
check exactly as it should, and was removed.

**163 tests**, of which the ones that matter most are the ones that police the template:
the naming rule, uniqueness, complete specifications, canonical and documented errors,
parameter schemas that refuse unknown members, JSON Schema 2020-12, examples that validate
against their own schemas and match reality, the manifest not being stale, the error
catalogue agreeing with the code, and standard output staying protocol only with
`SOLAR_LOG=trace`. A handler that panics and one that hangs are both tested end to end,
including that the session still works afterwards.

## What was measured

Every figure is a criterion median on the machine named above, in the `bench` profile,
which inherits `release` with fat LTO and one codegen unit.

| What | Median |
| ---- | ------ |
| A whole run of `solar call solar.ping` | 5.64 ms |
| One round trip in `solar serve --stdio` | 72.3 µs |
| Dispatch of `solar.ping` inside the process | 7.59 µs |
| Dispatch of an unknown method, with suggestions | 3.78 µs |
| Dispatch of an unknown parameter | 3.35 µs |
| A line that is not JSON | 1.65 µs |
| Parsing the envelope alone | 1.39 µs |

The release binary is 1.38 MiB, 1451520 bytes, on this machine.

One measurement changed the design. Starting a thread per call cost about 70 µs of the
77 µs a ping took. Reusing one worker thread per dispatching thread brought the same call
to 7.59 µs, and a call that overruns still costs a fresh thread, because the one it
abandoned can never be trusted again.

## What was decided without asking

All of it is in `docs/OPEN_QUESTIONS.md`, with the reasoning and with what would make each
one worth revisiting. The ones most worth a second opinion:

1. **`params: null` is refused.** Strict today, and compatible to loosen later.
2. **`solar.manifest` with `api` keeps the shape of the whole document**, and
   `solar.describe` is what returns a bare entry.
3. **`error.data` has exactly four members.** A `retriable` flag was considered and
   dropped; retriability is documented per status instead.
4. **The struct, module and file of an API follow from its method name with no
   exceptions**, which is why `solar.ping` is `SolarPing` rather than `Ping`.
5. **Every API is `experimental` until SOLAR reaches 1.0.0**, enforced by a test.
6. **A handler that overruns is abandoned, not killed**, and the contract says so rather
   than pretending the work was undone.

## What is left

**For the architect to confirm.** The six decisions above, and the shape of every response
in `manifest/solar.manifest.json`, which is the whole surface in one file.

**Not started, and out of scope for this stage by instruction.** Anything that runs an
external program. `tools.detect`, the table of known tools and the process runner were
written and removed; they are in the history at `1029db4` and its parent, and the
vocabulary they need is still in the contract. When they come back, `docs/CONTRACT.md`
needs its section on running external programs again, `docs/ERRORS.md` needs the reasons
for a program that cannot be started, and `system.info` can finally report the release of
the operating system.

**Known gaps.**

- Continuous integration is written and has never run. The first push proves it, or does
  not.
- The manifest is 1152 lines for five APIs, because every JSON Schema is inlined in full.
  It will not stay readable at fifty. A shared `$defs` section is the obvious answer and is
  a change to `schema_version`.
- There is no cancellation in `solar/1`, and no way for a caller to say it has stopped
  caring about a call it is waiting on.
- Batches are refused. If AURORA turns out to make many small calls in a burst, that is
  the first place to look, and the session round trip of 72 µs is the number to beat.
- Nothing measures a call under load: every figure above is one call at a time on an idle
  machine.
