# 30. The ASCII drawing of the mark is a switch, not a guess

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

The mark of record 29 is drawn with half block characters. A terminal that cannot render
them shows a screen of replacement characters instead, which is worse than no mark at all.

There is no reliable way to ask a terminal whether it can draw one. `TERM`, the code page
and the font are all guesses, and a wrong guess is invisible to whoever wrote the code and
obvious to whoever is looking at the screen.

## Decision

**`SOLAR_ASCII`, set to any value, picks the 7-bit drawing.** There is no detection.

## Consequences

The failure mode is chosen by the person who can see the screen, which is the only place
the information exists.

A documented switch is honest about what is not knowable, and it costs one line in the
documentation and one environment variable in a script that needs it.

Guessing from `TERM` would have been wrong on somebody's machine, and the report would
have been a photograph of mojibake with no way to reproduce it here.
