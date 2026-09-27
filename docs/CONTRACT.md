# The SOLAR contract

**Status:** normative. **Protocol:** `solar/1`. **Applies to:** SOLAR `0.1.0`.

This document defines the wire protocol of SOLAR, the central API of the Constellation
project. It is normative: where this document and the implementation disagree, the
implementation is wrong. Every rule stated here is enforced by a test, and the tests that
enforce it are named in [Conformance](#11-conformance).

The key words MUST, MUST NOT, SHOULD and MAY are used in the sense of RFC 2119.

## 1. Scope and audience

SOLAR is the single boundary of the Constellation project. Every interface, the `solar`
command line interface, the AURORA IDE and artificial intelligence agents, speaks to SOLAR
and to nothing else. Everything behind SOLAR is an implementation detail that callers MUST
NOT depend on.

Two properties drive every decision in this document.

1. **Every call gets a response.** Success or failure, a response is always produced. No
   code path ends in silence, in an unhandled panic or in a partial message.
2. **A response carries as much context as it can.** Callers include agents, which cannot
   ask a colleague what an error meant. Every error states what was expected, what arrived,
   where, why, and what to do next. Every response carries execution metadata.

## 2. Transport

- **Framing.** One JSON message per line, `\n` terminated, known as NDJSON. This is the
  framing used by LSP servers over stdio and by MCP. There is no HTTP, no port and no gRPC.
- **Encoding.** UTF-8. A line that is not valid UTF-8 is a parse error.
- **Line endings.** A trailing `\r` before the `\n` is accepted and stripped, so a caller
  writing CRLF on Windows interoperates. SOLAR itself always writes `\n`.
- **Empty lines.** A line that is empty or contains only whitespace is ignored and produces
  no response. It is not a message.
- **Size limit.** A single request line MUST NOT exceed **16 MiB** (16777216 bytes,
  excluding the terminating newline). A longer line is refused with `RESOURCE_EXHAUSTED` /
  `MESSAGE_TOO_LARGE` and `id: null`. SOLAR stops buffering at the limit; it then discards
  bytes up to the next newline, without keeping them, so that the stream stays aligned and
  the following message is read normally.
- **Channel discipline.** Standard output carries protocol and nothing else: one response
  per line, no banner, no progress, no logging. Diagnostics go to standard error and are
  controlled by the `SOLAR_LOG` environment variable, whose values are `off`, `error`,
  `warn`, `info`, `debug` and `trace`; the default is `off`. An API handler MUST NOT write
  to standard output.
- **Ordering.** This version is strictly sequential: one request is read, dispatched and
  answered before the next is read. Responses therefore appear in request order.

## 3. The request

```json
{"jsonrpc": "2.0", "id": 1, "method": "solar.ping", "params": {"message": "hi"}}
```

| Member    | Requirement                                                                  |
| --------- | ---------------------------------------------------------------------------- |
| `jsonrpc` | MUST be present and MUST be exactly the string `"2.0"`.                       |
| `id`      | MUST be present and MUST be a number or a string. See the deviation below.    |
| `method`  | MUST be present and MUST be a string matching the naming rule of section 4.   |
| `params`  | MAY be absent. When present it MUST be an object; absent means `{}`.          |

Unknown members at the top level of the request are ignored, which is what JSON-RPC 2.0
requires. Unknown members inside `params` are **not** ignored: every API declares its
parameters with `deny_unknown_fields`, so a misspelled parameter is an error instead of a
silent no-op.

### 3.1 The two deliberate deviations from JSON-RPC 2.0

1. **Notifications are not accepted.** A message without `id` is a notification in
   JSON-RPC 2.0, and a server must not answer it. SOLAR refuses that rule, because every
   call gets a response. A message without `id`, or with `id: null`, receives an error with
   code `-32600`, status `INVALID_ARGUMENT`, reason `NOTIFICATION_NOT_SUPPORTED` and
   `id: null`.
2. **Batches are not accepted in `solar/1`.** A top level JSON array receives a single
   error with status `UNIMPLEMENTED`, reason `BATCH_NOT_SUPPORTED` and `id: null`. A batch
   is valid JSON-RPC that SOLAR does not implement yet, which is why it is `UNIMPLEMENTED`
   and not a malformed request.

## 4. Method names

A method name is `namespace.verb_noun`: lowercase ASCII, words inside a segment joined by
underscores, segments joined by dots. The normative regular expression is

```
^[a-z]+(\.[a-z]+(_[a-z]+)*)+$
```

Names are in English, and they are read by people as well as by machines: `tools.detect`,
`solar.describe`, `system.info`. A name is unique across the whole registry and is never
reused for a different meaning.

## 5. The success response

```json
{"jsonrpc": "2.0", "id": 1, "result": {
  "data": {"pong": true, "echo": "hi", "received_at": "2026-09-26T21:41:03.123456Z"},
  "meta": {"...": "see section 7"},
  "warnings": []
}}
```

`result` always has exactly three members.

- `data` is the output of the API, whose shape is the `output_schema` published in the
  manifest. It is always present and is always a JSON object.
- `meta` is the execution metadata of section 7. Always present.
- `warnings` is an array, possibly empty, never absent. Each entry is
  `{"code": "UPPERCASE_CONSTANT", "message": "one sentence"}`. A warning reports something
  the caller should know that did not prevent the call from succeeding. Warnings never
  change the shape of `data`.

## 6. The error response

```json
{"jsonrpc": "2.0", "id": 1, "error": {
  "code": -32602,
  "message": "Invalid params for solar.describe: missing field `api`.",
  "data": {
    "status": "INVALID_ARGUMENT",
    "reason": "MISSING_FIELD",
    "details": [{"field": "/api", "expected": "string", "received": null,
                 "hint": "Pass the method name, e.g. {\"api\": \"solar.ping\"}.",
                 "docs": "docs/ERRORS.md#invalid_argument"}],
    "meta": {"...": "see section 7"}
  }
}}
```

- `code` is the JSON-RPC integer code, fixed by the table in section 6.1.
- `message` is one sentence of English prose, capitalised, ending in a full stop. It names
  the method when a method is known. It is written for a human reading a terminal.
- `data.status` is one of the eleven canonical status codes of section 6.1. These are the
  codes used by Google and by gRPC, so that a caller that already knows them needs to learn
  nothing new.
- `data.reason` is a finer grained uppercase constant that identifies the exact failure
  within a status. The full catalogue is [docs/ERRORS.md](ERRORS.md). A reason belongs to
  exactly one status.
- `data.details` is an array, in the spirit of RFC 9457 problem details. It is never empty:
  an error that has nothing specific to say still carries one entry pointing at the
  documentation. Every entry has the same five members, and every member is always present,
  `null` where it does not apply.

| Member     | Type             | Meaning                                                        |
| ---------- | ---------------- | -------------------------------------------------------------- |
| `field`    | string or null   | Where the problem is. A JSON pointer into `params` when the problem is a parameter, for example `/tools/0`; otherwise a short name such as `method`. |
| `expected` | string or null   | What that position should have held. For a suggestion it holds the suggested value itself. |
| `received` | any JSON value   | What arrived there, `null` when nothing did.                    |
| `hint`     | string or null   | What to do about it, in the imperative, with an example when one helps. |
| `docs`     | string           | A link into the error catalogue, always `docs/ERRORS.md#<status in lowercase>`. |

- `data.meta` is the same metadata block as in a success response.

`data` has exactly those four members. Nothing else is added to it in `solar/1`, so that a
caller can pattern match on the shape.

### 6.1 The status to code table

The JSON-RPC `code` is a function of the status. There are exactly two exceptions, both at
the envelope level, listed under the table.

| `status`              | `code`   | Meaning                                                      | Retriable |
| --------------------- | -------- | ------------------------------------------------------------ | --------- |
| `INVALID_ARGUMENT`    | `-32602` | The caller sent something SOLAR cannot accept.                | no        |
| `NOT_FOUND`           | `-32601` | A named thing does not exist: a method, an API, a file.       | no        |
| `ALREADY_EXISTS`      | `-32001` | Creating something that is already there.                     | no        |
| `FAILED_PRECONDITION` | `-32002` | The system is not in a state where the call can run.          | no        |
| `PERMISSION_DENIED`   | `-32003` | The operating system refused access.                          | no        |
| `RESOURCE_EXHAUSTED`  | `-32004` | A declared limit was reached.                                  | sometimes |
| `DEADLINE_EXCEEDED`   | `-32005` | The call ran past its timeout.                                 | yes       |
| `UNAVAILABLE`         | `-32006` | A dependency SOLAR needs is not available right now.           | yes       |
| `UNIMPLEMENTED`       | `-32007` | Valid, understood, not built yet.                              | no        |
| `INTERNAL`            | `-32603` | A bug in SOLAR. A panic reaches the caller as this.            | no        |
| `UNKNOWN`             | `-32099` | A failure that could not be classified. Should never be seen.  | no        |

The two envelope level exceptions, which exist because JSON-RPC 2.0 reserves those codes
for exactly these situations:

| Situation                                        | `code`   | `status`           | `reason`                    |
| ------------------------------------------------ | -------- | ------------------ | --------------------------- |
| The line is not valid JSON                        | `-32700` | `INVALID_ARGUMENT` | `PARSE_ERROR`               |
| The JSON is valid but the envelope is malformed   | `-32600` | `INVALID_ARGUMENT` | one of the envelope reasons |

The envelope reasons are `MISSING_FIELD`, `TYPE_MISMATCH`, `INVALID_VALUE` and
`NOTIFICATION_NOT_SUPPORTED`, and only when the failure is in `jsonrpc`, `id`, `method` or
in the type of `params`. Once dispatch has started, `MISSING_FIELD` and the others carry
their table code of `-32602`.

`-32000` to `-32099` is the range JSON-RPC 2.0 reserves for application errors, which is
why the statuses without a JSON-RPC counterpart live there.

### 6.2 An unknown method

An unknown method is `NOT_FOUND` / `METHOD_NOT_FOUND`. SOLAR compares the name it received
against every registered name with the Levenshtein edit distance and returns the closest
ones, at most three, each as its own detail entry whose `expected` holds the suggestion.
A name is only suggested when its distance is at most 3 and strictly smaller than its own
length. When nothing is close enough, a single detail entry points the caller at
`solar.manifest`.

## 7. `meta`

Every response, successful or not, carries the same block.

| Member          | Type             | Meaning                                                      |
| --------------- | ---------------- | ------------------------------------------------------------ |
| `request_id`    | number, string or null | The `id` that arrived, echoed unchanged. `null` when the request was too broken to have one. |
| `method`        | string or null   | The method that arrived, `null` when there was none.          |
| `api_version`   | string or null   | The semantic version of the API that ran, `null` when no API ran. |
| `solar_version` | string           | The version of the SOLAR build that answered.                 |
| `protocol`      | string           | `"solar/1"`.                                                  |
| `started_at`    | string           | When SOLAR started handling the request. RFC 3339, UTC, microsecond precision, `Z` suffix. |
| `duration_us`   | number           | Whole microseconds from the start of handling to the moment the response was assembled, measured on a monotonic clock. |
| `os`            | string           | `windows`, `linux`, `macos`, and so on: `std::env::consts::OS`. |
| `arch`          | string           | `x86_64`, `aarch64`, and so on: `std::env::consts::ARCH`.      |

`duration_us` measures SOLAR's work. It excludes the time the line spent in a pipe.

## 8. The API specification

Every API publishes a specification, and the manifest is the sum of those specifications.

| Field           | Meaning                                                                       |
| --------------- | ----------------------------------------------------------------------------- |
| `name`          | `namespace.verb_noun`, section 4.                                              |
| `version`       | The semantic version of this API, independent of the SOLAR version.            |
| `summary`       | One line, no full stop, what the API does.                                     |
| `description`   | Prose. What it is for, what it guarantees, what it does not do.                |
| `errors`        | Every `{status, reason}` pair the API may return, on top of the ones dispatch itself can produce. A pair that is not declared and is then returned is a bug. |
| `side_effects`  | A set drawn from `none`, `reads_filesystem`, `writes_filesystem`, `spawns_process`, `network`. `none` is exclusive: it cannot appear with another value. No API in `solar/1` writes, starts a process or uses the network; the vocabulary exists for the ones that will. |
| `idempotent`    | Whether calling twice with the same parameters has the same effect as calling once. |
| `stability`     | `experimental`, `stable` or `deprecated`.                                      |
| `since`         | The SOLAR version in which the API first appeared, not the API's own version.  |
| `timeout_ms`    | The wall clock budget for one call, strictly positive. Enforced by dispatch.   |
| `params_schema` | JSON Schema 2020-12, generated from the Rust parameter type.                   |
| `output_schema` | JSON Schema 2020-12, generated from the Rust output type.                      |
| `examples`      | At least one. See section 8.1.                                                 |

While `solar_version` is below `1.0.0`, every API is `experimental`. Nothing in SOLAR is
declared `stable` before the first stable release of SOLAR itself.

### 8.1 Examples are tests

An example is `{name, description, params, response, match}`, where `params` is a request
`params` object and `response` is the expected `result.data`. Examples are not decoration:
a test runs every example of every API and fails when reality disagrees.

`match` says how `response` is compared with what really came back.

- `exact`: deep equality.
- `subset`: every member named in the example must exist in the real output and must match;
  members the example does not name are ignored. Arrays must have the same length and match
  element by element.

In both modes the string `"$any"` matches any value at that position. This is how an
example pins down the shape of a timestamp, a path or a duration without pretending to know
the value. An example never states a value that the machine running the test cannot
reproduce.

## 9. Timeouts, panics and cancellation

- Dispatch runs every handler on a worker thread and waits for `timeout_ms`.
- A handler that panics produces `INTERNAL` / `HANDLER_PANIC`, with the panic message and
  the source location in `details`. The process survives, and the session continues.
- A handler that runs past its budget produces `DEADLINE_EXCEEDED` / `HANDLER_TIMEOUT`.
  The response goes out immediately. **The worker thread is abandoned, not killed**: Rust
  has no safe way to kill a thread. An abandoned handler keeps running until it finishes,
  and its result is discarded. Handlers are therefore written so that their own internal
  budgets are shorter than `timeout_ms`.
- There is no cancellation message in `solar/1`.

## 10. Versions

- **Protocol**: `solar/1`. The number changes only when an existing message shape changes in
  a way that breaks a caller. Adding an API never changes it.
- **API**: each API carries its own semantic version. A breaking change to its parameters or
  its output is a major bump of that API alone.
- **Manifest**: `schema_version`, semantic, currently `1.0.0`. A consumer MUST reject a
  manifest whose major differs from the one it was written against.
- **SOLAR**: the version of the build, reported in `meta.solar_version`.

## 11. Conformance

Every rule above is enforced mechanically. A change that breaks one of them fails a test
instead of waiting for a human reviewer.

| Rule | Test |
| ---- | ---- |
| Name matches the naming rule, and is unique | `solar-apis/tests/contract.rs::names_follow_the_naming_rule`, `::names_are_unique` |
| Summary, description, error list and examples are present | `::specs_are_complete` |
| Every declared status is canonical and every reason is documented | `::declared_errors_are_canonical_and_documented` |
| Parameters reject unknown fields | `::params_schemas_reject_unknown_fields` |
| Schemas are JSON Schema 2020-12 | `::schemas_are_2020_12` |
| Every example validates against the parameter schema | `::examples_validate_against_params_schema` |
| Every example really runs, and its output matches both the output schema and the example | `::examples_match_real_execution` |
| `timeout_ms` is strictly positive, `since` and `version` are semantic versions | `::specs_are_complete` |
| Below SOLAR 1.0.0 every API is experimental | `::everything_is_experimental_before_1_0` |
| The versioned manifest is exactly what the generator produces | `::manifest_is_not_stale` |
| The status to code table and the reason catalogue agree with `docs/ERRORS.md` | `solar-core/tests/docs.rs` |
| Standard output carries protocol only, even with `SOLAR_LOG=trace` | `solar-cli/tests/cli.rs::logging_never_touches_stdout` |

## 12. Exit codes of the `solar` binary

`solar call` prints the whole response envelope on standard output and then exits with a
code derived from the response, so that a shell script never has to parse JSON to know what
happened.

| Exit code | Meaning                                               |
| --------- | ----------------------------------------------------- |
| `0`       | Success.                                              |
| `2`       | `INVALID_ARGUMENT`, and also a misuse of the command line itself. |
| `3`       | `NOT_FOUND`                                           |
| `4`       | `ALREADY_EXISTS`                                      |
| `5`       | `FAILED_PRECONDITION`                                 |
| `6`       | `PERMISSION_DENIED`                                   |
| `7`       | `RESOURCE_EXHAUSTED`                                  |
| `8`       | `DEADLINE_EXCEEDED`                                   |
| `9`       | `UNAVAILABLE`                                         |
| `10`      | `UNIMPLEMENTED`                                       |
| `11`      | `INTERNAL`                                            |
| `12`      | `UNKNOWN`                                             |
| `70`      | SOLAR could not even produce a response, for example because standard output was closed. |

`solar serve --stdio` exits `0` when the input ends cleanly, whatever the individual calls
returned, because the session itself succeeded.
