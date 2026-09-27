# 15. A registry that does not build answers every call with the same error

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

The registry checks the template: names that match the rule, no duplicates, a complete
specification, at least one example. If a build of SOLAR ever shipped with an API that
broke one of those, the process would know at startup, before it had read a single
request.

The obvious response is to refuse to start.

## Decision

The process **starts anyway** and answers every call with `INTERNAL` /
`INVARIANT_BROKEN`, naming every rule that was broken.

## Consequences

The first principle holds even here: every call gets a response. A caller that sees that
error learns what is wrong with the build it is talking to; a caller whose process exited
before it could connect learns only that something is not there, which is the least useful
failure there is.

This costs nothing in practice, because the contract tests fail on the same list long
before a release: `crates/solar-apis/tests/contract.rs` builds the registry and asserts it
is valid, so a broken one never reaches a tag.

`Dispatcher::broken` is the state that does this, and a test drives it with a registry
holding the same API twice.
