# 10. Batches are answered in the order they were sent

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

Section 6 of JSON-RPC 2.0 allows a client to send an array of requests and requires the
server to answer with an array of responses. It explicitly permits the server to answer
them **in any order**, and tells the client to match responses to requests by `id`.

`solar/1` refused batches until now, answering `UNIMPLEMENTED`. That kept the first stage
small, and it cost a round trip for every small call: a client that wants to call
`solar.list`, `solar.describe` and `solar.version` pays three of them.

The specification's freedom of order is a problem for the audience SOLAR is written for.
An agent reading a transcript, or a person reading a recording made by
`solar serve --stdio --record`, has to reconstruct the correspondence before it can read
anything. A deterministic transcript is worth more than the freedom to reorder, which
SOLAR would not use anyway: its elements run one at a time.

## Decision

A line holding a JSON array of requests is answered with **one line** holding the array of
responses, **in the order of the requests**. Every element gets its own response, including
the invalid ones, and each response carries its own `meta`.

Three things are refused as a whole, with a single response rather than an array, because
the batch itself is what is wrong:

| What | Status and reason |
| ---- | ----------------- |
| An empty array | `INVALID_ARGUMENT` / `BATCH_EMPTY`, code `-32600` |
| More than 64 elements | `RESOURCE_EXHAUSTED` / `BATCH_TOO_LARGE` |
| Two elements with the same `id` | `INVALID_ARGUMENT` / `DUPLICATE_ID` |

The limit of 64 exists because the elements run one at a time: a larger batch holds the
session while the caller cannot tell how far it has got. It is not a memory bound; the
16 MiB line limit already is one.

Refusing a repeated `id` is stricter than JSON-RPC 2.0, which says nothing about it. Two
responses carrying the same `id` could not be told apart by a client that matches by `id`,
which is exactly what the specification tells clients to do.

## Consequences

**A client that sends no batch sees no difference**, so this is an additive change: the
protocol stays `solar/1` and the version scheme asks for a minor version.

Section 3.1 of the contract is now *the one* deliberate deviation from JSON-RPC 2.0
rather than the first of two. Record 2 said there were two; batches were the other one,
and this record is why there is now one.

Answering in order is a promise, so it is tested: a property test sends batches of
generated ids and checks that the responses come back in the order of the requests, and a
fuzz target checks that one response comes back per element whatever the array holds.
`Dispatcher::answer_line` is the call that speaks this protocol; `handle_line`, which
answers with a single `Response`, stays for the single request case that every benchmark
and most tests use.

A future change that ran the elements concurrently would break the order this record
promises, so it would be a breaking change, and would need a new record superseding this
one.
