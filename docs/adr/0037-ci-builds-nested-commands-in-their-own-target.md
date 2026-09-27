# 37. `cargo xtask ci` builds its nested commands in a target directory of their own

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

The one command is itself `target/debug/xtask.exe`. A nested `cargo test --workspace`
wants to relink that very file, and Windows refuses while it is running: *failed to remove
file ... Access is denied*. The same happens to the documentation blocks that call
`cargo xtask manifest`.

## Decision

Every nested build uses **`target/ci`**, passed explicitly rather than through the
environment, since `std::env::set_var` is `unsafe` in edition 2024 and unsafe code is
denied.

CI itself runs the cargo commands directly, where nothing is running from the tree, so it
keeps the default directory.

**`cargo xtask ci` removes those trees when it finishes**, and says how much that freed.
They are scaffolding, not a cache.

## Consequences

The first run after a change to the sources is slower than the second would have been,
which is the price of not keeping the tree.

The rule was wrongly applied once, and that is worth recording: the same environment
variable was given to `cargo mutants`, which does not build in the real tree at all but
copies the sources once per job. One shared target directory made cargo reuse a test
binary built in another copy, and a test binary carries the `CARGO_MANIFEST_DIR` of the
tree that compiled it, so `tests/docs.rs` read the contract from a directory that had
already been deleted. Worse than the failure was what it implied: a mutant could be judged
by an artefact built from a different mutant. The variable is gone from that task, with a
comment saying why its absence is deliberate.
