# 7. Every API is experimental until SOLAR reaches 1.0.0

- **Status:** Accepted
- **Date:** 2026-09-26
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

`ApiSpec` has a `stability` of `experimental`, `stable` or `deprecated`. Some of the five
APIs of the first stage look as settled as anything ever gets: `solar.ping` is unlikely to
change. Marking each one by judgement would mean arguing the question five times, and
again for every API after that.

## Decision

While `solar_version` is below `1.0.0`, every API is `experimental`. A specification that
claims otherwise fails the contract tests.

## Consequences

Nothing in SOLAR can be more stable than SOLAR. A caller reading the manifest sees one
honest answer rather than a scatter of judgements, and nobody has to defend why
`solar.ping` is stable while `system.info` is not.

The first stable release is then a deliberate act: somebody changes the rule, and the
tests make them look at every API while doing it.
