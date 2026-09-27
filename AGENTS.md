# Working on SOLAR

The single entry point, for a person or for a coding agent. Read this once before
changing anything. It is short on purpose: it says what SOLAR is, the one path for adding
an API, and what the machines enforce. Everything else is linked, never repeated here.

## What SOLAR is

The central API of the Constellation project, at NIPS-CERN. Every interface, the `solar`
command line interface, the ZENITH terminal, the AURORA IDE and artificial intelligence
agents, talks to SOLAR and to nothing else. Everything behind it is an implementation
detail that callers must not depend on.

It speaks JSON-RPC 2.0 over standard input and output, one message per line. There is no
port, no daemon and no network listener.

Three promises hold the design together, and every rule below serves one of them:

1. **Every call gets a response.** Success or failure, always. A handler that panics
   becomes `INTERNAL`, one that overruns becomes `DEADLINE_EXCEEDED`, and the session
   carries on. No path ends in silence.
2. **A response carries as much context as it can.** Every error says what was expected,
   what arrived, where, why and what to do. The audience includes agents, which cannot
   ask a colleague what an error meant.
3. **One template, no exceptions.** Every API is declared the same way, and what deviates
   is refused by a test rather than by a reviewer.

## The fixed rule: code and documentation move together

Decided on 27 September 2026, for every project of the laboratory.

**Every change updates, in the same commit, every document it affects:** the README, the
contract, the error catalogue, the guides, `STATUS.md` and `CHANGELOG.md`. An outdated
document is a defect, exactly like a failing test, and it is worse than no document
because a reader trusts it.

What a machine can check, it checks: a change under `crates/` comes with a `CHANGELOG.md`
entry, every shell block in the documentation really runs, and the manifest is what the
generator produces. The rest is on the pull request template, which lists it so that
nobody has to remember it.

## The one path for adding an API

```bash no-run
# no-run: writes a file and edits the registry
cargo xtask new-api build.run_target
```

That writes the API from the template, a test file beside it, and the two lines that
register it. The generated API compiles and passes the whole suite before you touch it.

**[`docs/ADDING_AN_API.md`](docs/ADDING_AN_API.md) is the detailed path**, step by step,
including what belongs in a specification and how examples become tests. Follow it. This
file does not repeat it.

Four things are true whoever, or whatever, is writing:

- An API is registered in **one** explicit list, `crates/solar-apis/src/lib.rs`. There is
  no discovery and no macro that finds things.
- The file, the module and the struct follow from the method name by a rule with no
  exceptions: `build.run_target` gives `build_run_target.rs`, `mod build_run_target`,
  `struct BuildRunTarget`.
- `manifest/solar.manifest.json` is generated. Never edit it by hand; run
  `cargo xtask manifest`.
- The examples in a specification are tests. Never write a value you have not seen a real
  run produce; use `"$any"` for what a machine cannot reproduce.

## The one command

```bash no-run
# no-run: it is the whole pipeline, and CI runs the same steps as its own
cargo xtask ci
```

It runs what CI runs, in the same order, with the same flags, and a test holds the two
together. Run it before every push. `cargo xtask ci --fast` skips the two slowest steps
for the loop between commits; it is not the gate.

## What the machines enforce, so you do not have to remember it

| Rule | What refuses you |
| ---- | ---------------- |
| The naming rule, a complete specification, at least one example | `crates/solar-apis/tests/contract.rs` |
| Parameters refuse unknown members | the same, and `#[serde(deny_unknown_fields)]` |
| Every example really runs and matches | the same |
| Every declared error exists in `docs/ERRORS.md` | the same, and `crates/solar-core/tests/docs.rs` |
| The manifest is what the generator produces | `cargo xtask manifest --check` |
| A breaking change carries a major bump of its API | `cargo xtask compat` |
| A change under `crates/` carries a changelog entry | `cargo xtask changelog` |
| No `unwrap`, `expect` or `panic!` outside tests | clippy, denied at workspace level |
| Every `allow` carries a reason, every `unsafe` block a `SAFETY` comment | clippy |
| British English | `typos`, with `locale = "en-gb"` |
| Every shell block in the documentation really runs | `cargo xtask doc-run` |
| The release binary embeds no local path | `cargo xtask leak-check` |
| Coverage of the shipped crates | `cargo xtask coverage`, floor in the source |
| Lines nothing checks | `cargo xtask mutants`, ceiling in the source |
| What one call allocates | `crates/solar-apis/tests/heap.rs`, ceiling in the source |

## Where the truth lives

| Question | File |
| -------- | ---- |
| What the protocol promises | [`docs/CONTRACT.md`](docs/CONTRACT.md), normative |
| What an error means | [`docs/ERRORS.md`](docs/ERRORS.md) |
| How to add an API | [`docs/ADDING_AN_API.md`](docs/ADDING_AN_API.md) |
| How to write it | [`docs/STYLE.md`](docs/STYLE.md) |
| How to test it by hand, for somebody new | [`docs/TESTING_BY_HAND.md`](docs/TESTING_BY_HAND.md) |
| Why it is the way it is | [`docs/adr/`](docs/adr/) |
| What is still undecided | [`docs/OPEN_QUESTIONS.md`](docs/OPEN_QUESTIONS.md) |
| How to set up, and the rules for dependencies | [`CONTRIBUTING.md`](CONTRIBUTING.md) |
| Where the project stands | [`STATUS.md`](STATUS.md) |

## What never to do

- Never invent a fact about SOLAR, about a measurement, or about what a tool does. Check
  it, or say you did not.
- Never silence a lint without `reason = "..."` saying why.
- Never exclude code from mutation testing to make the number look better. Kill the
  mutant with a test of observable behaviour, or explain the equivalence where
  `cargo-mutants` can see it.
- Never add a runtime dependency without measuring its effect on binary size and startup
  time, in the pull request. Development dependencies are free.
- Never change the shape of an existing response without the version bump `cargo xtask
  compat` asks for.
- Never leave a document behind. If the change makes a sentence in `docs/` or the README
  untrue, the same commit fixes it.
