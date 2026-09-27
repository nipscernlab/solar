# Testing SOLAR by hand

Written for **Prof. Luciano and Arthur**, who are going to check on a Mac that SOLAR does
what this repository says it does, and who have never used Rust. Nothing here assumes you
have. Every command is meant to be copied exactly, and every one says what you should see.

If a step does something other than what is written here, **that is the finding**. Stop
there and send what the last section asks for; do not work around it.

There are two halves to the testing:

1. **The automated checks**, which are this guide. One command runs everything the
   machines check, and a short list of calls shows the binary answering.
2. **The interactive test**, which is ZENITH, the terminal of the project, in
   [`nipscernlab/zenith`](https://github.com/nipscernlab/zenith). SOLAR has no interface
   of its own beyond `solar` on a command line: ZENITH is how a person drives it. Its own
   guide is the one to follow, and this guide does not repeat it.

   *Checked on 27 September 2026:* ZENITH is a Rust project with `docs/DESIGN.md`,
   `docs/STYLE.md`, `docs/OPEN_QUESTIONS.md` and `docs/adr/`, and it does not yet carry a
   guide for testing by hand. When it does, that is the one to follow for this half. Ask
   Chrysthofer which revision of ZENITH to test against.

---

## 1. Install the toolchain

Rust installs into your own home directory. It touches nothing that belongs to the system,
needs no administrator, and is removed with one command.

```bash no-run
# no-run: it installs a toolchain, which CI already has
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Accept the default installation when it asks. Then, in a **new** terminal:

```bash no-run
# no-run: the version depends on the machine, and CI pins its own
rustc --version
cargo --version
```

Both should print a version. If the terminal says `command not found`, open a new terminal
first: the installer adds `~/.cargo/bin` to your `PATH`, and a terminal that was already
open does not know about it yet.

> **Mac only.** Building needs the Apple command line developer tools, which most Macs
> already have. If a build later stops with `linker 'cc' not found` or `xcrun: error`, run
> `xcode-select --install`, accept the dialogue, and build again. This is the one step
> that may ask for your password, and it is Apple's installer asking, not SOLAR.

SOLAR pins the exact Rust version it is built with, in `rust-toolchain.toml`. The first
`cargo` command inside the repository downloads that version by itself. You do not have to
choose anything.

---

## 2. Get the code

```bash no-run
# no-run: CI already has the repository it is running in
git clone https://github.com/nipscernlab/solar.git
cd solar
```

Every command from here on is run from inside that `solar` directory.

---

## 3. Build it

```bash no-run
# no-run: CI builds with its own flags
cargo build --release --locked
```

The first build downloads the dependencies and takes a few minutes; later ones take
seconds. `--locked` means the exact dependency versions recorded in the repository, so
what you build is what CI built.

**What you should see:** a last line like `Finished `release` profile [optimized +
debuginfo] target(s) in 1m 04s`. Warnings from dependencies are normal. A line beginning
`error` is not: that is a finding.

The binary is now at `./target/release/solar`.

---

## 4. Run every check, with one command

```bash no-run
# no-run: it is the whole pipeline, and CI runs the same steps as its own
cargo xtask ci
```

This is the same list of checks CI runs, in the same order, and a test holds the two
together so they cannot drift apart. It takes several minutes, mostly in the last step. It
runs every step even after one fails, and ends with a table.

**What you should see:** a table where every row says `ok`, and a last line saying
everything passed.

Each row, and what a failure there would mean:

| Step | What it checks | A failure means |
| ---- | -------------- | ---------------- |
| formatting | The code is formatted as `rustfmt` formats it | Someone committed unformatted code |
| toml formatting | The same for the `.toml` files | The same |
| spelling | `typos`, in British English | A misspelling, or a word the dictionary does not know |
| lints | `clippy`, with warnings treated as errors | A lint fired, or a new compiler warning |
| tests | The whole suite, through `cargo nextest` | **The important one.** A promise of the contract is broken |
| doctests | The examples inside the documentation comments | An example in the source does not run |
| documentation | `cargo doc` builds with no warnings | A broken link in the documentation |
| manifest | `manifest/solar.manifest.json` is what the generator produces | Someone edited it by hand, or forgot to regenerate it |
| compatibility | The manifest against `main`, and whether versions answer for what changed | A change that a caller would notice, without the version bump it needs |
| changelog | A change under `crates/` came with a `CHANGELOG.md` entry | Code moved without its documentation |
| documentation runs | Every shell block in the README and `docs/` really runs | The documentation claims something the binary does not do |
| no local paths | The release binary embeds no home directory or user name | A build that leaks where it was built |
| supply chain | `cargo deny`: advisories, licences, sources | A dependency with a known vulnerability or a licence that does not fit |
| coverage | How much of the shipped code the tests run, against a floor | Coverage fell below the floor |

Some steps need a tool that is not part of Rust itself: `typos`, `taplo`, `cargo-nextest`,
`cargo-deny` and `cargo-llvm-cov`. `cargo xtask ci` says which one is missing and the
exact `cargo install` line for it. Install what it names and run it again, or use the
faster form while you are setting up:

```bash no-run
# no-run: it deliberately skips two of the steps above
cargo xtask ci --fast
```

which skips the documentation runner and the coverage step, the two slowest, and says so
at the end. It is not the gate: run the full `cargo xtask ci` at least once.

---

## 5. Make the binary answer

These are the calls worth making by hand. Run each one and compare with what is written
here. The shell on a Mac is `zsh` or `bash`, and both take the single quotes below as they
are written.

### It is there

```bash
./target/release/solar call solar.ping '{"message":"hello from the Mac"}'
```

One line of JSON, holding `"pong":true`, `"echo":"hello from the Mac"`, and a `meta` block
naming the version, the operating system and how long the call took in microseconds.

### What it is

```bash
./target/release/solar version
```

Eight lines: the SOLAR version, the protocol `solar/1`, the manifest schema version, the
commit it was built from, whether the working tree was dirty, the profile, the target
triple and the compiler. On a Mac the target says `aarch64-apple-darwin` on Apple silicon,
or `x86_64-apple-darwin` on an Intel Mac.

### What it can do

```bash
./target/release/solar list
```

The APIs this build answers to, one per line, with a version and a one line summary, then
three lines telling you what to try next.

### What one of them is, in detail

```bash
./target/release/solar describe system.info
```

The name, the version, the prose description, the stability, the timeout, the parameters
with their types, the failures it may return, and its examples, each shown for `bash`,
`powershell` and `cmd`.

### The machine it is running on

```bash
./target/release/solar call system.info '{}'
```

> **Mac only, and the reason you are being asked.** Look at `os_name`, `os_release` and
> `os_build`. On a Mac they are read from `/System/Library/CoreServices/SystemVersion.plist`
> and should say `macOS`, the release you are running, such as `15.5`, and the build, such
> as `24F74`. Compare them with the Apple menu, About This Mac. **If any of the three is
> `null`, there will be a warning in `warnings` saying which file could not be read:
> that is a finding, and the exact output is what to send back.** This is the one part of
> SOLAR that no machine of ours can check, because only a Mac can say what a Mac calls
> itself.

### An error, which is where SOLAR is meant to be good

```bash
./target/release/solar call solar.ping '{"mesage":"typo on purpose"}' || echo "exit $?"
```

An error envelope, not a crash. It should name the field, say what was expected, say what
arrived, suggest `message`, give a call that works, and point at
`docs/ERRORS.md#invalid_argument`. The process then exits with code **2**, which is what
`INVALID_ARGUMENT` maps to, and the `|| echo` above prints it. Every status has an exit
code of its own; section 13 of [`CONTRACT.md`](CONTRACT.md) is the table.

### A session, which is how everything except the command line talks to SOLAR

```bash no-run
# no-run: it reads from a terminal and waits for you
./target/release/solar serve --stdio
```

It waits. Paste one line and press Return:

```json
{"jsonrpc": "2.0", "id": 1, "method": "solar.ping"}
```

One line comes back. Paste another with a different `id`; it answers again. **Ctrl+D** ends
the session.

Two things are worth trying here, because they are new in this version:

- **A batch.** Paste an array of requests on one line:

  ```json
  [{"jsonrpc":"2.0","id":1,"method":"solar.ping"},{"jsonrpc":"2.0","id":2,"method":"solar.version"}]
  ```

  One line comes back, holding an array of two responses, in the order you asked.

- **A cancellation.** `solar.cancel` answers about a call by its `id`:

  ```json
  {"jsonrpc":"2.0","id":9,"method":"solar.cancel","params":{"id":1}}
  ```

  Because call `1` finished long ago, the outcome is `already_finished`. An id nothing has
  used answers `unknown`. Neither is an error: both are answers.

---

## 6. What was checked where

Honesty about what has actually been run, so that you know what you are the first to try.

| Step | Windows 11 | Linux, AlmaLinux 9 under WSL | macOS |
| ---- | ---------- | ----------------------------- | ----- |
| Install with `rustup` | yes | yes | **only a Mac can confirm** |
| `cargo build --release --locked` | yes | yes | **only a Mac can confirm** |
| `cargo xtask ci` | yes | see `STATUS.md` | **only a Mac can confirm** |
| The calls of section 5 | yes | yes | **only a Mac can confirm** |
| `system.info` naming the system | yes, `Windows 11 Home Single Language` | yes | **only a Mac can confirm**, and it is the reason this guide exists |
| The Apple developer tools | not applicable | not applicable | **only a Mac can confirm** |

CI has run the whole suite on macOS since the first stage, on GitHub's runners. What it
cannot do is tell us whether the guide above works for a person: that is what you are
checking.

---

## 7. When something fails, send this

Send it to Chrysthofer, at chrysthofer.afonso@cern.ch, or open an issue on the repository.
**Do not summarise the output. Paste it.** A summary of an error loses the part that
matters more often than not.

1. **Which step**, by its number and title in this guide.
2. **The command you ran**, copied exactly, and **everything it printed**, copied exactly.
   If it is long, the last fifty lines are enough to start with.
3. **The versions**, which is one command:

   ```bash
   ./target/release/solar version
   ```

   and, if the build itself failed so there is no binary yet:

   ```bash no-run
   # no-run: the versions of the machine reporting the problem
   rustc --version && cargo --version && uname -a && sw_vers
   ```

4. **`system.info`**, whenever the problem is about the machine rather than about a call:

   ```bash
   ./target/release/solar call system.info '{}'
   ```

5. **A recording, if a session misbehaved.** This writes every line in and every line out,
   with the time each one crossed:

   ```bash no-run
   # no-run: it records an interactive session
   ./target/release/solar serve --stdio --record session.ndjson
   ```

   Do the thing that went wrong, press Ctrl+D, and send `session.ndjson`. It can be
   replayed here against any build with `solar replay session.ndjson`, which is the
   difference between a bug we can see and a bug we can only imagine.

**What not to send:** the whole `target/` directory, which is hundreds of megabytes and
holds nothing we do not already have.

---

## 8. Removing it afterwards

```bash no-run
# no-run: it removes the toolchain
rustup self uninstall
```

and delete the `solar` directory. Nothing else was installed and nothing outside your home
directory was touched.
