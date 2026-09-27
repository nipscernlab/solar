# The SOLAR mark

**Status:** chosen on 2026-09-26. **Source:** the Claude Design canvas "SOLAR · logo", page "Rodada 2" (private to the owner).

This folder is the single source for the SOLAR symbol, its colours and its terminal form. Code that draws the mark reads from here; nothing here is generated at build time.

## 1. Symbol

A solid disc cut by two horizontal slots. A message enters through one slot and an action leaves through the other; what remains of the disc between them is the S.

Geometry, in the 48 × 48 viewBox of every file in `svg/`:

| Rule | Value |
| --- | --- |
| Disc | centre (24, 24), radius 22 |
| Module | m = diameter ÷ 8 = 5.5 |
| Bands and slots, top to bottom | 2m, 1m, 2m, 1m, 2m |
| Upper slot | open to the right, stops 1.5m left of the vertical axis |
| Lower slot | open to the left, stops 1.5m right of the vertical axis |
| Symmetry | a 180° rotation gives the same drawing; a mirror image is not an S and is never used |

The mark is a single flat colour. It has no gradient, outline, glow or shadow, and it is never stretched, mirrored or enclosed in a ring.

| File | Use |
| --- | --- |
| `svg/symbol-gold.svg` | on dark backgrounds, 24 px and up |
| `svg/symbol-copper.svg` | on light backgrounds, 24 px and up |
| `svg/symbol-ink.svg`, `svg/symbol-white.svg` | one-colour print and engraving |
| `svg/symbol-16px-gold.svg`, `svg/symbol-16px-ink.svg` | below 24 px: a separate pixel drawing, m = 2 px, `shape-rendering="crispEdges"` |
| `svg/lockup-dark.svg`, `svg/lockup-light.svg` | symbol with the name |

In the lockups the name is Martian Mono SemiBold (SIL OFL 1.1), converted to outlines, so no font is needed to render them. The cap height runs from the top of the upper slot to the bottom of the lower one. Keep 2m clear around the lockup.

## 2. Colours

| Name | Hex | Role |
| --- | --- | --- |
| Gold | `#FFC23D` | the symbol on dark |
| Copper | `#B35A00` | the symbol on light, one-colour print |
| Night | `#0B0C14` | dark background |
| Mist | `#F5F1E8` | light background, the name on dark |
| Ink | `#17161C` | the name on light, text |

WCAG contrast, computed: gold on night 12.10:1, mist on night 17.30:1, copper on mist 4.25:1, copper on white 4.80:1, ink on mist 15.95:1. Gold on a light background is 1.43:1 and is never used.

## 3. Terminal

`banner.txt` is the symbol in 16 × 8 terminal cells. It uses only `█`, `▀`, `▄` and the space, and every half block is one pixel of the 16 px icon. Every line is 16 characters wide, trailing spaces included. Editors tend to strip those spaces, so code that reads the file pads each line to 16 on the right instead of trusting it. `banner-ascii.txt` is the same drawing in 7-bit ASCII, for terminals without UTF-8.

With text beside it, leave three spaces after the symbol, so text starts at column 19 (0-based). Line 3, the one whose slot opens to the right, carries the name:

```text
   ▄▄██████▄▄
 ▄████████████▄
▄████              SOLAR
████████████████   The central API of the Constellation
████████████████   NIPS-CERN
           ████▀   0.2.0
 ▀████████████▀
   ▀▀██████▀▀
```

Rules for the CLI:

1. **Only in output meant for a person.** The banner never reaches a machine-readable channel: the NDJSON stream of the stdio server, the envelope printed by `solar call`, or any output that is piped. The contract in `docs/CONTRACT.md` requires that output to be exact.
2. **Colour only on a terminal.** Colour the whole symbol in gold when stdout is a terminal and `NO_COLOR` is absent or empty ([no-color.org](https://no-color.org)). Otherwise print it without escape codes.
3. **Escape codes.** Truecolor `ESC[38;2;255;194;61m`, 256-colour fallback `ESC[38;5;215m`, 16-colour fallback `ESC[33m`, reset `ESC[0m`.
4. **Version.** The version line comes from `env!("CARGO_PKG_VERSION")`, never from a literal.

Reference implementation. It compiles with rustc 1.95, edition 2024 and `-D warnings -W unreachable_pub -W missing_docs`. It has not been built inside this workspace (rust-version 1.97), so adapt it to the crate's layout and lints:

```rust
use std::io::IsTerminal;

/// The SOLAR symbol, 16 × 8 terminal cells. Source: docs/brand/banner.txt.
const SYMBOL: [&str; 8] = [
    "   ▄▄██████▄▄   ",
    " ▄████████████▄ ",
    "▄████           ",
    "████████████████",
    "████████████████",
    "           ████▀",
    " ▀████████████▀ ",
    "   ▀▀██████▀▀   ",
];

const GOLD: &str = "\x1b[38;2;255;194;61m";
const RESET: &str = "\x1b[0m";

/// The symbol, one row per line, in gold when `colour` is true.
pub(crate) fn symbol(colour: bool) -> String {
    let (on, off) = if colour { (GOLD, RESET) } else { ("", "") };
    SYMBOL.iter().map(|row| format!("{on}{row}{off}\n")).collect()
}

/// Colour only on a terminal, and never when NO_COLOR is set to a non-empty value.
pub(crate) fn wants_colour() -> bool {
    std::io::stdout().is_terminal()
        && !std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty())
}
```

## 4. Open items

- No trademark search has been made. Search INPI and the WIPO Global Brand Database before registering the mark or printing it on hardware.
- The cool AURORA tones that appeared in the first round were approximations and are not part of this mark.
