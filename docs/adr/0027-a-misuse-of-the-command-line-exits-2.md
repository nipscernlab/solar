# 27. A misuse of the command line exits 2, the same as `INVALID_ARGUMENT`

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

The `solar` binary maps every status to an exit code, so a script can branch on the number
without reading the JSON. A misuse of the command line itself, an unknown subcommand, a
missing argument, an unreadable `--log` level, is not a status: no call was made.

It still needs a number.

## Decision

A misuse of the command line exits **2**, the same code as `INVALID_ARGUMENT`.

## Consequences

It is the same kind of mistake seen from a different place: the caller asked for something
that cannot be understood. A script that checks for 2 should not have to learn a second
number for the case where the argument was wrong before the protocol was reached.

The cost is that 2 does not distinguish "your parameters were wrong" from "your command
line was wrong". Standard error says which, in a sentence, and standard output carries the
envelope in the first case and nothing in the second, so a script that cares can tell.
