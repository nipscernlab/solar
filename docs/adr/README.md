# Decision records

One file per decision, in the [MADR](https://adr.github.io/madr/) format. A record says
what was decided, what it rules out, and why, in enough detail that somebody who was not
there can disagree with it on the merits.

A decision arrives in [`../OPEN_QUESTIONS.md`](../OPEN_QUESTIONS.md), which is the list of
things settled without asking and still open to being overruled. When the architect
confirms one, it moves here and leaves that file. So:

- **`OPEN_QUESTIONS.md`** is what is still open.
- **`docs/adr/`** is what is settled.

A record is never edited to say something different. A decision that is reversed gets a
new record that supersedes the old one, and the old one is marked as superseded, because
the reasoning that was once persuasive is part of the history.

**Records 12 to 38 were confirmed together**, on 27 September 2026: they are the
twenty-seven decisions that had been made alone and written in `OPEN_QUESTIONS.md` while
the first three stages were built. Their reasoning is the reasoning that was written at
the time; what the architect added is that they are settled.

| Record | Decision | Status |
| ------ | -------- | ------ |
| [0001](0001-json-rpc-over-stdio.md) | JSON-RPC 2.0 over standard input and output, one message per line | Accepted |
| [0002](0002-every-call-gets-a-response.md) | Notifications are refused, because every call gets a response | Accepted |
| [0003](0003-params-must-be-an-object.md) | `params`, when present, must be an object, and `null` is refused | Accepted |
| [0004](0004-one-response-shape-per-api.md) | One API, one response shape | Accepted |
| [0005](0005-error-data-has-four-members.md) | `error.data` has exactly four members | Accepted |
| [0006](0006-names-derive-from-the-method.md) | The file, the module and the struct of an API follow from its name | Accepted |
| [0007](0007-experimental-until-one-point-zero.md) | Every API is experimental until SOLAR reaches 1.0.0 | Accepted |
| [0008](0008-abandon-a-handler-that-overruns.md) | A handler that overruns is abandoned, not killed | Accepted |
| [0009](0009-one-reusable-worker-thread.md) | One reusable worker thread per dispatching thread | Accepted |
| [0010](0010-batches-answer-in-order.md) | Batches are answered in the order they were sent | Accepted |
| [0011](0011-cancelling-is-an-ordinary-call.md) | Cancelling is an ordinary call, and every request still gets exactly one response | Accepted |
| [0012](0012-envelope-is-checked-in-a-fixed-order.md) | The envelope is checked in a fixed order | Accepted |
| [0013](0013-a-malformed-method-name-is-invalid-value.md) | A method name that breaks the naming rule is `INVALID_VALUE`, not `NOT_FOUND` | Accepted |
| [0014](0014-received-is-cut-at-two-hundred-bytes.md) | A value echoed in `received` is cut at 200 bytes | Accepted |
| [0015](0015-a-broken-registry-still-answers.md) | A registry that does not build answers every call with the same error | Accepted |
| [0016](0016-the-panic-hook-is-global-and-forwards.md) | The panic hook is global and installed by the dispatcher | Accepted |
| [0017](0017-an-oversized-line-is-discarded-to-the-newline.md) | An oversized line is discarded up to the next newline | Accepted |
| [0018](0018-calls-run-one-at-a-time-in-order.md) | Calls run one at a time, in the order they arrived | Accepted |
| [0019](0019-system-info-reports-the-release.md) | `system.info` reports the release of the operating system | Accepted |
| [0020](0020-unsafe-is-denied-not-forbidden.md) | Unsafe code is denied rather than forbidden, for exactly one function | Accepted |
| [0021](0021-cpu-count-may-be-null.md) | `cpu_count` may be `null` | Accepted |
| [0022](0022-the-manifest-layout-version-is-semantic.md) | The manifest layout version is semantic | Accepted |
| [0023](0023-suggestions-come-from-the-edit-distance.md) | Suggestions come from the Levenshtein distance, at most three of them | Accepted |
| [0024](0024-the-minimum-rust-version-is-1-97.md) | The declared minimum Rust version is 1.97 | Accepted |
| [0025](0025-an-example-declares-how-it-is-compared.md) | An example declares how it is compared, and `$any` stands for what cannot be reproduced | Accepted |
| [0026](0026-integration-tests-lift-the-panic-lints.md) | Integration tests lift the lints that forbid panicking | Accepted |
| [0027](0027-a-misuse-of-the-command-line-exits-2.md) | A misuse of the command line exits 2, the same as `INVALID_ARGUMENT` | Accepted |
| [0028](0028-solar-manifest-indents-on-a-terminal.md) | `solar manifest` indents on a terminal and prints one line into a pipe | Accepted |
| [0029](0029-the-mark-is-printed-by-solar-version-only.md) | The mark is printed by `solar version` and nowhere else | Accepted |
| [0030](0030-the-ascii-mark-is-a-switch-not-a-guess.md) | The ASCII drawing of the mark is a switch, not a guess | Accepted |
| [0031](0031-no-api-runs-an-external-program.md) | No API runs an external program | Accepted |
| [0032](0032-ci-runs-on-three-platforms.md) | Continuous integration runs on Linux, Windows and macOS | Accepted |
| [0033](0033-local-paths-are-stripped-with-computed-flags.md) | Local paths are stripped with computed remap flags, not committed ones | Accepted |
| [0034](0034-the-documentation-runs-and-no-run-is-audited.md) | The documentation runner executes blocks, and `no-run` is the audited exception | Accepted |
| [0035](0035-the-repository-is-written-in-british-english.md) | The repository is written in British English | Accepted |
| [0036](0036-pedantic-lints-are-allowed-at-the-site.md) | Pedantic lints are fixed or allowed at the site, never at the workspace | Accepted |
| [0037](0037-ci-builds-nested-commands-in-their-own-target.md) | `cargo xtask ci` builds its nested commands in a target directory of their own | Accepted |
| [0038](0038-loom-is-not-adopted.md) | `loom` is not adopted, and here is what would change that | Accepted |
