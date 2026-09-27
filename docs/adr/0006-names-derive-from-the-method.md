# 6. The file, the module and the struct of an API follow from its name

- **Status:** Accepted
- **Date:** 2026-09-26
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

Adding an API touches four things that must agree: the method name, the file it lives in,
the module that declares it and the struct that implements it. Four things that must agree
are three opportunities to disagree, and the disagreement is found at the worst moment, by
whoever is adding the fifth API.

## Decision

Three of the four follow from the first, by a rule with no exceptions:

| From the name | Rule | Example |
| ------------- | ---- | ------- |
| The file | dots become underscores, `.rs` | `build.run_target` gives `build_run_target.rs` |
| The module | the same, without the extension | `mod build_run_target` |
| The struct | `PascalCase` of the whole name, dots and underscores removed | `struct BuildRunTarget` |

## Consequences

Anybody, and any tool, can work out the other three from any one of them, without looking.
`cargo xtask new-api` applies the rule, and the contract tests check the name itself.

The existing APIs were renamed to obey it: `solar.ping` is `SolarPing` rather than `Ping`,
and `solar.manifest` is `SolarManifest` rather than `ManifestApi`, a name that had only
been chosen to avoid colliding with the core type `Manifest`. The rule makes such
collisions impossible, which is the second reason for it.

The cost is a struct name that reads a little heavily inside its own module:
`solar_ping::SolarPing`. The registration list, where every name appears together, reads
better for it.
