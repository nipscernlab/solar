# 1. JSON-RPC 2.0 over standard input and output, one message per line

- **Status:** Accepted
- **Date:** 2026-09-26
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

SOLAR is the single boundary of the Constellation project. Every interface talks to it:
the `solar` command line interface, the AURORA IDE, and artificial intelligence agents.
The transport had to be one that all three can speak without ceremony, on Windows, Linux
and macOS alike, and that a person can drive by hand while debugging.

## Decision

JSON-RPC 2.0 over standard input and output, one JSON message per line, the framing known
as NDJSON. No HTTP, no port, no gRPC.

## Consequences

An interface starts SOLAR as a child process and writes lines to it. There is nothing to
listen on, so there is nothing to secure, no port to collide, and no daemon to leave
running. This is the framing LSP servers and MCP use, so an editor or an agent framework
already knows the shape.

The cost is that a session is one process and one conversation: no fan-out, no
subscriptions, no server push. Nothing in the project needs those today, and adding one
would be a protocol change, `solar/2`, rather than an implementation detail.

One message per line also means a message can never contain a raw newline, which the
serialiser guarantees and a test checks.
