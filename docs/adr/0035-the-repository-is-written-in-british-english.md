# 35. The repository is written in British English

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

Licence, serialise, behaviour, catalogue: each of them has an American spelling that a
contributor may reach for without thinking. The first stage already wrote one way without
saying so, which is how a repository ends up with both.

## Decision

**British English**, everywhere: prose, comments, error messages and documentation.
`typos` runs with `locale = "en-gb"`, so an American spelling is flagged like any other
typo, and `docs/STYLE.md` states the choice for the prose a tool cannot check.

## Consequences

The wire is unaffected: status names, reason names and JSON members are what the contract
says they are, and several of them are American because the vocabulary they come from is.
The spelling check exempts them by name.

It caught a real case in stage three: a sample of cargo's own output quoted in the testing
guide, where cargo uses the American spelling of "optimised". The guide now describes the
line instead of quoting it, which is better documentation anyway. It caught this record
too, which quoted the spellings it forbids.

The rule costs nothing to follow and stops a reviewer from ever having to raise it.
