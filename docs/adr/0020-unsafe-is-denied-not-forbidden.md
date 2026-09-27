# 20. Unsafe code is denied rather than forbidden, for exactly one function

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

The workspace lint on `unsafe_code` was `forbid`, which cannot be lifted anywhere, at all,
by any attribute. Then record 19 required `RtlGetVersion`, a foreign function, and calling
one is `unsafe`.

`forbid` would have left starting a program as the only way to ask Windows what it is,
which the scope of the stage refuses, or leaving Windows unable to say.

## Decision

The workspace lint is **`deny`**, and `windows_version` in
`crates/solar-apis/src/os_release.rs` carries the only `allow(unsafe_code)` in the
repository, with a `SAFETY` comment above the call.

## Consequences

`grep -rn "allow(unsafe_code)" crates/` finds the whole of the exception. That is the
property worth having: not that there is no unsafe code, which was never quite true of a
program that talks to an operating system, but that every line of it can be listed in one
command.

`clippy::undocumented_unsafe_blocks` is denied workspace-wide, so the `SAFETY` comment is
enforced by a machine rather than by a reviewer.

The call is six lines. If a second one ever appears, the question to ask again is whether
a dependency such as `windows-sys` is cheaper than two exceptions, which was considered
and rejected for one.
