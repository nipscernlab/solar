# Open questions

Everything here was decided without asking, because the brief said to take the most
conservative option and write down why. Each entry says what was chosen, what it rules
out, and what would make it worth revisiting.

**What is settled has moved.** A decision the architect has confirmed becomes a record in
[`adr/`](adr/) and leaves this file, so that this file is only ever the list of things
still open to being overruled.

**On 27 September 2026 the architect confirmed all twenty-seven entries that stood here**,
and each of them is now a record: 12 to 38, with 18 written for the part of the superseded
entry below that is still a decision. The reasoning is unchanged; what changed is that it
is settled and dated rather than provisional.

## Still open

**Nothing.** Every decision made alone so far has been confirmed and recorded.

A decision made alone in a later stage is written here first, with what it rules out and
what would make it worth revisiting, and moves to `adr/` when the architect confirms it.

## Superseded, and kept as a marker

### The session is strictly sequential

It used to say that one request was read, dispatched and answered before the next was
read. Cancellation made that impossible: a `solar.cancel` sent while a call was running
would have waited in the pipe until the call it was cancelling had finished.

**Superseded on 27 September 2026 by record
[11](adr/0011-cancelling-is-an-ordinary-call.md).** A session now reads on a thread of its
own, into a bounded queue.

The part that was worth keeping is a record of its own, record
[18](adr/0018-calls-run-one-at-a-time-in-order.md): **calls still run one at a time, in
the order they arrived**, so a client that never cancels sees its responses in the order
of its requests. What changed is only that reading no longer waits for running.

This entry stays here, rather than disappearing, because somebody who remembers the old
behaviour will come looking for it, and a file that simply drops what it used to say is a
file that cannot be trusted about its own history.
