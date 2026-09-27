# 33. Local paths are stripped with computed remap flags, not committed ones

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

A release binary must not name the machine that built it: not the home directory, not the
user name, not the path of the repository. `trim-paths`, the profile key that will one day
do this, still needs `-Z` on the pinned 1.97, which was checked against the toolchain
rather than assumed.

The fallback is `--remap-path-prefix`, and the two prefixes that leak differ per machine,
so they cannot live in a committed configuration file.

## Decision

The flags are **computed**, in `xtask/src/flags.rs`, and applied to every build `xtask`
makes. CI exports the same flags for the builds it makes directly.

What is enforced is the artefact rather than the flags: **`cargo xtask leak-check` builds
the release binary and scans every byte of it** for the home directory, the user name as a
path segment and the repository root, in both slash spellings.

## Consequences

**A bare `cargo build --release` outside `xtask`, on a developer's machine, still embeds
that machine's paths.** That is recorded plainly rather than hidden: the check is on the
artefact that ships, and the artefact that ships is built by the pipeline.

macOS needed one more flag than the others. Mach-O keeps debug information as OSO entries
naming the object files the linker read, absolute paths that rustc's remap never sees;
`-oso_prefix` strips one prefix from them, and the home directory covers both the target
directory and the registry cache. That was found by the scan, not by reading.

When `trim-paths` stabilises, the flags move into the release profile and this record gets
a successor.
