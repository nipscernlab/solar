//! The SOLAR mark, drawn in terminal cells.
//!
//! The rules come from `docs/brand/README.md` and are followed here to the letter:
//!
//! 1. The mark appears only in output meant for a person. It never reaches the NDJSON
//!    session, the envelope printed by `solar call`, or anything that goes into a pipe.
//! 2. It is coloured only on a terminal, and never when `NO_COLOR` holds a value.
//! 3. The escape codes are the three the guide gives, chosen from the environment.
//! 4. The version comes from the crate version, never from a literal.
//!
//! The two drawings are the files in `docs/brand`, compiled in so that a copy of the
//! binary carries its own mark. Editors strip trailing spaces, so every row is padded back
//! to sixteen cells here rather than trusted.

use std::io::IsTerminal;

/// The symbol in half blocks, for a terminal that speaks UTF-8.
const BLOCKS: &str = include_str!("../../../docs/brand/banner.txt");

/// The same drawing in 7-bit ASCII, for a terminal that does not.
const ASCII: &str = include_str!("../../../docs/brand/banner-ascii.txt");

/// How many cells wide the symbol is.
const WIDTH: usize = 16;

/// How many cells separate the symbol from the text beside it.
const GAP: usize = 3;

/// Gold, `#FFC23D`, in the three depths a terminal may understand.
const TRUECOLOR: &str = "\x1b[38;2;255;194;61m";
const XTERM_256: &str = "\x1b[38;5;215m";
const ANSI_16: &str = "\x1b[33m";
const RESET: &str = "\x1b[0m";

/// The eight rows of the symbol, each one exactly [`WIDTH`] cells wide.
fn rows() -> Vec<String> {
    let drawing = if wants_ascii() { ASCII } else { BLOCKS };
    drawing
        .lines()
        .take(8)
        .map(|row| {
            let mut row = row.to_owned();
            let cells = row.chars().count();
            if cells < WIDTH {
                row.push_str(&" ".repeat(WIDTH - cells));
            }
            row
        })
        .collect()
}

/// Whether the ASCII drawing was asked for.
///
/// There is no reliable way to ask a terminal whether it can draw a half block, so this is
/// a switch rather than a guess: `SOLAR_ASCII` with any value picks the ASCII drawing.
fn wants_ascii() -> bool {
    std::env::var_os("SOLAR_ASCII").is_some_and(|value| !value.is_empty())
}

/// Whether the mark may be coloured: a terminal, and no `NO_COLOR`.
#[must_use]
pub(crate) fn wants_colour() -> bool {
    std::io::stdout().is_terminal()
        && !std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty())
}

/// The escape code for the deepest colour this terminal is known to understand.
fn gold() -> &'static str {
    let colorterm = std::env::var("COLORTERM").unwrap_or_default();
    if colorterm.contains("truecolor") || colorterm.contains("24bit") {
        return TRUECOLOR;
    }
    // Windows Terminal renders 24-bit colour and does not announce it through COLORTERM.
    if std::env::var_os("WT_SESSION").is_some() {
        return TRUECOLOR;
    }
    if std::env::var("TERM")
        .unwrap_or_default()
        .contains("256color")
    {
        return XTERM_256;
    }
    ANSI_16
}

/// The mark with `text` laid out beside it, one line per row.
///
/// The text starts at the column the guide fixes, and the line whose slot opens to the
/// right carries the first line of it.
#[must_use]
pub(crate) fn beside(text: &[String], colour: bool) -> String {
    let (on, off) = if colour { (gold(), RESET) } else { ("", "") };
    let first_text_row = 2;

    rows()
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            let beside = index
                .checked_sub(first_text_row)
                .and_then(|line| text.get(line))
                .map(String::as_str)
                .unwrap_or_default();
            if beside.is_empty() {
                format!("{on}{}{off}\n", row.trim_end())
            } else {
                format!("{on}{row}{off}{}{beside}\n", " ".repeat(GAP))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_drawings_are_eight_rows_of_sixteen_cells() {
        for drawing in [BLOCKS, ASCII] {
            let lines: Vec<&str> = drawing.lines().take(8).collect();
            assert_eq!(lines.len(), 8, "the mark is eight rows");
            for line in lines {
                assert!(
                    line.chars().count() <= WIDTH,
                    "a row is wider than the mark: {line:?} in {drawing:?}"
                );
            }
        }
        assert!(rows().iter().all(|row| row.chars().count() == WIDTH));
    }

    #[test]
    fn the_text_starts_at_the_column_the_guide_fixes() {
        let text = vec!["SOLAR".to_owned()];
        let drawn = beside(&text, false);
        let line = drawn.lines().nth(2).unwrap();
        let at = line
            .char_indices()
            .find(|(_, c)| *c == 'S')
            .map(|(index, _)| index);
        assert_eq!(line.chars().take_while(|c| *c != 'S').count(), WIDTH + GAP);
        assert!(at.is_some());
    }

    #[test]
    fn without_colour_there_is_not_one_escape_code() {
        let drawn = beside(&["SOLAR".to_owned()], false);
        assert!(
            !drawn.contains('\x1b'),
            "a pipe must never see an escape code"
        );
    }

    #[test]
    fn with_colour_every_row_is_wrapped_and_closed() {
        let drawn = beside(&["SOLAR".to_owned()], true);
        for line in drawn.lines() {
            assert!(
                line.contains("\x1b["),
                "a row was left uncoloured: {line:?}"
            );
            assert!(line.contains(RESET), "a row was left open: {line:?}");
        }
    }

    #[test]
    fn a_row_with_no_text_beside_it_carries_no_trailing_spaces() {
        let drawn = beside(&[], false);
        for line in drawn.lines() {
            assert_eq!(line, line.trim_end(), "trailing spaces on {line:?}");
        }
    }
}
