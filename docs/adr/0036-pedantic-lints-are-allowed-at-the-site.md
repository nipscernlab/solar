# 36. Pedantic lints are fixed or allowed at the site, never at the workspace

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

The brief for stage two asked for `clippy::pedantic` as warnings, with each allowed lint
listed at workspace level with its reason. Doing that would silence a lint everywhere,
including the future places where it is right.

## Decision

Every pedantic finding is **either fixed, or allowed exactly where it fires**, with
`reason = "..."` on the attribute. There is no workspace-level allow.

`clippy::allow_attributes_without_reason` is denied workspace-wide, so the reason is
enforced by a machine rather than by a reviewer.

## Consequences

**The letter of the brief bends and its intent is enforced:** nothing is silenced without
a written argument, and the argument sits where the next reader will meet it.

The cost is a few more attributes in the source than a workspace list would need, and the
gain is that the lint stays alive in every other place. A cast that loses precision is
still a warning tomorrow in a file nobody has written yet.

`grep -rn "allow(clippy" crates/ xtask/` lists every exception with its reason, which is
the review the workspace list was meant to make possible.
