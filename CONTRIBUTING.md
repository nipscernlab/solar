# Contributing to SOLAR

SOLAR is the central API of the Constellation project. It will be used by people who
cannot ask you what you meant, in laboratories that are not this one, so the rules below
are about making that possible rather than about ceremony.

Read [`docs/STYLE.md`](docs/STYLE.md) once. It says how the prose, the comments, the error
messages and the commit messages are written, and most of it is enforced by a tool.

## Setting up

You need Rust. `rust-toolchain.toml` pins the exact version, so `rustup` installs it for
you the first time you build.

```bash no-run
# no-run: a fresh clone, which the machine running the documentation already has
git clone https://github.com/nipscernlab/solar
cd solar
cargo build
```

Five tools are used by the checks. Install them once:

```bash no-run
# no-run: minutes of compilation, and CI installs prebuilt binaries instead
cargo install cargo-nextest cargo-deny cargo-llvm-cov cargo-mutants --locked
cargo install typos-cli taplo-cli --locked
```

## The one command

```bash no-run
# no-run: it is the whole pipeline, and CI runs the same steps as its own
cargo xtask ci
```

It runs what CI runs, in the same order, with the same flags: formatting, TOML
formatting, spelling, lints, tests, doctests, documentation, the manifest check, the
compatibility check, the changelog check, the documentation runner, the local path
check, the supply chain check and the coverage floor. It runs every step even after one fails, and ends with a
table of what passed and what did not.

A test holds it to that: every check appears in both `xtask/src/ci.rs` and
`.github/workflows/ci.yml`, in the same order, and no cargo command in the workflow is
missing `--locked`.

**Run it before you push.** It takes about a minute on a warm build tree.

## Adding an API

One file and one line. The whole procedure is
[`docs/ADDING_AN_API.md`](docs/ADDING_AN_API.md); in short:

```bash no-run
# no-run: writes a file and edits the registry
cargo xtask new-api build.run_target
cargo xtask manifest
```

The generator writes the API from the template, a test file beside it, and the two lines
that register it, in alphabetical order. The generated API compiles and passes the whole
suite before you touch it, so you start from something correct and keep it correct.

The examples in a specification are tests: the suite runs them and compares. Use `$any`
for anything a machine cannot reproduce, and never write a value you have not seen a real
run produce.

## Changing an API that exists

The manifest is the whole surface of SOLAR, and a change to it is a change every caller
sees. `cargo xtask compat main` says what you changed and whether the version answers for
it:

- **additive**, which needs a minor version of that API: a new API, a new optional
  parameter, a new output member, a new declared error;
- **breaking**, which needs a major version: anything removed, renamed or retyped, a
  parameter that becomes required, a declared error withdrawn.

CI runs the same check against the base branch of your pull request and fails when a
breaking change comes without the major bump.

## The rules about dependencies

**A runtime dependency must justify itself in its pull request, with numbers.** Measure
the binary size and the startup time before and after:

```bash no-run
# no-run: a benchmark takes minutes; run it yourself when the answer matters
cargo build --release && ls -l target/release/solar
cargo bench -p solar-cli -- startup
```

Put both figures in the description. SOLAR is started as a child process by every
interface that uses it, so startup is a cost every caller pays on every call, and a
megabyte of binary is a megabyte in every installation.

**Development dependencies are free.** A test runner, a snapshot library, a fuzzer: none
of them ship, so none of them need a measurement.

`cargo deny check` refuses a licence that is not on the list, a source that is not
crates.io, and anything with a RustSec advisory against it.

## Running the slower checks

```bash no-run
# no-run: each of these takes minutes to hours
cargo xtask coverage --report              # lcov and HTML in target/coverage
cargo xtask mutants                        # the full mutation suite
cargo xtask mutants -- --in-diff origin/main   # only what this branch changed
cargo fuzz run envelope -- -max_total_time=60  # needs nightly; see fuzz/
cargo xtask load                           # 10 000 requests through a real session
cargo xtask load --soak                    # a million, to see whether memory grows
```

Coverage says which lines ran. Mutation says whether anything checked what they did: it
changes a line, runs the whole workspace suite, and reports the lines where nothing
failed. A surviving mutant in `solar-core` is a missing test.

`cargo xtask load` needs a release build to measure, and says so if there is none. What it
reports is the latency a client sees, not the time dispatch spends, because that is the
figure a caller can check. What one call allocates is a test rather than a command:
`cargo test -p solar-apis --test heap -- --nocapture` prints it and holds a ceiling.

## Recording a session for a bug report

If SOLAR answers something wrong, record it and attach the file:

```bash no-run
# no-run: an interactive session, which has nobody to talk to in CI
solar serve --stdio --record bug.ndjson
```

Do what produced the wrong answer, then end the session. `bug.ndjson` holds every line in
and every line out with the time each crossed. Whoever picks up the report runs `solar
replay bug.ndjson`, which sends the same requests again and shows where the answers
differ, ignoring what differs between any two runs.

`SOLAR_LOG=trace` adds the diagnostics, and `SOLAR_LOG_FORMAT=json` makes each one a JSON
object if something is collecting them.

## Opening a pull request

The template asks four things, and they are all in `cargo xtask ci` except the judgement
in the last one:

1. `cargo xtask ci` green.
2. The manifest regenerated, if you touched an API.
3. The compatibility check green, and the version bumped if it asked for one.
4. **Code and documentation moved together.** Every change updates, in the same commit,
   every document it affects: the README, the contract, the error catalogue, the guides,
   `STATUS.md` and `CHANGELOG.md`. An outdated document is a defect, like a failing test.
   `cargo xtask changelog` checks the part a machine can see, which is that a change under
   `crates/` came with a changelog entry; the template lists the rest, and you tick it.

Commits follow [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/),
one per finished and tested step, with a body that says **why**. See `docs/STYLE.md`.

Chrysthofer Arthur Amaro Afonso reviews everything: `CODEOWNERS` says so, and he is the
architect of SOLAR. Write the description for him, and for whoever reads it in two years.

## Reporting something exploitable

Not here. [`SECURITY.md`](SECURITY.md) says where.
