# 2. Notifications are refused, because every call gets a response

- **Status:** Accepted
- **Date:** 2026-09-26
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

JSON-RPC 2.0 defines a notification as a message without an `id`, and forbids a server
from answering one. That leaves a caller unable to distinguish a notification that
succeeded, one that failed, and one that was never received.

The first principle of SOLAR is that every call gets a response. The audience includes
agents, which cannot notice silence and cannot ask what happened.

## Decision

SOLAR does not accept notifications. A message without an `id`, or with `id: null`,
receives an error with code `-32600`, status `INVALID_ARGUMENT`, reason
`NOTIFICATION_NOT_SUPPORTED`, and `id: null`.

This is stated in section 3.1 of the contract as one of exactly two deliberate deviations
from JSON-RPC 2.0. The other is that batches are `UNIMPLEMENTED` in `solar/1`.

## Consequences

A caller that speaks JSON-RPC 2.0 correctly and sends a notification gets a clear error
rather than silence, which is the point. A client library that sends notifications
automatically, for something like a cancellation, has to be told not to.

Every code path in dispatch therefore ends in a response: a panic becomes `INTERNAL`, an
overrun budget becomes `DEADLINE_EXCEEDED`, a registry that did not build answers every
call with the reason it did not. A property test drives arbitrary bytes at the dispatcher
and checks that exactly one well formed response comes back every time.

> **Editorial note, 27 September 2026.** The sentence above about batches was true when
> this record was written and is no longer: record 10 implemented them. The decision this
> record makes is unaffected, since a batch is a line of requests and every one of them
> still gets a response. Nothing else here has been changed.
