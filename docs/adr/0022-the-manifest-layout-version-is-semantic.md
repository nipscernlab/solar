# 22. The manifest layout version is semantic

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

The manifest carries a version of its own layout, separate from the version of SOLAR and
from the version of each API. It could have been an integer, bumped when the shape
changes, which is what many schema registries do.

## Decision

`schema_version` is a **semantic version**, `"2.0.0"` at the time of writing, not `2`. A
consumer MUST refuse a manifest whose **major** differs from the one it was written
against.

## Consequences

A member can be added to the manifest in a minor bump without every consumer refusing the
file, which is what an integer version cannot express: with an integer, every change looks
the same to a reader and the safe response to any of them is to refuse.

It is the same rule as everything else in the repository, so nobody has to remember a
second one.

It has been used: `1.0.0` to `2.0.0` when the schemas stopped being self-contained and
began sharing the `$defs` of section 8.1, which is a structural change a consumer must
notice.
