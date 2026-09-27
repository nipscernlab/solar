# 16. The panic hook is global and installed by the dispatcher

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

A handler that panics must become `INTERNAL` / `HANDLER_PANIC` with the panic message and
the place it happened, and the session must survive. Rust gives the location only through
the panic hook, which is process-wide: there is one, and installing it replaces whatever
was there.

Replacing the hook of a whole process from inside a library is not a small thing. It is
what makes `cargo test` print where a test failed.

## Decision

Dispatch installs a global panic hook, once, through `std::sync::Once`. The hook records
the location of a panic **that happened inside a call** and stays quiet about it. A panic
anywhere else is **forwarded to the hook that was already installed**.

## Consequences

`cargo test` behaves exactly as it did: a panic in a test prints its message and its
place, because the hook that was there when dispatch installed its own still runs.

A panic inside a call prints nothing of its own, because dispatch is about to answer with
an envelope that says the same thing better, and standard output must stay protocol.

A thread local flag is what tells the two cases apart, set while a handler runs. It is the
only global state dispatch keeps besides the count of abandoned workers.
