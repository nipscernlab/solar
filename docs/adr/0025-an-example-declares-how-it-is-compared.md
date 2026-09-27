# 25. An example declares how it is compared, and `$any` stands for what cannot be reproduced

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

Every API carries examples, and the examples are run as tests. A response holds a
timestamp, a duration in microseconds, a path, a version and a compiler string, none of
which is the same twice. An example that wrote them out would fail on every machine
including the one that produced it.

Excluding examples from the tests was the other option, and it makes them decoration.

## Decision

An example declares its own comparison: **`exact`** compares everything, **`subset`**
compares the members the example names and ignores the rest, and the string **`"$any"`**
matches any value at that position.

## Consequences

Examples are tests, and a test that cannot pass is worse than no test. `$any` keeps the
shape checked while admitting what is not knowable in advance, and `subset` lets an
example say something about `result.data` without saying anything about `meta`.

The cost is that an example can hide a change behind `$any`, so the rule for writing one
is in `docs/ADDING_AN_API.md`: prefer `subset`, and prefer `$any` only for what a machine
cannot reproduce.

The same two words are what a conformance case uses, so a case and an example are read the
same way by a client in any language.
