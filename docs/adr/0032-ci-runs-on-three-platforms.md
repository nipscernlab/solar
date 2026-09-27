# 32. Continuous integration runs on Linux, Windows and macOS

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

The laboratory uses all three. The work is done on Windows, which is where the first stage
was written, and Windows is the least like the others: a different path separator, a
different quoting rule in two shells, a byte order mark on the first line PowerShell
writes, and a refusal to replace a running executable.

Testing on one of the three would have found none of those.

## Decision

The pipeline builds, lints, tests, checks the manifest and builds a release on
**`ubuntu-latest`, `windows-latest` and `macos-latest`**, with a fourth job for the API
documentation and a fifth for coverage.

## Consequences

It cost about six minutes of Windows runner per push, and it found four defects that a
single platform would not have: the quoting form the README claimed for `cmd.exe`, the
byte order mark PowerShell writes, the home directory that `ld64` embeds on macOS through
its OSO entries, and the relink that Windows refuses while a binary is running.

The first run passed on all four jobs, run
[36287385822](https://github.com/nipscernlab/solar/actions/runs/36287385822), with Windows
the slowest at 6m24s.

The five checks are what the branch ruleset on `main` requires, so this record is also
what makes a pull request mergeable.
