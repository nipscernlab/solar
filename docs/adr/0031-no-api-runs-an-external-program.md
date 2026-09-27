# 31. No API runs an external program

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

SOLAR exists to drive the tools of the Constellation project: simulators, synthesizers,
waveform viewers. An early draft had `tools.detect`, a table of known tools and a process
runner, and they worked.

The architect removed them, so that the first stages are the core of the API and nothing
else: the envelope, the template, dispatch, the registry, the session.

## Decision

**No API runs an external program**, in this version. Third-party tools come in a later
stage, after this version is delivered to the laboratory.

The vocabulary they will need is kept: `spawns_process` is still one of the side effects a
specification may declare, so the contract does not have to change to admit the first one.

## Consequences

`docs/CONTRACT.md` has no section about running external programs, and the error catalogue
has no reason for a program that cannot be started. Both come back with the first API that
needs them.

What was removed is in the history at commit `1029db4` and its parent, so it is recovered
by reading rather than rewritten from memory. The reasons that went with it were
`UNKNOWN_TOOL`, `TOOL_TABLE_INVALID`, `ACCESS_DENIED`, `PROGRAM_TIMEOUT` and
`SPAWN_FAILED`, and the warning codes were `TOOL_TABLE_OVERRIDDEN`, `VERSION_NOT_PARSED`
and `TOOL_EXITED_NON_ZERO`.

The gain is that everything the core promises is provable without a machine that happens
to have a simulator installed: the whole suite runs anywhere Rust does.
