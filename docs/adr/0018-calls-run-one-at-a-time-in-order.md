# 18. Calls run one at a time, in the order they arrived

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

Record 11 gave a session two threads, so that `solar.cancel` could be answered while a
call was running. It said, inside a record about cancelling, that calls still run one at a
time in the order they arrived. That is a decision in its own right, and it was left
stated in a record about something else.

It is also the promise most callers actually depend on. A client that never cancels wants
what it always had: the answer to its third request after the answer to its second.

## Decision

**Calls run one at a time, in the order they arrived.** A session reads ahead, and runs
nothing ahead. A client that never calls `solar.cancel` sees its responses in the order of
its requests.

Two things a client does to itself are outside that promise, and both are answered the
moment they are read rather than in their turn: a request that arrives at a full queue,
section 9.6 of the contract, and a request whose `id` is already in flight, section 9.5.

## Consequences

**There is no concurrency to reason about inside a call.** A handler cannot observe
another handler, no API needs a lock, and an API that is slow delays the ones behind it
rather than racing them. That is the trade, and it is the right one for a core whose
callers are interfaces and agents rather than a load balancer.

It is tested rather than assumed: forty requests through a real session come back in the
order they were sent, and a batch answers its elements in the order they appear.

The throughput this costs is measured, in `STATUS.md`: about 29 microseconds per call for
the thread handoff that reading ahead needs.

Running calls concurrently would break this promise, so it would be a breaking change and
would need a record superseding this one. The entry that used to say the session was
strictly sequential stays in `docs/OPEN_QUESTIONS.md`, marked superseded, pointing here.
