# The SOLAR error catalogue

Every SOLAR error carries a `status` and a `reason`. The status is one of eleven canonical
codes, the same ones used by Google and by gRPC, and it answers *what kind of failure was
this*. The reason is a finer grained constant, and it answers *which failure exactly*. A
reason belongs to exactly one status.

This file is the complete catalogue, and it is checked by a test: a reason that exists in
the code and not here, or here and not in the code, fails `solar-core/tests/docs.rs`.

Errors are linked from responses through the `docs` member of every detail entry, in the
form `docs/ERRORS.md#<status in lowercase>`, so a reader always lands on the status section
and finds the reason just below it.

## How to read an error

```json
{"jsonrpc":"2.0","id":1,"error":{
  "code":-32601,
  "message":"Method not found: solar.pnig.",
  "data":{
    "status":"NOT_FOUND",
    "reason":"METHOD_NOT_FOUND",
    "details":[{"field":"method","expected":"solar.ping","received":"solar.pnig",
                "hint":"Did you mean \"solar.ping\"? The edit distance is 2.",
                "docs":"docs/ERRORS.md#not_found"}],
    "meta":{"request_id":1,"method":"solar.pnig","api_version":null,
            "solar_version":"0.1.0","protocol":"solar/1",
            "started_at":"2026-09-26T21:41:03.123456Z","duration_us":58,
            "os":"windows","arch":"x86_64"}}}}
```

Read it in this order: `reason` tells you what to fix, `details[].hint` tells you how, and
`meta` tells you which build answered and how long it took.

## INVALID_ARGUMENT

JSON-RPC code `-32602`, except at the envelope level where it is `-32700` for a parse error
and `-32600` for a malformed envelope. Not retriable: the same request will fail the same
way. The caller sent something SOLAR cannot accept.

### PARSE_ERROR

The line is not valid JSON, or not valid UTF-8. Code `-32700`, `id` is `null` because there
is no way to know what it was.

`details[0]` carries `field: null`, `expected: "a JSON object"`, `received` as the
offending text truncated to 200 characters, and a hint with the position reported by the
JSON parser.

Fix the producer. The usual causes are a newline inside the message, since the framing is
one message per line, and a trailing comma.

### MISSING_FIELD

A member that the contract requires is absent. At the envelope level this is `jsonrpc` or
`method` and the code is `-32600`; inside `params` it is any required parameter and the code
is `-32602`.

`field` holds the JSON pointer of what is missing, `received` is `null`, and `hint` shows a
minimal request that would work.

### UNKNOWN_FIELD

A member inside `params` that the API does not declare. Every parameter type is declared
with `deny_unknown_fields`, so a misspelled parameter is refused instead of being ignored.
Unknown members at the *envelope* level are ignored, as JSON-RPC 2.0 requires; this reason
is only about `params`.

`field` holds the pointer of the unexpected member and `hint` lists the members the API
does accept.

### TYPE_MISMATCH

The member is there and holds the wrong kind of value: a number where a string belongs, an
array where an object belongs. `expected` names the type the schema requires and `received`
is the value that arrived.

### INVALID_VALUE

The type is right and the value is not usable: `jsonrpc` that is not `"2.0"`, a method name
that does not match `^[a-z]+(\.[a-z]+(_[a-z]+)*)+$`, an integer outside the range of its
type, an empty string where a name belongs.

### NOTIFICATION_NOT_SUPPORTED

The message has no `id`, or its `id` is `null`. JSON-RPC 2.0 calls that a notification and
forbids a reply; SOLAR does not accept notifications, because every call gets a response.
Code `-32600`, and the response carries `id: null`.

Add an `id`. Any number or string will do, and it comes back untouched in
`meta.request_id`.

## NOT_FOUND

JSON-RPC code `-32601`. Not retriable. A name was given and nothing answers to it.

### METHOD_NOT_FOUND

The `method` of the request is not registered. SOLAR measures the Levenshtein distance
between what arrived and every registered name and returns the three closest, each as its
own detail entry whose `expected` holds the candidate. A candidate is only offered when the
distance is at most 3 and smaller than the length of the name itself.

Call `solar.manifest` for the full list, or `solar list` from the command line.

### API_NOT_FOUND

`solar.describe` or `solar.manifest` was asked about an API that is not registered. Same
suggestion mechanism as `METHOD_NOT_FOUND`. The distinction matters: `METHOD_NOT_FOUND`
means *the call you made does not exist*, while `API_NOT_FOUND` means *the call you made
exists and the thing you asked it about does not*.

## ALREADY_EXISTS

JSON-RPC code `-32001`. Not retriable. Creating something that is already there.

No API in `solar/1` returns this status. It is part of the canonical list, it has its place
in the status to code table, and it is reserved for the first API that writes something.

## FAILED_PRECONDITION

JSON-RPC code `-32002`. Not retriable without changing the state of the system first. The
request is well formed, and the system is not in a state where it can be served.

No API in `solar/1` returns this status. It is reserved for the first API that depends on
something being prepared beforehand.

## PERMISSION_DENIED

JSON-RPC code `-32003`. Not retriable as the same user. The operating system refused.

No API in `solar/1` returns this status. It is reserved for the first API that opens a
file it may not open, or starts a program it may not start.

## RESOURCE_EXHAUSTED

JSON-RPC code `-32004`. Retriable only with a smaller request. A declared limit was reached.

### MESSAGE_TOO_LARGE

A request line went past 16 MiB. SOLAR stops buffering at the limit and answers with
`id: null`, since the `id` may well be in the part that was never read. It then discards
bytes up to the next newline so that the following message is read normally.

`details` reports the limit and how many bytes had been read when the limit was reached.

## DEADLINE_EXCEEDED

JSON-RPC code `-32005`. Retriable. The work ran past the time it was given.

### HANDLER_TIMEOUT

The API did not finish within the `timeout_ms` it declares in its specification. The
response is sent immediately. The worker thread is abandoned rather than killed, because
Rust has no safe way to kill a thread; it keeps running until it finishes and its result is
thrown away. `details` reports the budget and the method.

## UNAVAILABLE

JSON-RPC code `-32006`. Retriable. Something SOLAR depends on is not there right now.

### THREAD_SPAWN_FAILED

SOLAR could not start the worker thread that a call runs on, which means the operating
system is out of threads or out of memory. The call never started. Retrying after the load
drops is the right response.

### ENVIRONMENT_UNAVAILABLE

SOLAR could not read something about its own process that it needs to answer: the current
directory, which can happen when the directory has been deleted underneath the process, or
the path of its own executable.

## UNIMPLEMENTED

JSON-RPC code `-32007`. Not retriable. Valid, understood, and not built yet.

### BATCH_NOT_SUPPORTED

The message is a JSON array. Batches are valid JSON-RPC 2.0 and `solar/1` does not
implement them. Send one request per line instead; `solar serve --stdio` keeps the session
open and answers each one in order.

## INTERNAL

JSON-RPC code `-32603`. Not retriable, and always a bug in SOLAR. Please report it with the
`meta` block, which names the exact build.

### HANDLER_PANIC

An API panicked. Dispatch catches the unwind, turns it into this error and keeps the
process alive, so a session survives a bug in one call. `details` carries the panic message
and the file and line where it happened.

### SERIALIZATION_FAILED

The API produced a value that could not be turned into JSON. The output types are generated
from Rust types, so this can only happen through a non-finite floating point number or a
map with non-string keys.

### INVARIANT_BROKEN

SOLAR verified something that cannot be false, and it was false: a registry that describes
an API twice, a value that changed type between two steps of the same call. The call never
reached the API. `details` says which invariant broke.

This is always a bug in SOLAR itself, and the contract tests exist to catch it before a
release. Report it with the `meta` block.

## UNKNOWN

JSON-RPC code `-32099`. A failure that could not be classified.

No reason maps to this status in `solar/1`, and none should ever have to. It exists so that
the canonical list is complete and so that a caller has a sensible default arm in a match.

## Warnings

A warning travels in `result.warnings` when a call succeeded and the caller still needs to
know something. Each one is `{"code": "...", "message": "..."}`. The codes are closed and
documented here, exactly like reasons.

### BUILD_METADATA_INCOMPLETE

`solar.version` could not report every piece of build metadata, because the binary was
built outside a Git working tree or without Git available. The fields it could not fill are
`"unknown"`.
