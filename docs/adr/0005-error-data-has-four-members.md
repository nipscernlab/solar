# 5. `error.data` has exactly four members

- **Status:** Accepted
- **Date:** 2026-09-26
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

The second principle of SOLAR is that a response carries as much context as it can, and
the audience includes agents. A natural addition to an error is a `retriable` flag, since
an agent deciding whether to try again is exactly the case the principle is about.

## Decision

`error.data` carries `status`, `reason`, `details` and `meta`, and nothing else in
`solar/1`. Retriability is documented per status in `docs/ERRORS.md`.

## Consequences

A caller can pattern match on a fixed shape. A field that is sometimes there is a field
every caller has to test for, and the value of "as much context as it can" is in the
detail entries, which already say what was expected, what arrived, where, and what to do.

Retriability is a property of the status, not of the occurrence: `DEADLINE_EXCEEDED` is
always worth retrying and `INVALID_ARGUMENT` never is. Putting it in the catalogue means
an agent reads it once rather than on every error.

A future version that finds a genuine per-occurrence reason to retry, a rate limit with a
time in it for instance, can add the member: adding to `data` is a minor change of the
protocol, not a breaking one.
