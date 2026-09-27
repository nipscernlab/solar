# 19. `system.info` reports the release of the operating system

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

The first draft of `system.info` left out the release of the operating system, on the
grounds that reading it meant running `sw_vers` or `lsb_release`, and no API in this stage
starts an external program.

**The architect overruled that on 26 September 2026**, and was right: every system keeps
the release somewhere a process can read directly.

## Decision

`system.info` reports `os_name`, `os_release` and `os_build`, read where each system keeps
them and never by starting a program:

- **Linux**, and anything else following the os-release specification: `/etc/os-release`,
  then `/usr/lib/os-release`, which is the fallback that specification names.
- **macOS**: `/System/Library/CoreServices/SystemVersion.plist`, for `ProductVersion` and
  `ProductBuildVersion`.
- **Windows**: `RtlGetVersion` in `ntdll`. `GetVersionEx`, its documented alternative,
  reports an older version to a program that carries no compatibility manifest, so it
  would have this build call Windows 11 something else.

A source that is missing or unreadable leaves the three members `null` and adds an
`OS_RELEASE_UNAVAILABLE` warning naming the source that was tried. **Nothing is guessed,
and the call still succeeds.**

## Consequences

`system.info` went to 1.1.0 for it: members were added and none changed, which is a minor
bump under the version scheme of the contract.

The Windows path needs a foreign function call, which is `unsafe`. That is record 20, and
it is the only `unsafe` in the repository.

The rule for choosing between the two Linux paths is compiled everywhere and tested
everywhere, since stage three: only the paths themselves are conditional.
