# Adding an API

An API is one file and one line. Nothing that already exists has to change, the manifest
regenerates itself, and the tests check the template so that nobody has to remember it.

This document is written for whoever adds the next one, which may well be an agent. Follow
it in order.

## 1. Choose the name

`namespace.verb_noun`, lower case, in English, matching
`^[a-z]+(\.[a-z]+(_[a-z]+)*)+$`. The namespace says which part of Constellation the call
belongs to, and the rest says what it does to what: `tools.detect`, `build.run_target`,
`system.info`.

A name is forever. It is what every interface, every script and every agent will write.
Read it aloud before you keep it.

## 2. Generate the file

```bash no-run
# no-run: writes a file and edits the registry
cargo xtask new-api build.run_target
```

That writes `crates/solar-apis/src/build_run_target.rs` from the template and adds two
lines to `crates/solar-apis/src/lib.rs`, both in alphabetical order: the module and the
registration. The struct is the name in `PascalCase` with the dots and underscores
removed, so `build.run_target` becomes `BuildRunTarget`, and the file is the name with the
dots turned into underscores. The rule has no exceptions, which is what lets a tool, or an
agent, work out any of the three from any other.

The generated file compiles and passes the whole test suite before you touch it. That is
deliberate: you start from something correct and keep it correct.

## 3. Fill it in

**`Params`** is what the caller sends. It always carries `#[serde(deny_unknown_fields)]`,
so a misspelled parameter is refused instead of silently ignored. Every member gets a doc
comment, because that comment becomes the `description` in the JSON Schema, which is what
an agent reads.

**`Output`** is what comes back in `result.data`. Same rule: a doc comment on every member.

**`spec()`** is the part worth spending time on.

| Field | What to write |
| ----- | -------------- |
| `summary` | One line, no full stop. It is what `solar list` prints. |
| `description` | Prose. What the API is for, what it guarantees, what it refuses to do, and anything a caller would otherwise learn by experiment. An agent reads this before deciding to call. |
| `errors` | Every `{status, reason}` this API can return, on top of what dispatch itself produces. Returning an undeclared failure is a bug. |
| `side_effects` | What it touches. `none` is exclusive. |
| `idempotent` | Whether calling twice is the same as calling once. |
| `stability` | `Experimental` while SOLAR is below 1.0.0. A test enforces it. |
| `since` | The SOLAR version this API first appears in, not the API's own version. |
| `timeout_ms` | The budget of one call. Dispatch enforces it. Be generous but finite. |
| `examples` | At least one, and they are tests. See below. |

**`call()`** does the work. It gets a `&Context`, which knows how much time is left and
holds the registry, and it returns either the output or a `SolarError` whose reason is one
of the ones declared above.

## 4. Write examples that are true

An example is replayed by `examples_match_real_execution`, which runs it through the real
dispatcher and compares what comes back. An example that lies fails the build.

Use `Example::exact` when you know the whole answer, and `Example::subset` when you know
part of it. Use the token `"$any"` for anything the machine running the test cannot
reproduce: a timestamp, a path, a duration, a version.

```rust
Example::subset(
    "one_target",
    "Builds a single target",
    json!({"target": "hello"}),
    json!({"target": "hello", "artefacts": ANY, "duration_ms": ANY}),
)
```

Never write a value you have not seen a real run produce.

## 5. Run the tests

```bash no-run
# no-run: the whole suite already runs as its own CI step
cargo test -p solar-apis
```

The contract tests will tell you, by name, which rule you have not satisfied yet: a
summary that ends in a full stop, a description too short to be useful, a parameter type
that accepts unknown members, an example whose response does not match reality, an error
that is not in `docs/ERRORS.md`.

If you need a reason that does not exist yet, add it to `Reason` in
`crates/solar-core/src/reason.rs` **and** to `docs/ERRORS.md`, under the status it belongs
to. `solar-core/tests/docs.rs` fails until both are there.

## 6. Regenerate the manifest

```bash
cargo xtask manifest
```

Commit the result. `cargo xtask manifest --check` is what CI runs, and it fails when the
versioned file is not what the generator produces.

## 7. Before you push

```bash no-run
# no-run: each line is already its own CI step
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All three run in CI on Linux, Windows and macOS.

## What never to do

- Never register an API anywhere but the one list in `lib.rs`. There is no discovery and
  no macro that finds things: the list is the truth.
- Never edit `manifest/solar.manifest.json` by hand.
- Never use `unwrap`, `expect` or `panic!` in an API. The lints refuse it, and the reason
  is the first principle of the contract: every call gets a response.
- Never write a value in an example that you have not seen.
- Never change an existing API's parameters or output without a major bump of that API's
  own version.
