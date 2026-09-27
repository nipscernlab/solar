//! Comparing the response an example promises with the one that really came back.
//!
//! The two modes and the `$any` token are defined in section 8.2 of the contract. This is
//! the only place that knows how to read them, and the contract tests are its only caller.

use serde_json::Value;

use crate::api::{ANY, MatchMode};

/// Where and how a real response differed from the one an example promised.
#[derive(Debug, Clone, PartialEq)]
pub struct Mismatch {
    /// A JSON pointer to the position that differs, `""` for the whole value.
    pub path: String,
    /// What the example promised there.
    pub expected: Value,
    /// What arrived there, `None` when nothing did.
    pub actual: Option<Value>,
    /// One sentence naming the difference.
    pub note: String,
}

impl std::fmt::Display for Mismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let position = if self.path.is_empty() {
            "the response"
        } else {
            &self.path
        };
        match &self.actual {
            Some(actual) => write!(
                f,
                "at {position}: {}. Expected {}, got {}.",
                self.note, self.expected, actual
            ),
            None => write!(
                f,
                "at {position}: {}. Expected {}, got nothing.",
                self.note, self.expected
            ),
        }
    }
}

/// Compares an expected value with a real one under the given mode.
///
/// # Errors
///
/// Returns the first [`Mismatch`], in document order, so that a failing test names one
/// precise position instead of dumping two documents side by side.
pub fn matches(expected: &Value, actual: &Value, mode: MatchMode) -> Result<(), Mismatch> {
    compare(expected, actual, mode, &mut String::new())
}

fn compare(
    expected: &Value,
    actual: &Value,
    mode: MatchMode,
    path: &mut String,
) -> Result<(), Mismatch> {
    if expected.as_str() == Some(ANY) {
        return Ok(());
    }

    match (expected, actual) {
        (Value::Object(want), Value::Object(got)) => {
            if mode == MatchMode::Exact {
                for key in got.keys() {
                    if !want.contains_key(key) {
                        return Err(Mismatch {
                            path: format!("{path}/{key}"),
                            expected: Value::Null,
                            actual: got.get(key).cloned(),
                            note: "an exact example must name every member of the response"
                                .to_owned(),
                        });
                    }
                }
            }
            for (key, want_value) in want {
                let Some(got_value) = got.get(key) else {
                    return Err(Mismatch {
                        path: format!("{path}/{key}"),
                        expected: want_value.clone(),
                        actual: None,
                        note: "the response is missing this member".to_owned(),
                    });
                };
                let restore = path.len();
                path.push('/');
                path.push_str(key);
                compare(want_value, got_value, mode, path)?;
                path.truncate(restore);
            }
            Ok(())
        }
        (Value::Array(want), Value::Array(got)) => {
            if want.len() != got.len() {
                return Err(Mismatch {
                    path: path.clone(),
                    expected: Value::from(want.len()),
                    actual: Some(Value::from(got.len())),
                    note: "the arrays have different lengths".to_owned(),
                });
            }
            for (index, (want_item, got_item)) in want.iter().zip(got).enumerate() {
                let restore = path.len();
                path.push('/');
                path.push_str(&index.to_string());
                compare(want_item, got_item, mode, path)?;
                path.truncate(restore);
            }
            Ok(())
        }
        (want, got) if want == got => Ok(()),
        (want, got) => Err(Mismatch {
            path: path.clone(),
            expected: want.clone(),
            actual: Some(got.clone()),
            note: "the values differ".to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn identical_values_match_in_both_modes() {
        let value = json!({"a": 1, "b": [true, "x"]});
        assert!(matches(&value, &value, MatchMode::Exact).is_ok());
        assert!(matches(&value, &value, MatchMode::Subset).is_ok());
    }

    #[test]
    fn subset_ignores_members_the_example_does_not_name() {
        let want = json!({"a": 1});
        let got = json!({"a": 1, "b": 2});
        assert!(matches(&want, &got, MatchMode::Subset).is_ok());
        let failure = matches(&want, &got, MatchMode::Exact).unwrap_err();
        assert_eq!(failure.path, "/b");
    }

    #[test]
    fn a_missing_member_fails_in_both_modes() {
        let want = json!({"a": 1, "nested": {"deep": true}});
        let got = json!({"a": 1, "nested": {}});
        for mode in [MatchMode::Exact, MatchMode::Subset] {
            let failure = matches(&want, &got, mode).unwrap_err();
            assert_eq!(failure.path, "/nested/deep");
            assert_eq!(failure.actual, None);
        }
    }

    #[test]
    fn any_matches_whatever_is_there() {
        let want = json!({"when": ANY, "nested": {"path": ANY}});
        let got = json!({"when": "2026-09-26T21:41:03.123456Z", "nested": {"path": null}});
        assert!(matches(&want, &got, MatchMode::Exact).is_ok());

        let got = json!({"when": [1, 2, 3], "nested": {"path": {"deep": true}}});
        assert!(matches(&want, &got, MatchMode::Exact).is_ok());
    }

    #[test]
    fn arrays_must_have_the_same_length() {
        let want = json!({"items": [1, 2]});
        let got = json!({"items": [1, 2, 3]});
        let failure = matches(&want, &got, MatchMode::Subset).unwrap_err();
        assert_eq!(failure.path, "/items");
        assert!(failure.note.contains("lengths"));
    }

    #[test]
    fn a_differing_scalar_names_its_position() {
        let want = json!({"items": [{"name": "iverilog"}]});
        let got = json!({"items": [{"name": "vvp"}]});
        let failure = matches(&want, &got, MatchMode::Subset).unwrap_err();
        assert_eq!(failure.path, "/items/0/name");
        assert_eq!(failure.expected, json!("iverilog"));
        assert_eq!(failure.actual, Some(json!("vvp")));
        assert!(failure.to_string().contains("/items/0/name"));
    }

    #[test]
    fn a_type_change_is_a_mismatch_not_a_coincidence() {
        assert!(matches(&json!(1), &json!("1"), MatchMode::Exact).is_err());
        assert!(matches(&json!(true), &json!(1), MatchMode::Exact).is_err());
        assert!(matches(&json!(null), &json!(0), MatchMode::Exact).is_err());
    }

    #[test]
    fn the_whole_response_can_be_any() {
        assert!(matches(&json!(ANY), &json!({"anything": [1]}), MatchMode::Exact).is_ok());
    }
}
