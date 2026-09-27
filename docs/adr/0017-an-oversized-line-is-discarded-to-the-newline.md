# 17. An oversized line is discarded up to the next newline

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

The session refuses a request line longer than 16 MiB. "Refuse without reading the rest"
cannot mean "stop reading": the bytes that follow are still in the pipe, and whatever
comes after the limit would then be parsed as new messages. A 20 MiB line would become a
refusal followed by several megabytes of nonsense, each piece of it answered.

## Decision

SOLAR stops **buffering** at the limit, answers `RESOURCE_EXHAUSTED` / `MESSAGE_TOO_LARGE`
with `id: null`, and then **reads and throws away** the rest of that line, up to the next
newline.

## Consequences

Nothing oversized is ever held in memory, which is the part that matters: the bytes past
the limit are read and dropped a chunk at a time.

The stream stays aligned, so the message after the oversized one is answered normally.
That is tested: a line past the limit followed by a valid request produces a refusal and
then the right answer.

`id: null` is unavoidable and is stated in the contract, because the `id` may well be in
the part that was never kept.
