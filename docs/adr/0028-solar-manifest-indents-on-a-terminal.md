# 28. `solar manifest` indents on a terminal and prints one line into a pipe

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

The manifest is over a thousand lines indented and one line compact. A person running
`solar manifest` to read it wants the first; a program running it to parse it wants the
second, and a thousand lines of indentation through a pipe is waste.

Detecting a terminal to change output is a thing programs do badly: it surprises people
when a pipeline behaves differently from what they saw on screen.

## Decision

`solar manifest` indents when standard output is a terminal and prints one line when it is
not. **`--pretty` forces the indented form.** Nothing else in the command line interface
changes with the terminal.

## Consequences

A person gets something readable without asking, and a pipeline gets NDJSON-shaped output
without asking. The surprise is bounded because it happens in exactly one command and is
documented in the contract and in `solar manifest --help`.

The escape hatch is a flag rather than an environment variable, so a script that wants the
indented form says so in the command it runs, where the next reader will see it.

The mark of record 29 follows the same rule for the same reason.
