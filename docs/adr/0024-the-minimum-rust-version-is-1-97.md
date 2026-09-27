# 24. The declared minimum Rust version is 1.97

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

`rust-version` in `Cargo.toml` is a promise: a compiler at least this new will build this.
Edition 2024 needs 1.85, so the true floor is somewhere between 1.85 and the 1.97.1 this
was written with, and nobody has looked for it.

Three options were on the table: leave it at the version that was actually used, bisect
for the real floor, or raise it with each release.

## Decision

`rust-version = "1.97"`, the compiler this was built and tested with, and it **stays there
until somebody is actually blocked**.

## Consequences

Declaring a version that was never tried is the kind of claim that costs someone an
afternoon: they take it at face value, hit an error from a feature stabilised later, and
have to bisect the thing anyway with less information than we have.

The claim is checked rather than asserted: a weekly job builds and tests with exactly the
declared minimum, so the day it stops being true is the day a job goes red rather than the
day somebody complains.

Finding the real floor is a scheduled job and a few hours whenever somebody has an older
toolchain and cares. Until then it buys nothing.
