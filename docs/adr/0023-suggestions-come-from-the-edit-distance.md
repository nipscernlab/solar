# 23. Suggestions come from the Levenshtein distance, at most three of them

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

`METHOD_NOT_FOUND` is a better error when it says what the caller probably meant. Any
suggestion mechanism has to decide how far it will guess, and a mechanism that guesses too
freely produces `did you mean solar.ping?` for `build.compile`, which is worse than saying
nothing.

## Decision

A candidate is offered when the **Levenshtein distance is at most 3** and **smaller than
the candidate's own length**. Ties are broken **alphabetically**.

## Consequences

The threshold catches a mistyped letter, a swapped pair and a missing word, and refuses to
guess when the caller wrote something else entirely. The second condition stops short
names from matching everything: `a.b` is not a suggestion for `x.y` merely because both
are short.

Alphabetical ties make the same mistake produce the same error every time, on every
machine, which matters because those errors are in snapshots and in conformance cases.

`crates/solar-core/src/text.rs` holds the distance and the choice, and it is the one file
taken to zero surviving mutants as a worked example.
