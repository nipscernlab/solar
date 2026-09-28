# The SOLAR contract

**Status:** normative. **Protocol:** `solar/1`. **Applies to:** SOLAR `0.3.0`.

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
- **Byte order mark.** A `U+FEFF` at the front of a line is ignored. RFC 8259 forbids
  adding one to JSON and allows a parser to ignore one, and Windows PowerShell adds one to
  the first thing it writes into the standard input of a native program. Refusing it would
  cost every PowerShell caller its first request.
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
- **The level can change while a session runs.** `SOLAR_LOG` and `--log` decide what a
  session starts at; `solar.set_log_level` changes it afterwards, for the rest of the
  process, and reports what it was and what it is. It moves nothing but standard error:
  the same call answers the same way at `off` and at `trace`, which is what makes it safe
  for an interface to turn the diagnostics up while it is waiting for something.
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
| `params`  | MAY be absent. When present it MUST be an object; absent means `{}`. `null` is not a structured value and is refused: JSON-RPC 2.0, section 4.2. |

Unknown members at the top level of the request are ignored, which is what JSON-RPC 2.0
requires. Unknown members inside `params` are **not** ignored: every API declares its
parameters with `deny_unknown_fields`, so a misspelled parameter is an error instead of a
silent no-op.

### 3.1 The one deliberate deviation from JSON-RPC 2.0

**Notifications are not accepted.** A message without `id` is a notification in
JSON-RPC 2.0, and a server must not answer it. SOLAR refuses that rule, because every call
gets a response. A message without `id`, or with `id: null`, receives an error with code
`-32600`, status `INVALID_ARGUMENT`, reason `NOTIFICATION_NOT_SUPPORTED` and `id: null`.

Batches were the second deviation until `solar/1` learned them; section 3.2 is what they
do now.

### 3.2 Batches

A line holding a JSON **array** of requests is a batch. It gets **one line back**, holding
an array of responses.

- **The order is the order of the requests.** JSON-RPC 2.0 allows a server to answer a
  batch in any order and asks the client to match by `id`; SOLAR is stricter, so that a
  batch is deterministic and a client may match by position as well.
- **Every element gets its own response**, including the ones that are invalid. An element
  without `id` gets `INVALID_ARGUMENT` / `NOTIFICATION_NOT_SUPPORTED` with `id: null`,
  inside the array, exactly as a single request would outside one.
- **Each response carries its own `meta`**, with its own `duration_us`.
- **The elements run one at a time, in the order they appear.** An element that fails does
  not stop the ones after it.

Three things are refused as a whole, with a **single** response rather than an array,
because the batch itself is what is wrong:

| What | Status and reason |
| ---- | ----------------- |
| An empty array | `INVALID_ARGUMENT` / `BATCH_EMPTY`, code `-32600`, as JSON-RPC 2.0 requires |
| More than **64** elements | `RESOURCE_EXHAUSTED` / `BATCH_TOO_LARGE` |
| Two elements with the same `id` | `INVALID_ARGUMENT` / `DUPLICATE_ID` |

Two elements with the same `id` are refused because their responses could not be told
apart: `id` is how a caller matches an answer to a question, and a batch that asks the
same question twice has no answer that means anything.

A batch is not nested: an element that is itself an array is refused like any other
element that is not a request object.

**A client that sends no batch sees no difference.** A single request object behaves
exactly as it did before.

## 4. Method names

A method name is `namespace.verb_noun`: lowercase ASCII, words inside a segment joined by
underscores, segments joined by dots. The normative regular expression is

```text
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
| `CANCELLED`           | `-32008` | The caller asked for the call to stop, and it stopped.         | no        |
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
| `max_output_bytes` | The largest response this API may produce, in bytes of serialised JSON. Enforced by dispatch. See 8.4. |
| `params_schema` | JSON Schema 2020-12, from the Rust parameter type. A fragment: see 8.1.        |
| `output_schema` | JSON Schema 2020-12, from the Rust output type. A fragment: see 8.1.           |
| `examples`      | At least one. See section 8.2.                                                 |

While `solar_version` is below `1.0.0`, every API is `experimental`. Nothing in SOLAR is
declared `stable` before the first stable release of SOLAR itself.

### 8.1 The schemas share their definitions

The manifest carries, at its root:

- `schema_dialect`, the URI of the dialect every schema in the document is written in,
  currently `https://json-schema.org/draft/2020-12/schema`;
- `$defs`, the definitions the schemas share, by name.

Every `params_schema` and `output_schema` is a **fragment** of that document, not a schema
resource of its own. It carries no `$schema` and no `$defs`, and a `$ref` of the form
`#/$defs/Name` means the entry called `Name` in the `$defs` at the root of the manifest.

This is what keeps the document readable as APIs are added: the enumeration of reasons is
written once rather than once per API that mentions it. A consumer that needs a schema on
its own puts the dialect and the definitions back around the fragment, which is what
`solar_core::manifest::standalone_schema` does and what the contract tests use to compile
every schema with a validator.

### 8.2 The manifest says what the protocol accepts

The APIs are not the whole of what a client has to know. SOLAR 0.1.0 and 0.2.0 both answer
under the protocol name `solar/1` and differ in whether a batch is accepted, so a client
that reads only the protocol name cannot tell them apart.

The manifest therefore carries **`capabilities`** at its root, and everything in it is the
value the code enforces rather than a copy of it:

```json
{
  "capabilities": {
    "batch": {"accepted": true, "max_elements": 64, "ordered": true},
    "cancellation": {"accepted": true, "method": "solar.cancel"},
    "notifications": {"accepted": false},
    "limits": {
      "max_request_bytes": 16777216,
      "max_queued_requests": 256,
      "max_queued_bytes": 67108864,
      "remembered_request_ids": 1024,
      "max_abandoned_workers": 64,
      "default_max_output_bytes": 8388608
    }
  }
}
```

| Member | What a client does with it |
| ------ | --------------------------- |
| `batch.accepted` | Whether to send an array of requests at all, section 3.2 |
| `batch.max_elements` | How many to put in one, before `BATCH_TOO_LARGE` |
| `batch.ordered` | Whether the responses come back in the order of the requests. SOLAR promises it; JSON-RPC 2.0 does not, so a client that speaks to both asks |
| `cancellation.accepted` | Whether a call in flight can be asked to stop, section 9 |
| `cancellation.method` | What does the asking, so that the name can move without breaking a reader. `null` when cancellation is not accepted |
| `notifications.accepted` | Always `false` in `solar/1`, which is the deviation of section 3.1 |
| `limits.*` | Every number a caller has to respect: the request line of section 2, the queue bounds of 9.6, the window of 9.7, the cap of section 10 and the default of 8.3 |

**A client reads this instead of experimenting.** A limit that changes without the
manifest changing fails a test in `solar-core/src/manifest.rs`, which compares each number
with the constant that enforces it.

`capabilities` arrived in `schema_version` **2.1.0**, a minor bump: a member was added to
the document and nothing else changed, so a consumer written against 2.0.0 reads a 2.1.0
manifest by ignoring what it does not know.

### 8.3 Examples are tests

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

### 8.4 A response has a maximum size

Every API declares `max_output_bytes`, the largest response it may produce, measured on
the serialised `result.data`. The default for the whole contract is **8 MiB**, half the
request line limit of section 2.

An API that produces more gets `RESOURCE_EXHAUSTED` / `OUTPUT_TOO_LARGE`, and the response
the caller receives is that error rather than a line nothing can buffer. The limit exists
because a session is a pipe: a response that cannot be held in memory cannot be sent, and
a caller that receives a line of unbounded size cannot read it either.

**Large data travels by pagination or by reference**, never in one response: a page and a
cursor, or a path to a file the caller opens. `docs/ADDING_AN_API.md` says so where an API
is written.

## 9. Cancellation

A caller that has stopped waiting says so with an ordinary call. SOLAR accepts no
notifications, so there is no special message: `solar.cancel`, with `{"id": <the id>}`,
is an API like any other, with a specification, a schema and examples.

### 9.1 What a session does while a call is running

A session reads its input on a thread of its own, into a queue. Calls still run **one at a
time, in the order they arrived**. The single exception is `solar.cancel` **sent on its
own**, which is answered the moment it is read, because a cancellation that waited its
turn behind the call it is cancelling would be useless. Inside a batch it waits its turn
like any other element: a batch is one message, and its elements run in order.

A response may therefore arrive out of order: the answer to a `solar.cancel` sent second
can precede the answer to the call sent first. JSON-RPC 2.0 allows this, and `id` is how a
caller matches an answer to its question.

**A client that never calls `solar.cancel` sees its responses in the order of its
requests.** That is a promise, and a test holds it. Two things a client does to itself are
outside it, and both are answered the moment they are read rather than in their turn: a
request that arrives at a full queue, section 9.6, and a request whose `id` is already in
flight, section 9.5. A client that does neither, which is every client that reads its
responses, sees pure order.

### 9.2 What `solar.cancel` reports

| `outcome` | What happened |
| --------- | -------------- |
| `cancelled_while_queued` | The call had not started. It will not start, and it has already been answered with `CANCELLED`. |
| `cancellation_requested` | The call is running. Its handler has been told, and it will end with its own result or with `CANCELLED`, whichever it reaches first. |
| `already_finished` | The call was answered before the cancellation arrived. Nothing changed. |
| `unknown` | No call with that `id` has been seen in this session. Nothing changed. |

### 9.3 Exactly one response, whatever the timing

**Every request gets exactly one response: its result, or `CANCELLED`, never both and
never neither.** This holds however the cancellation and the call are interleaved, and it
is the property the implementation is built around: the queue and the running call are
owned by one lock, so a cancellation finds a call either waiting, where it is removed and
answered at once, or running, where its handler is told. It can never find it in both
states, nor in neither.

A call cancelled while queued is answered with `CANCELLED` / `CALL_CANCELLED`.

### 9.4 What a handler must do

A handler receives a cancellation token through its [`Context`] and checks it at points
where stopping is safe. A handler that never checks is not a special case: it runs to its
end, or past its budget, and section 10 already says what happens then.

**Dispatch checks the token once before the handler starts.** A call cancelled between
leaving the queue and beginning is answered `CANCELLED` without running, because the first
safe point of a call that has not begun is not beginning.

Cancelling is a request, not a command. A handler that has already produced its result
returns it, and the caller is told `already_finished`.

### 9.5 An `id` may not be reused while it is alive

Cancellation targets a call by `id`, so an `id` that is already queued or running in this
session is refused for a new request, with `INVALID_ARGUMENT` / `ID_IN_FLIGHT`. An `id`
becomes free again as soon as its call is answered.

**A client that never calls `solar.cancel` is unaffected by any of this**, as long as it
does not reuse an `id` while the first call is still unanswered, which it could not have
matched anyway.

### 9.6 The queue is bounded

The queue of section 9.1 is bounded twice: **256 requests** and **64 MiB** of request
text, whichever is reached first. A session that reads faster than it runs would otherwise
grow without end.

When the queue is full, the session **keeps reading**. Each request that arrives is
answered at once with `RESOURCE_EXHAUSTED` / `QUEUE_FULL`, saying how many requests and
how many bytes are waiting, and `solar.cancel` is still accepted and still answered at
once, because a full queue is exactly when cancelling matters.

A client that sends one request and waits for its response never meets this limit.

### 9.7 What a session remembers

To answer `already_finished`, a session remembers the identifiers of the calls it has
answered: the **most recent 1024**. An `id` older than that is reported as `unknown`
rather than `already_finished`.

The two outcomes mean the same thing to a caller that is cancelling, which is that there
is nothing to cancel. The distinction is a courtesy, and it is bounded so that a long
session does not grow with every call it has ever answered.

## 10. Timeouts and panics

- Dispatch runs every handler on a worker thread and waits for `timeout_ms`.
- A handler that panics produces `INTERNAL` / `HANDLER_PANIC`, with the panic message and
  the source location in `details`. The process survives, and the session continues.
- A handler that runs past its budget produces `DEADLINE_EXCEEDED` / `HANDLER_TIMEOUT`.
  The response goes out immediately. **The worker thread is abandoned, not killed**: Rust
  has no safe way to kill a thread. An abandoned handler keeps running until it finishes,
  and its result is discarded. Handlers are therefore written so that their own internal
  budgets are shorter than `timeout_ms`.
- **Abandoned threads are capped at 64.** Each one keeps its stack, so an API that
  overruns on every call would otherwise consume the process. While 64 are alive, a new
  call is refused with `UNAVAILABLE` / `TOO_MANY_ABANDONED` before it starts, and the
  count falls as the abandoned handlers finish. A caller can read the current count in
  `system.info`, under `abandoned_workers`.
- Cancellation has a section of its own, section 9. A handler that ignores its token
  is abandoned exactly like one that overruns, which is what this section describes.

## 11. Versions

- **Protocol**: `solar/1`. The number changes only when an existing message shape changes in
  a way that breaks a caller. Adding an API never changes it.
- **API**: each API carries its own semantic version. A breaking change to its parameters or
  its output is a major bump of that API alone.
- **Manifest**: `schema_version`, semantic, currently `2.0.0`. A consumer MUST reject a
  manifest whose major differs from the one it was written against. It went to `2.0.0`
  when the schemas stopped being self-contained and began sharing the `$defs` of section
  8.1.
- **SOLAR**: the version of the build, reported in `meta.solar_version`.

## 12. Recording a session

`solar serve --stdio --record <file>` writes every line that crosses, with the time it
crossed, and `solar replay <file>` sends the requests again and reports where the answers
differ. The file is **NDJSON with a header line naming a format version**, and
[`RECORDING.md`](RECORDING.md) is its normative specification, with a JSON Schema for one
line.

What this section promises, and what `RECORDING.md` details:

- **A recording is versioned.** The first line declares `solar_recording`, semantic, and a
  reader refuses a file whose major it does not know rather than guessing at it.
- **A line is kept as it crossed**, as a string, without its newline and without being
  re-serialised. A line that was never valid JSON is recorded as faithfully as one that
  was, because a recording of a parse error is worth keeping.
- **Something other than SOLAR may write one.** A file that follows `RECORDING.md`
  replays, whatever wrote it.
- **Replay ignores only what cannot be reproduced**: the members of `meta` that differ
  between any two runs, and every timestamp anywhere in a response. Everything else,
  including the whole of `result.data`, is compared as it stands.

## 13. Conformance

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
| A batch answers in order, one response per element | `solar-apis/tests/properties.rs::a_batch_answers_every_element_in_order`, `conformance/cases/batch_*` |
| A batch that is wrong as a whole answers with a single response | `solar-core/src/protocol.rs` tests, `solar-cli/tests/cli.rs::an_empty_batch_is_refused_with_a_single_response` |
| Every request gets exactly one response, whatever the cancellation timing | `solar-apis/tests/cancellation.rs::every_request_gets_exactly_one_response_however_the_race_falls` |
| A client that never cancels sees its responses in order | `::a_client_that_never_cancels_sees_its_responses_in_order` |
| The queue is bounded, and a cancellation is answered even when it is full | `::a_full_queue_refuses_new_requests_and_still_answers_a_cancellation` |
| A response larger than its API declares is refused | `solar-core/tests/dispatch.rs::a_response_larger_than_the_api_declares_is_refused` |
| Abandoned handlers are capped, and the count is visible | `solar-core/tests/abandoned.rs::abandoned_handlers_are_capped_and_the_count_is_visible` |

## 14. Exit codes of the `solar` binary

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
| `13`      | `CANCELLED`                                           |
| `70`      | SOLAR could not even produce a response, for example because standard output was closed. |

`solar serve --stdio` exits `0` when the input ends cleanly, whatever the individual calls
returned, because the session itself succeeded.
