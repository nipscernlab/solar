# 9. One reusable worker thread per dispatching thread

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** measurement, confirmed by Chrysthofer Arthur Amaro Afonso

## Context

Dispatch runs every handler on a worker thread, so that a panic cannot take the session
down and so that a budget can be enforced without cooperation from the handler. The first
implementation started a thread per call.

Then it was measured. `cargo bench` on the development machine, a Windows laptop with an
i7-13620H:

| What | Median |
| ---- | ------ |
| A whole `solar.ping` through dispatch | 77 us |
| The same call stopping at the parameters, which starts no thread | 4 us |

Starting the thread was about 70 of the 77 microseconds: more than the parsing, the
registry lookup, the parameter decoding and the serialisation put together.

## Decision

One worker thread is started per dispatching thread, on the first call, and reused. A call
that runs past its budget drops it, and the next call starts a fresh one, because the
thread it abandoned can never be trusted again, per record 8.

## Consequences

The same ping now costs **7.59 us**, a tenfold improvement, and a session round trip
through two pipes costs 72 us, which is dominated by the pipes rather than by SOLAR.

Panic isolation and the budget are unchanged: the worker catches the unwind and survives
it, so a panicking call does not even cost a new thread.

The state that is now shared between two threads is one `mpsc` channel pair and one mutex
around the warnings. That is the whole of the concurrency, which is why `loom` is not used
here and why `OPEN_QUESTIONS.md` says what would change that: a second lock, an atomic of
our own, or more than one worker.

This record exists because the measurement changed the design, and the next person to
wonder why dispatch is not simply `entry.invoke(ctx, params)` deserves the number.
