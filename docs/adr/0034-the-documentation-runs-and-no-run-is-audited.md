# 34. The documentation runner executes blocks, and `no-run` is the audited exception

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

The README claimed a quoting form that does not work in `cmd.exe`. Nobody noticed, because
nothing ran it. Documentation that drifts is worse than none: a reader trusts it.

## Decision

Every fenced block tagged `bash`, `powershell` or `cmd` is **executed by `cargo xtask
doc-run`, in that shell**, as a step of the one command and of CI. A block tagged
**`no-run`** is skipped, and its first line says why.

`powershell` means Windows PowerShell 5.1, the shell whose quoting the README documents,
so those blocks run on the Windows job.

## Consequences

The documentation cannot claim something the binary does not do. It has already caught a
second invented fact: the testing guide said a call with a misspelled parameter exits 3,
and `INVALID_ARGUMENT` exits 2.

`no-run` is an exception that has to be argued in writing, once per block, which keeps the
list short and reviewable: the blocks that would mutate the working tree, the ones that
are already steps of the pipeline, and the ones in the testing guide that install a
toolchain or wait for a terminal.

A block that needs the binary gets it: the runner builds the release binary first and puts
it on the path the blocks use.
