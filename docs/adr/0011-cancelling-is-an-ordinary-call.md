# 11. Cancelling is an ordinary call, and every request still gets exactly one response

- **Status:** Accepted
- **Date:** 2026-09-27
- **Decided by:** Chrysthofer Arthur Amaro Afonso, architect of SOLAR

## Context

A caller that has stopped waiting for a call has, until now, had nothing to say. It could
close the session, which throws away every other call in it, or wait for a budget it does
not control. For the audience SOLAR is written for, an agent that changed its mind and a
person who pressed Ctrl+C, that is the wrong pair of choices.

Every protocol that has solved this used a notification: LSP sends `$/cancelRequest`, and
the JSON-RPC way is a message that expects no reply. Record 2 refuses notifications, so
that door is closed on purpose.

There is also a structural problem. A session read one line, answered it, and read the
next. A cancellation sent while a call was running would sit in the pipe until the call it
was cancelling had finished, which is the one moment it is useless.

## Decision

**Cancelling is an ordinary call:** `solar.cancel`, with `{"id": <the id>}`, following the
template like every other API, and reporting `cancelled_while_queued`,
`cancellation_requested`, `already_finished` or `unknown`.

**A session reads on a thread of its own, into a queue.** Calls still run one at a time, in
the order they arrived. `solar.cancel` is the one message answered where it is read rather
than in its turn.

**Every request gets exactly one response**, its result or `CANCELLED`, never both and
never neither. The queue and the running call are owned by **one lock**, so a cancellation
finds a call either waiting, where it is removed and answered at once, or running, where
its handler is told. It can never find it in both states, nor in neither.

**Cancelling is a request, not a command.** A handler checks `ctx.is_cancelled()` at points
where stopping is safe. A handler that never checks runs to its end or past its budget and
is abandoned exactly as record 8 says.

Two consequences of reading ahead are declared rather than hidden:

- **An `id` may not be reused while it is alive.** Cancellation targets a call by `id`, so
  an `id` names one call at a time: `INVALID_ARGUMENT` / `ID_IN_FLIGHT`.
- **The queue is bounded**, by 256 requests and 64 MiB, and a request that arrives at a
  full queue is answered at once with `RESOURCE_EXHAUSTED` / `QUEUE_FULL`, while
  `solar.cancel` is still accepted. A session that reads faster than it runs would
  otherwise grow without end.

## Consequences

**A client that never calls `solar.cancel` sees no difference**, and sees its responses in
the order of its requests, which is a promise and a test. The two exceptions are things a
client does to itself and is told about at once: filling the queue, and reusing an `id`
that is unanswered. This is therefore an additive change: the protocol stays `solar/1`.

A session now costs two threads rather than one, plus the worker each of them starts on
demand. That is the price of answering a cancellation while a call is running, and it is
paid once per session rather than once per call.

Responses may arrive out of order when `solar.cancel` is used: the answer to a
cancellation sent second can precede the answer to the call sent first, and a call
cancelled while queued is answered before the cancellation that caused it. JSON-RPC 2.0
allows this and says `id` is how a caller matches an answer to a question. `solar replay`
was changed to match by `id` for the same reason.

The exactly-one-response promise is tested by driving the races on purpose, with handlers
that stop inside the session until the test lets them go, and by repeating the unsynchronised
race many times and checking that every id was answered exactly once whichever way it fell.
`loom`, which was left open in `OPEN_QUESTIONS.md` until there was a second lock to model,
is still not used here: there is one lock and no lock-free protocol between the threads, so
what `loom` explores does not exist in this design. If a later change adds a second lock or
an atomic protocol, that entry comes back.
