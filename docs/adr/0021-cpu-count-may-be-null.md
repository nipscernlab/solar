# 21. `cpu_count` may be `null`

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

`system.info` reports how many threads can run at once. `std::thread::available_parallelism`
returns an error when the system does not say, which happens inside some containers and on
systems that do not expose the number.

Something has to go in the member.

## Decision

`cpu_count` is **`null`** when the system does not report it, rather than `1`.

## Consequences

`1` would be a guess that looks like a measurement, and a caller sizing a pool from it
would size it wrong while believing it had asked.

`null` makes the caller decide, which is the honest place for the decision: a client that
wants a default can pick one, knowing it picked it.

This is the same rule as the three release members of record 19, and the same rule the
whole contract follows: nothing is guessed, and what could not be read says so.
