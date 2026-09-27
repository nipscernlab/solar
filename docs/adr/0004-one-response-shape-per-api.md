# 4. One API, one response shape

- **Status:** Accepted
- **Date:** 2026-09-26
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

`solar.manifest` takes an optional `api`. The obvious reading of "returns the whole
manifest or the entry of one API" is that the shape of the answer changes with the
parameter: a document when the parameter is absent, a bare entry when it is present.

## Decision

`solar.manifest` always answers with the manifest document. With `api`, the `apis` array
holds exactly that one entry. `solar.describe` is the API that returns a bare entry.

## Consequences

A caller parses one shape whatever it asked for, and a caller that wants the bare entry
calls the API whose job that is. This is the rigidity principle applied to the answer
rather than to the declaration: an API that could answer with two shapes would need every
caller to branch, and every schema to be a union.

The cost is one extra level of nesting for a caller that only ever wants one entry from
the manifest, which is what `solar.describe` is for.
