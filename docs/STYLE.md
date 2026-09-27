# How this repository is written

Code is read far more often than it is written, and this code will be read by people who
did not attend the conversation that produced it, in laboratories that are not this one,
and by agents that cannot ask what something meant. These rules exist for them.

They are rules, not preferences. Where a tool can enforce one, it does, and the rule says
which tool.

## The language

**British English**, everywhere: licence, serialise, behaviour, catalogue, artefact.
`typos` runs with `locale = "en-gb"` and refuses the American spellings.

The exceptions are never prose: an identifier the ecosystem fixes, such as serde's
`Serialize`, and a constant published on the wire, such as `SERIALIZATION_FAILED`. Those
are listed in `typos.toml` with the reason beside them.

Write plainly. Short sentences. No emoji, no exclamation marks, no filler: "it is
important to note that", "simply", "just", "obviously". If a sentence would survive being
deleted, delete it.

## Comments

A comment says **why**, because the code already says what. A comment that restates the
line above it is noise that will one day be wrong.

```rust
// Wrong: says what the line says.
// Increment the counter.
counter += 1;

// Right: says what the reader cannot see.
// The abandoned thread may still answer, and its answer belongs to a call that has
// already been given up on, so the worker is dropped rather than reused.
*slot = None;
```

Three kinds of comment are always worth writing:

1. **The reason for a decision** that a reader would otherwise want to undo.
2. **The measurement behind a choice**: `starting a thread per call cost 70 us of the 77`.
3. **A `SAFETY:` argument** above every `unsafe` block, which
   `clippy::undocumented_unsafe_blocks` requires and which says why the operation is sound.

Every `#[allow]` carries `reason = "..."`, which `clippy::allow_attributes_without_reason`
requires. A lint is never silenced without a written argument, and never at workspace
level when it can be silenced at the one place it fires.

## Documentation comments

Every public item has one; `missing_docs` is a warning and CI turns warnings into errors.

- The first line is a sentence that stands alone, because it is what `cargo doc` lists.
- A function that returns `Result` has an `# Errors` section saying what it returns and
  when.
- An example that would help is a doctest, so that it cannot rot.
- Link to the contract by section when the contract is the reason: "which section 9 of the
  contract states plainly".

## Error messages

This is the part of the repository that matters most, because an error is read by somebody
who is already stuck, and often by an agent that cannot ask anyone.

Every error answers five questions. Four live in the detail entry, the fifth in its hint:

| Question | Where |
| -------- | ----- |
| What was expected | `expected` |
| What arrived | `received` |
| Where | `field`, a JSON pointer when it is a parameter |
| Why | `reason`, and the sentence in `message` |
| What to do about it | `hint` |

The `message` is one sentence of English prose, capitalised, ending in a full stop,
naming the method when a method is known. Written for a person reading a terminal.

The `hint` is in the imperative and shows a call that works when one can be shown:

```text
There is no mesage parameter. Did you mean message? A call that works: {"message":"hi"}.
```

Never write an error that only says something failed. If the code knows the limit, the
message says the limit. If it knows what would have worked, the hint says so. The tests in
`crates/solar-core/src/params.rs` are the worked examples.

## Naming

- An API is `namespace.verb_noun`, and its file, module and struct follow from it with no
  exceptions: `build.run_target` gives `build_run_target.rs`, `mod build_run_target` and
  `struct BuildRunTarget`.
- A test is named as a sentence that says what must be true:
  `a_handler_that_panics_becomes_an_internal_error_and_the_session_lives`. When it fails,
  the name is the bug report.
- Avoid abbreviations. `registry` rather than `reg`, `request` rather than `req`.

## Commit messages

[Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/). The subject is
lower case, imperative, under seventy-two characters:

```text
feat(core): the protocol, the errors and the API template
fix(cli): a way to pass parameters that Windows PowerShell cannot mangle
docs: record the architect's confirmation of the six decisions
```

The types in use are `feat`, `fix`, `docs`, `test`, `build`, `ci`, `style`, `refactor`,
`perf` and `chore`. The scope is the crate or the area: `core`, `apis`, `cli`, `xtask`,
`protocol`, `system.info`.

The body says **why**, in prose, wrapped at seventy-six characters. It is the only place
where the reasoning behind a change survives, so it carries the measurement that prompted
it, the alternative that was rejected, and anything the diff cannot show. A commit that
fixes something says how it was found.

One commit per finished, tested step. `cargo xtask ci` green before pushing.

## What the tools decide, so nobody argues about it

| Thing | Tool |
| ----- | ---- |
| Formatting of Rust | `cargo fmt`, with `rustfmt.toml` |
| Formatting of TOML | `taplo fmt`, with `taplo.toml` |
| Line endings, final newlines, trailing spaces | `.editorconfig` |
| Spelling | `typos`, with `typos.toml` |
| Lints, including pedantic | `clippy`, configured in `Cargo.toml` and `clippy.toml` |

All five run in `cargo xtask ci` and in CI.
