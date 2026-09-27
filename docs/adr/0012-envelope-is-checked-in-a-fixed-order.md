# 12. The envelope is checked in a fixed order

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

A request can break several rules at once: no `id`, a `jsonrpc` that is not `"2.0"`, a
method name in capitals, and `params` that is an array. Something has to decide which of
those the caller is told about, and any order at all is a decision, including the one that
falls out of how the code happens to be written.

Left to chance, two implementations of `solar/1` would disagree about what a broken
message deserves, and a test written against one would fail against the other.

## Decision

The members are checked in the order **`id`, `jsonrpc`, `method`, `params`**, and a
message that breaks several rules is answered about the first one in that order.

`id` comes first because every later error can then be addressed to the right call: a
caller that sent four requests and gets back `INVALID_VALUE` with `id: null` learns much
less than one that gets it with `id: 3`.

The order is stated in section 3 of the contract, not only in the code, so that it is part
of the protocol rather than an accident of this implementation.

## Consequences

A message with two faults reports one of them, and fixing it reveals the next. That is the
cost, and it is smaller than the alternative: an error that lists everything wrong with a
message is harder to read than the first thing wrong with it.

`crates/solar-core/src/protocol.rs` checks in that order and its tests pin each step, so a
refactor that reorders them fails rather than quietly changing the protocol.
