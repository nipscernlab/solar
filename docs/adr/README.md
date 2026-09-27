# Decision records

One file per decision, in the [MADR](https://adr.github.io/madr/) format. A record says
what was decided, what it rules out, and why, in enough detail that somebody who was not
there can disagree with it on the merits.

A decision arrives in [`../OPEN_QUESTIONS.md`](../OPEN_QUESTIONS.md), which is the list of
things settled without asking and still open to being overruled. When the architect
confirms one, it moves here and leaves that file. So:

- **`OPEN_QUESTIONS.md`** is what is still open.
- **`docs/adr/`** is what is settled.

A record is never edited to say something different. A decision that is reversed gets a
new record that supersedes the old one, and the old one is marked as superseded, because
the reasoning that was once persuasive is part of the history.

| Record | Decision | Status |
| ------ | -------- | ------ |
| [0001](0001-json-rpc-over-stdio.md) | JSON-RPC 2.0 over standard input and output, one message per line | Accepted |
| [0002](0002-every-call-gets-a-response.md) | Notifications are refused: every call gets a response | Accepted |
| [0003](0003-params-must-be-an-object.md) | `params`, when present, must be an object, and `null` is refused | Accepted |
| [0004](0004-one-response-shape-per-api.md) | One API, one response shape | Accepted |
| [0005](0005-error-data-has-four-members.md) | `error.data` has exactly four members | Accepted |
| [0006](0006-names-derive-from-the-method.md) | The file, module and struct of an API follow from its name | Accepted |
| [0007](0007-experimental-until-one-point-zero.md) | Every API is experimental until SOLAR 1.0.0 | Accepted |
| [0008](0008-abandon-a-handler-that-overruns.md) | A handler that overruns is abandoned, not killed | Accepted |
| [0009](0009-one-reusable-worker-thread.md) | One reusable worker thread per dispatching thread | Accepted |
| [0010](0010-batches-answer-in-order.md) | Batches are answered in the order they were sent | Accepted |
| [0011](0011-cancelling-is-an-ordinary-call.md) | Cancelling is an ordinary call, and every request still gets exactly one response | Accepted |
