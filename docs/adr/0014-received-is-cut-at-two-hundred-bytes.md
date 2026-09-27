# 14. A value echoed in `received` is cut at 200 bytes

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

Every error says what arrived, in `details[].received`, because an error that does not
quote the input leaves the caller guessing. A request may be 16 MiB, and quoting it would
put 16 MiB into the error, through the same pipe, for a caller that has just been told its
request was too large.

## Decision

A value echoed in `received` is cut at **200 bytes**. A long string is truncated with a
`[...]` marker; a large array or object is replaced by a sentence describing it.

## Consequences

The type of `received` changes when a composite is described: what was an object becomes a
string saying what the object was. That is the price, and it is paid only past the limit.

The alternative considered was to omit the value entirely when it is large, which tells
the caller nothing and makes the error worse exactly when the request is hardest to read.

`MAX_RECEIVED_BYTES` in `crates/solar-core/src/protocol.rs` is the number, `brief` is the
function, and its tests pin both the truncation marker and the description.
