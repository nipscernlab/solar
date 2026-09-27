# 29. The mark is printed by `solar version` and nowhere else

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

`docs/brand/README.md` asks for the mark to appear where a person sees SOLAR. The contract
requires machine-readable output to stay exact, byte for byte, which rules out decorating
the protocol.

## Decision

The mark is printed by **`solar version`**, and by no other command. It appears only when
standard output is a terminal, and it is coloured only when `NO_COLOR` is absent or empty,
as [no-color.org](https://no-color.org) asks.

## Consequences

The NDJSON stream of `solar serve --stdio`, the envelope printed by `solar call` and every
piped output stay exactly what the contract says they are. A test asserts that logging and
decoration never reach standard output.

`solar version` is the one human command where the mark says something rather than
decorating: it tells you at a glance which build is answering, which is the question that
command exists for.

Record 30 covers the drawing itself on terminals that cannot render it.
