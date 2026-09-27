## What this changes, and why

<!-- The why matters more than the what: the diff already says what. If a measurement
     prompted this, put the number here. If you rejected an alternative, say which. -->

## The checklist

- [ ] `cargo xtask ci` is green on my machine
- [ ] `cargo xtask manifest` run and the result committed, if an API changed
- [ ] `cargo xtask compat main` is green, and the version of any API I changed answers
      for the change: a minor bump for anything added, a major one for anything removed,
      renamed, retyped or made required
- [ ] `CHANGELOG.md` and the documentation say what changed
- [ ] A new runtime dependency, if there is one, is justified above with its measured
      effect on binary size and startup time. Development dependencies need no
      measurement.

## What I tested, and how

<!-- Not "it works". Which case, run how, with what result. A test that fails without
     your change and passes with it is the strongest thing you can write here. -->
