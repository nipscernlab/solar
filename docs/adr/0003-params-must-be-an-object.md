# 3. `params`, when present, must be an object, and `null` is refused

- **Status:** Accepted
- **Date:** 2026-09-26
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

Many clients send `"params": null` when a call takes no parameters. Accepting it would
cost nothing today and would save those callers one correction.

## Decision

`params` may be absent, which means `{}`. When it is present it must be an object.
`null` is refused with `TYPE_MISMATCH` and a hint that says to omit the member.

## Consequences

This is the specification rather than strictness of ours. JSON-RPC 2.0, section 4.2,
requires `params`, when present, to be a structured value: an object or an array. `null`
is neither, so a message carrying it is already invalid before SOLAR has an opinion.

SOLAR narrows the structured value further, to an object, because parameters here are
always passed by name. That narrowing is SOLAR's own and section 3 of the contract states
it. Passing by position would tie every caller to the order of a struct's fields, which is
exactly the kind of coupling the manifest exists to remove.

The decision was flagged as the one most likely to be overruled, because accepting `null`
later is a compatible change while refusing it later would not be. The architect confirmed
the refusal on 26 September 2026.
