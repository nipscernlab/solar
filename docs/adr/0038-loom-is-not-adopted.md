# 38. `loom` is not adopted, and here is what would change that

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

The brief for stage two asked for `loom` if the reusable worker and the abandonment of an
overrunning call have shared state it can model, and for the reason if not. At the time
they did not: one `std::sync::mpsc` channel pair between exactly two threads, one
`Mutex<Vec<Warning>>` inside the context, no atomics of SOLAR's own.

Cancellation changed that in stage three, which is what the original entry said to watch
for: a second lock, the `Mutex<Inner>` of `SessionState`; atomics, the one-way flag of
`Cancellation` and the counter of abandoned workers; and a second thread per session.

## Decision

**`loom` is still not adopted**, for a reason that has changed.

**Every transition of the session state happens under one lock.** A message is queued,
taken, cancelled or finished inside `SessionState`, and each of those is one critical
section; no lock is ever held while another is taken, so there is no ordering to get
wrong. What is left for `loom` to explore is the order in which those critical sections
run, and that order is small enough to enumerate: a cancellation arrives while its target
is queued, while it is running, or after it has finished.

Each of the three is tested deterministically in `crates/solar-apis/tests/cancellation.rs`,
with handlers that stop inside the session until the test lets them go, which is stronger
than repeating an unsynchronised race and hoping to hit them. The unsynchronised race is
also repeated forty times, as a second net.

## Consequences

The shape of the code under test stays what ships. Adopting `loom` would mean making
`session.rs` generic over its synchronisation primitives or duplicating it behind
`cfg(loom)`, and what that would prove is largely what the deterministic tests already
pin.

**Revisit when** a transition of the session state happens outside that lock, or when the
two threads talk to each other through atomics rather than through it. Either makes the
interleavings SOLAR's own in a way enumeration cannot cover, and then `loom` earns the
change.
