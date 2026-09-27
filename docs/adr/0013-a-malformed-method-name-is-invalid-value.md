# 13. A method name that breaks the naming rule is `INVALID_VALUE`, not `NOT_FOUND`

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

`solar.ping` is a method; `Solar.Ping` is not, because section 4 of the contract requires
lower case. Two answers were possible for the second one: consult the registry, find
nothing, and say `NOT_FOUND` with a suggestion, or refuse the name before looking at all.

## Decision

A method name that does not match the naming rule is refused with `INVALID_ARGUMENT` /
`INVALID_VALUE`, **before the registry is consulted**, with a hint that names are lower
case.

## Consequences

The caller is told what is actually wrong. `Solar.Ping` cannot possibly be registered,
whatever is in the registry, so `NOT_FOUND` would be a true statement that points at the
wrong thing: it would invite the caller to check the list of methods rather than the shape
of the name.

The hint offers the lower case spelling, which is the correction in nine cases out of ten,
and a caller that meant something else entirely still learns the rule.

`NOT_FOUND` keeps its meaning: a well formed name that nothing answers to, where the edit
distance search of record 23 is worth running.
