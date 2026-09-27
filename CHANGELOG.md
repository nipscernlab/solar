# Changelog

Every notable change to SOLAR, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the versions follow
[semantic versioning](https://semver.org/spec/v2.0.0.html).

The protocol has a version of its own, `solar/1`, which is not this one. A change to the
protocol is stated here in its own line.

## [Unreleased]

### Added

- The `solar/1` protocol: JSON-RPC 2.0 over standard input and output, one message per
  line, with two deliberate deviations. Notifications are refused, because every call gets
  a response, and batches are `UNIMPLEMENTED` in this version.
- `solar-core`: the envelope, the eleven canonical statuses and the reason catalogue, the
  `Api` template, the registry that checks it, dispatch with a worker thread per call that
  turns a panic into `INTERNAL` and an overrun budget into `DEADLINE_EXCEEDED`, the NDJSON
  session loop with a 16 MiB line limit, and the manifest generator.
- Five APIs: `solar.ping`, `solar.version`, `solar.manifest`, `solar.describe` and
  `system.info`.
- `system.info` 1.1.0 reports the release of the operating system in `os_name`,
  `os_release` and `os_build`, read where each system keeps it and never by starting a
  program: the os-release file on Linux, `SystemVersion.plist` on macOS, `RtlGetVersion`
  on Windows. A source that cannot be read leaves the three null and adds an
  `OS_RELEASE_UNAVAILABLE` warning naming what was tried.
- The `solar` binary: `call`, `serve --stdio`, `list`, `describe`, `manifest` and
  `version`, with an exit code per status.
- `cargo xtask manifest` and `cargo xtask new-api <name>`.
- `docs/CONTRACT.md`, `docs/ERRORS.md`, `docs/ADDING_AN_API.md` and
  `docs/OPEN_QUESTIONS.md`.
- Continuous integration on Linux, Windows and macOS, with the toolchain pinned by
  `rust-toolchain.toml`, every cargo command `--locked`, a weekly early-warning job on the
  latest stable and a weekly proof of the declared minimum.
- The documentation runs: every `bash`, `powershell` and `cmd` block of the Markdown is
  executed by `cargo xtask doc-run` in that shell, in CI, on every push.
- `cargo xtask leak-check`: the release binary is scanned byte by byte and embeds no home
  directory, no user name and no repository root. Release builds carry
  `debug = "line-tables-only"`, so a crash report names the file and the line.
- `solar call` logs at `SOLAR_LOG` levels exactly as a session does; it used to stay
  silent.
- The hints of `solar list` and `solar describe` are pastable in the shell they name:
  `bash`, Windows PowerShell and `cmd.exe` disagree about quotes, so an example with
  parameters is printed once per shell.

[Unreleased]: https://github.com/nipscernlab/solar/commits/main
