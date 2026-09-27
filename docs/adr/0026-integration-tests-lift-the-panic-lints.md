# 26. Integration tests lift the lints that forbid panicking

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

`unwrap`, `expect` and `panic!` are denied across the workspace, because the first
principle is that every call gets a response and a panic in production code is a call that
does not.

A test reports failure by panicking. There is no other mechanism.

## Decision

Files under `tests/` and `benches/` carry an `allow` for those three lints, written at the
top of each file with `reason = "..."` beside it.

## Consequences

`clippy.toml` already exempts `#[cfg(test)]` modules inside a crate, which covers unit
tests. An integration test is its own crate, so the exemption does not reach it and the
allow has to be written.

Writing it per file, with a reason, rather than at workspace level keeps the lint alive
everywhere else, which is record 36's rule applied to this case.

The reason is the same sentence in every file, which is a small amount of repetition in
exchange for never having to wonder whether a particular `unwrap` was meant.
