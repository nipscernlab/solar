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
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
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
    fn truncation_keeps_whole_characters() {
        assert_eq!(truncate("hello", 10), ("hello".to_owned(), false));
        assert_eq!(truncate("hello", 5), ("hello".to_owned(), false));
        assert_eq!(truncate("hello", 3), ("hel".to_owned(), true));
        // "é" is two bytes, so a limit of three bytes keeps only the first character.
        assert_eq!(truncate("éé", 3), ("é".to_owned(), true));
        assert_eq!(truncate("éé", 0), (String::new(), true));
    }
}
