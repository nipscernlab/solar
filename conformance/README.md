# The conformance suite

Request and response pairs in plain JSON, one file per case. Nothing here is Rust, and
nothing here needs Rust to run: a client written in TypeScript, in Python, or in anything
else can replay these files against the `solar` binary and check that it agrees.

This is what makes the protocol portable. The Rust tests are one implementation of the
runner, in `crates/solar-apis/tests/conformance.rs`; the files are the truth.

## What a case looks like

```json
{
  "name": "ping_with_a_message",
  "description": "The simplest successful call there is",
  "request": {"jsonrpc": "2.0", "id": 1, "method": "solar.ping", "params": {"message": "hi"}},
  "response": {
    "jsonrpc": "2.0",
    "id": 1,
    "result": {
      "data": {"pong": true, "echo": "hi", "received_at": "$any"},
      "meta": "$any",
      "warnings": []
    }
  },
  "match": "subset"
}
```

- `request` is written to the session exactly as it stands, on one line.
- `response` is what must come back.
- `match` is `exact` or `subset`, the two modes of section 8.1 of the contract. `subset`
  compares the members the case names and ignores the rest, which is how a case says
  nothing about `meta`.
- The string `"$any"` matches any value at that position. Use it for everything that
  differs between two runs: timestamps, durations, paths, versions.

A case whose `request` is not valid JSON is written as `request_line`, a string, so that
the suite can cover what happens to a malformed line.

A `request` that is a JSON **array** is a batch, and its `response` is then an array too,
compared element by element, in order. A batch that is refused as a whole answers with a
single object, so those cases have an object for a `response` like any other.

## Running them

Against a built binary, in one session:

```bash no-run
# no-run: the Rust runner does exactly this, and CI runs the Rust runner
jq -c '.request' conformance/cases/*.json | solar serve --stdio
```

In Rust, and in CI:

```bash no-run
# no-run: part of the test suite, which CI runs as its own step
cargo nextest run -p solar-apis --test conformance
```

## It really is language-neutral

That claim was checked rather than asserted, twice. The first eleven cases were replayed
from Python against the release binary in stage two. All fifteen were replayed again on
27 September 2026, with a forty line client and no Rust involved: **fifteen sent, fifteen
answered as the case says, none failed.**

A client in TypeScript needs the same forty lines: send `request` or `request_line`, read
one line back, and compare by the rule in `match`, treating `"$any"` as a wildcard. The
one thing worth knowing before writing it is that the bytes matter: one case begins with
a byte order mark, so the request goes to the process as UTF-8 bytes rather than through
whatever encoding the system happens to prefer.

## Adding one

Write the file, run the suite, and read the difference it reports. A case is a promise to
every client in every language, so it says only what the contract really requires: prefer
`subset`, and prefer `$any` to a value your machine happens to produce.
