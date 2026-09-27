# 8. A handler that overruns is abandoned, not killed

- **Status:** Accepted
- **Date:** 2026-09-26
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

Every API declares a `timeout_ms`, and dispatch enforces it, because a session that hangs
breaks the first principle as surely as one that answers nothing. Rust has no safe way to
kill a thread, so the enforcement has to mean something other than killing.

The alternatives were: no timeout at all, which is not enforcement; or a cancellation
token that every handler must remember to check, which is a promise the compiler cannot
keep and which the first handler to forget would break.

## Decision

Dispatch waits for the budget. When it runs out, the response goes to the caller
immediately with `DEADLINE_EXCEEDED` / `HANDLER_TIMEOUT`, and the thread carries on until
it finishes, with its result thrown away.

Section 9 of the contract states this plainly, in those words, rather than implying that
the work was undone.

## Consequences

The caller is never left waiting, which is what the budget is for. The work is not undone,
which anybody writing a handler that touches anything outside itself must know: a handler
that starts something long lived gives it a shorter budget of its own.

An abandoned thread is never reused, because its next answer would belong to a call that
has already been given up on. The worker is dropped and the next call starts a fresh one,
which is the cost of a timeout and a reason they should be rare.

`solar-core/tests/dispatch.rs` proves the behaviour: the caller gets its error inside the
budget, and the next call neither waits for the abandoned handler nor receives its answer.
