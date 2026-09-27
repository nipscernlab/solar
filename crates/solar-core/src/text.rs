//! Small text utilities the protocol needs: method names, suggestions, truncation.

/// The largest edit distance at which a name is still offered as a suggestion.
pub const MAX_SUGGESTION_DISTANCE: usize = 3;

/// How many suggestions an error carries at most.
pub const MAX_SUGGESTIONS: usize = 3;

/// Whether a name matches `^[a-z]+(\.[a-z]+(_[a-z]+)*)+$`, the naming rule of section 4.
///
/// The rule is checked by hand rather than with a regular expression engine, because it is
/// the only pattern SOLAR needs and it is checked on every call.
#[must_use]
pub fn is_valid_method_name(name: &str) -> bool {
    let mut segments = name.split('.');
    let Some(first) = segments.next() else {
        return false;
    };
    if !is_lowercase_word(first) {
        return false;
    }
    let mut has_second_segment = false;
    for segment in segments {
        has_second_segment = true;
        if segment.is_empty() {
            return false;
        }
        if !segment.split('_').all(is_lowercase_word) {
            return false;
        }
    }
    has_second_segment
}

/// A non empty run of ASCII lowercase letters.
fn is_lowercase_word(word: &str) -> bool {
    !word.is_empty() && word.bytes().all(|b| b.is_ascii_lowercase())
}

/// Drops a byte order mark from the front of a string.
///
/// Nothing should send one: RFC 8259 forbids adding a byte order mark to JSON. Windows
/// PowerShell adds one anyway, to the first thing it writes into the standard input of a
/// native program, which used to cost a caller its first request. The same RFC allows a
/// parser to ignore one, so SOLAR ignores it.
#[must_use]
pub fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

/// The Levenshtein edit distance between two strings, counted in characters.
///
/// The implementation keeps two rows rather than the whole matrix, so the cost is linear in
/// memory and quadratic in time, which is nothing at the length of a method name.
#[must_use]
pub fn levenshtein(left: &str, right: &str) -> usize {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    if left.is_empty() {
        return right.len();
    }
    if right.is_empty() {
        return left.len();
    }

    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0usize; right.len() + 1];

    for (i, left_char) in left.iter().enumerate() {
        current[0] = i + 1;
        for (j, right_char) in right.iter().enumerate() {
            let substitution = previous[j] + usize::from(left_char != right_char);
            let insertion = current[j] + 1;
            let deletion = previous[j + 1] + 1;
            current[j + 1] = substitution.min(insertion).min(deletion);
        }
        std::mem::swap(&mut previous, &mut current);
    }

    previous[right.len()]
}

/// The closest candidates to `input`, nearest first, at most [`MAX_SUGGESTIONS`] of them.
///
/// A candidate is only offered when the distance is at most [`MAX_SUGGESTION_DISTANCE`] and
/// strictly smaller than the length of the candidate itself, so that a short name does not
/// attract every other short name in the registry. Ties are broken alphabetically, which
/// makes the output of an error reproducible.
#[must_use]
pub fn suggestions<'a, I>(input: &str, candidates: I) -> Vec<(&'a str, usize)>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut scored: Vec<(&'a str, usize)> = candidates
        .into_iter()
        .map(|candidate| (candidate, levenshtein(input, candidate)))
        .filter(|(candidate, distance)| {
            *distance <= MAX_SUGGESTION_DISTANCE && *distance < candidate.chars().count()
        })
        .collect();
    scored.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(b.0)));
    scored.truncate(MAX_SUGGESTIONS);
    scored
}

/// Cuts a string to at most `max_bytes`, never in the middle of a character.
///
/// Returns the piece that was kept and whether anything was dropped.
#[must_use]
pub fn truncate(text: &str, max_bytes: usize) -> (String, bool) {
    if text.len() <= max_bytes {
        return (text.to_owned(), false);
    }
    // `is_char_boundary(0)` is always true, so this walks back to at worst zero and
    // stops. A guard on `end > 0` would be redundant, and nothing could ever prove it
    // was doing anything.
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text.get(..end).unwrap_or_default().to_owned(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_names_of_the_six_apis_are_valid() {
        for name in [
            "solar.ping",
            "solar.version",
            "solar.manifest",
            "solar.describe",
            "system.info",
        ] {
            assert!(is_valid_method_name(name), "{name} should be valid");
        }
        assert!(is_valid_method_name("tools.detect"));
        assert!(is_valid_method_name("build.run_target"));
        assert!(is_valid_method_name("a.b.c"));
    }

    #[test]
    fn names_outside_the_rule_are_rejected() {
        for name in [
            "",
            "solar",
            "solar.",
            ".ping",
            "solar..ping",
            "Solar.ping",
            "solar.Ping",
            "solar.ping1",
            "solar.pi-ng",
            "solar._ping",
            "solar.ping_",
            "solar.ping__twice",
            "so_lar.ping",
            "solar ping",
            "solar.ping ",
        ] {
            assert!(!is_valid_method_name(name), "{name:?} should be rejected");
        }
    }

    #[test]
    fn the_edit_distance_matches_the_textbook() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("flaw", "lawn"), 2);
        assert_eq!(levenshtein("solar.ping", "solar.ping"), 0);
        assert_eq!(levenshtein("solar.pnig", "solar.ping"), 2);
    }

    #[test]
    fn deleting_and_inserting_cost_the_same_one_each() {
        // The three terms of the recurrence are substitution, insertion and deletion, and
        // a case that needs deletions is the only thing that proves the third is there:
        // without it these come out one too large.
        assert_eq!(levenshtein("ab", "a"), 1, "one deletion");
        assert_eq!(levenshtein("a", "ab"), 1, "one insertion");
        assert_eq!(
            levenshtein("abcde", "ae"),
            3,
            "three deletions in the middle"
        );
        assert_eq!(
            levenshtein("ae", "abcde"),
            3,
            "and the same the other way round"
        );
        assert_eq!(levenshtein("deletion", "dton"), 4);
    }

    #[test]
    fn a_substitution_costs_one_and_a_match_costs_nothing() {
        // The mutation that replaces + with * in the substitution cost survives unless a
        // case distinguishes them: with * a match would cost zero and a substitution
        // would cost zero too, collapsing every distance that is all substitutions.
        assert_eq!(levenshtein("abc", "abd"), 1, "one substitution at the end");
        assert_eq!(
            levenshtein("abc", "xyz"),
            3,
            "three substitutions, no insertions"
        );
        assert_eq!(levenshtein("aaa", "aaa"), 0);
        assert_eq!(
            levenshtein("ab", "ba"),
            2,
            "a swap is two substitutions here"
        );
    }

    #[test]
    fn a_candidate_is_offered_only_when_both_conditions_hold() {
        // Two conditions, so two cases where exactly one of them fails. Either of them
        // alone, or an || in place of the &&, would offer one of these.
        let far_but_long = suggestions("abcdefgh", ["abcdwxyz"]);
        assert!(
            far_but_long.is_empty(),
            "distance 4 is past the limit, however long the name"
        );

        // Distance 2 on a candidate of length 2: within the limit, and not shorter than
        // the candidate, so it is refused by the second condition alone.
        let close_but_short = suggestions("xy", ["ab"]);
        assert!(
            close_but_short.is_empty(),
            "a name cannot be a suggestion for something wholly unlike it"
        );

        // Distance 2 on a candidate of length 3 passes both, and must be offered.
        assert_eq!(
            suggestions("xyz", ["xab"]).first().map(|(name, _)| *name),
            Some("xab")
        );
    }

    #[test]
    fn the_distance_limit_is_inclusive_and_the_length_limit_is_not() {
        // Exactly at MAX_SUGGESTION_DISTANCE: offered, so <= is not <.
        let at_the_limit = suggestions("abcdef", ["abcxyz"]);
        assert_eq!(
            at_the_limit.len(),
            1,
            "a distance of exactly three is still a suggestion"
        );

        // Distance equal to the length of the candidate: refused, so < is not <=.
        let as_far_as_it_is_long = suggestions("abc", ["xyz"]);
        assert!(as_far_as_it_is_long.is_empty(), "every character differs");
    }

    #[test]
    fn the_edit_distance_counts_characters_not_bytes() {
        assert_eq!(levenshtein("é", "e"), 1);
        assert_eq!(levenshtein("café", "cafe"), 1);
    }

    #[test]
    fn a_typo_suggests_the_name_it_meant() {
        let known = [
            "solar.ping",
            "solar.version",
            "solar.manifest",
            "tools.detect",
        ];
        let found = suggestions("solar.pign", known);
        assert_eq!(found.first().map(|(name, _)| *name), Some("solar.ping"));
    }

    #[test]
    fn something_unrelated_suggests_nothing() {
        let known = ["solar.ping", "tools.detect"];
        assert!(suggestions("completely.different_thing", known).is_empty());
    }

    #[test]
    fn at_most_three_suggestions_come_back_and_the_order_is_stable() {
        let known = ["a.aa", "a.ab", "a.ac", "a.ad", "a.ae"];
        let found = suggestions("a.aa", known);
        assert_eq!(found.len(), MAX_SUGGESTIONS);
        assert_eq!(found[0], ("a.aa", 0));
        assert_eq!(found[1].0, "a.ab");
        assert_eq!(found[2].0, "a.ac");
    }

    #[test]
    fn a_byte_order_mark_at_the_front_is_ignored() {
        assert_eq!(strip_bom("\u{feff}{\"id\":1}"), "{\"id\":1}");
        assert_eq!(strip_bom("{}"), "{}");
        assert_eq!(strip_bom(""), "");
        // Only at the front, and only one of them.
        assert_eq!(strip_bom("a\u{feff}b"), "a\u{feff}b");
        assert_eq!(strip_bom("\u{feff}\u{feff}x"), "\u{feff}x");
    }

    #[test]
    fn truncation_keeps_whole_characters() {
        assert_eq!(truncate("hello", 10), ("hello".to_owned(), false));
        // Exactly at the limit is kept whole: the comparison is <=, not <.
        assert_eq!(truncate("hello", 5), ("hello".to_owned(), false));
        // One byte under, and it is cut: the comparison is not >=.
        assert_eq!(truncate("hello", 4), ("hell".to_owned(), true));
        assert_eq!(truncate("hello", 3), ("hel".to_owned(), true));
        // "é" is two bytes, so a limit of three bytes keeps only the first character.
        assert_eq!(truncate("éé", 3), ("é".to_owned(), true));
        assert_eq!(truncate("éé", 0), (String::new(), true));
    }
}
