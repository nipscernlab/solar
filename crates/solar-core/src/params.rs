//! Turning a parameter object into a typed value, and a failure into a usable error.
//!
//! Serde already knows what went wrong. What it does not know is the JSON pointer of the
//! offending member, the type the schema wanted there, the members that would have been
//! accepted, or what a working call looks like. This module adds all four.

use serde::de::DeserializeOwned;
use serde_json::Value;
use serde_path_to_error::Segment;

use crate::error::{ErrorDetail, SolarError};
use crate::protocol::brief;
use crate::reason::Reason;
use crate::status::Status;
use crate::text::suggestions;

/// Reads `params` into the type an API declared.
///
/// `schema` is the parameter schema of that API, used to say what was expected where, and
/// `example` is the `params` of its first example, used to show a call that works.
///
/// # Errors
///
/// Returns `INVALID_ARGUMENT` with the reason that fits: `MISSING_FIELD`, `UNKNOWN_FIELD`,
/// `TYPE_MISMATCH` or `INVALID_VALUE`.
pub fn deserialize<T: DeserializeOwned>(
    method: &str,
    params: &Value,
    schema: &Value,
    example: Option<&Value>,
) -> Result<T, SolarError> {
    let failure = match serde_path_to_error::deserialize::<_, T>(params) {
        Ok(value) => return Ok(value),
        Err(failure) => failure,
    };

    let raw = failure.inner().to_string();
    let message = strip_position(&raw);
    let mut pointer = pointer_of(failure.path());
    let (reason, expected_from_message) = classify(message);

    if reason == Reason::MissingField
        && let Some(name) = quoted(message)
    {
        pointer = format!("{pointer}/{name}");
    }

    let received = params.pointer(&pointer).map_or(Value::Null, brief);
    let expected = match reason {
        // The member is not in the schema, so the schema cannot say what belongs there.
        // What helps instead is the list of members that would have been accepted.
        Reason::UnknownField => {
            let accepted = accepted_members(schema);
            if accepted.is_empty() {
                "no parameters at all".to_owned()
            } else {
                format!("one of: {}", accepted.join(", "))
            }
        }
        _ => expected_from_message
            .map(str::to_owned)
            .or_else(|| expected_from_schema(schema, &pointer))
            .unwrap_or_else(|| "a value the schema accepts".to_owned()),
    };

    let mut detail = ErrorDetail::new(Status::InvalidArgument)
        .field(if pointer.is_empty() {
            "params".to_owned()
        } else {
            pointer.clone()
        })
        .expected(expected)
        .received(received)
        .hint(hint_for(reason, message, schema, &pointer, example));

    if reason == Reason::MissingField {
        detail.received = Value::Null;
    }

    Err(SolarError::new(
        reason,
        format!(
            "Invalid params for {method}: {}.",
            message.trim_end_matches('.')
        ),
    )
    .with_detail(detail))
}

/// Builds a JSON pointer from the path serde walked before it gave up.
fn pointer_of(path: &serde_path_to_error::Path) -> String {
    let mut pointer = String::new();
    for segment in path {
        match segment {
            Segment::Seq { index } => {
                pointer.push('/');
                pointer.push_str(&index.to_string());
            }
            Segment::Map { key } => {
                pointer.push('/');
                pointer.push_str(&key.replace('~', "~0").replace('/', "~1"));
            }
            Segment::Enum { variant } => {
                pointer.push('/');
                pointer.push_str(variant);
            }
            Segment::Unknown => {}
        }
    }
    pointer
}

/// Drops the `at line 1 column 7` tail that serde appends when it read from text.
fn strip_position(message: &str) -> &str {
    match message.find(" at line ") {
        Some(cut) => message.get(..cut).unwrap_or(message),
        None => message,
    }
}

/// The first `` `quoted` `` word of a serde message, which is the member it is about.
fn quoted(message: &str) -> Option<&str> {
    let start = message.find('`')? + 1;
    let rest = message.get(start..)?;
    let end = rest.find('`')?;
    rest.get(..end)
}

/// What serde said, expressed as a reason and, when serde knew it, the expected type.
fn classify(message: &str) -> (Reason, Option<&str>) {
    let expected = message.split(", expected ").nth(1);
    if message.starts_with("missing field") {
        (Reason::MissingField, None)
    } else if message.starts_with("unknown field") {
        (Reason::UnknownField, None)
    } else if message.starts_with("invalid type") {
        (Reason::TypeMismatch, expected)
    } else if message.starts_with("invalid value") || message.starts_with("invalid length") {
        (Reason::InvalidValue, expected)
    } else if message.starts_with("duplicate field") {
        (Reason::InvalidValue, None)
    } else {
        (Reason::InvalidValue, expected)
    }
}

/// Follows a JSON pointer through a JSON Schema and reports the type declared there.
///
/// Best effort by design: an exotic schema simply yields `None`, and the caller falls back
/// to what serde said. Only the shapes `schemars` actually produces are walked.
fn expected_from_schema(schema: &Value, pointer: &str) -> Option<String> {
    let mut current = resolve(schema, schema)?;
    for raw in pointer.split('/').skip(1) {
        let key = raw.replace("~1", "/").replace("~0", "~");
        let next = if key.chars().all(|c| c.is_ascii_digit()) && current.get("items").is_some() {
            current.get("items")?
        } else {
            current.get("properties")?.get(&key)?
        };
        current = resolve(next, schema)?;
    }
    describe_type(current)
}

/// Follows a single `$ref` into the `$defs` of the root schema.
fn resolve<'a>(node: &'a Value, root: &'a Value) -> Option<&'a Value> {
    let Some(reference) = node.get("$ref").and_then(Value::as_str) else {
        return Some(node);
    };
    let name = reference.strip_prefix("#/$defs/")?;
    root.get("$defs")?.get(name)
}

/// Turns the `type` of a schema node into the words a caller reads in `expected`.
fn describe_type(node: &Value) -> Option<String> {
    match node.get("type") {
        Some(Value::String(name)) => Some(name.clone()),
        Some(Value::Array(names)) => {
            let listed: Vec<&str> = names.iter().filter_map(Value::as_str).collect();
            if listed.is_empty() {
                None
            } else {
                Some(listed.join(" or "))
            }
        }
        _ => {
            if node.get("enum").is_some() {
                Some(format!("one of {}", node.get("enum")?))
            } else {
                None
            }
        }
    }
}

/// The sentence that tells the caller what to do about this particular failure.
fn hint_for(
    reason: Reason,
    message: &str,
    schema: &Value,
    pointer: &str,
    example: Option<&Value>,
) -> String {
    let mut hint = match reason {
        Reason::MissingField => {
            let name = quoted(message).unwrap_or("the member");
            format!("Add {name} to params.")
        }
        Reason::UnknownField => {
            let name = quoted(message).unwrap_or("that member");
            let accepted = accepted_members(schema);
            let closest = suggestions(name, accepted.iter().map(String::as_str));
            match closest.first() {
                Some((suggestion, _)) => {
                    format!("There is no {name} parameter. Did you mean {suggestion}?")
                }
                None if accepted.is_empty() => {
                    format!("There is no {name} parameter, and this API takes none.")
                }
                None => {
                    format!(
                        "There is no {name} parameter. This API accepts {}.",
                        accepted.join(", ")
                    )
                }
            }
        }
        Reason::TypeMismatch => {
            let position = if pointer.is_empty() {
                "params"
            } else {
                pointer
            };
            format!("Send the value at {position} with the type the schema declares.")
        }
        _ => "Check the value against the parameter schema in the manifest.".to_owned(),
    };

    if let Some(example) = example {
        use std::fmt::Write as _;
        let _ = write!(hint, " A call that works: {example}.");
    }
    hint
}

/// The member names a parameter schema accepts, in the order the schema lists them.
fn accepted_members(schema: &Value) -> Vec<String> {
    schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|members| members.keys().cloned().collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::{JsonSchema, schema_for};
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Debug, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct Params {
        /// The method name.
        api: String,
        /// How deep to go.
        #[serde(default)]
        depth: Option<u8>,
        /// The tools to look at.
        #[serde(default)]
        tools: Vec<String>,
    }

    fn schema() -> Value {
        serde_json::to_value(schema_for!(Params)).unwrap()
    }

    fn read(params: &Value) -> SolarError {
        let example = json!({"api": "solar.ping"});
        deserialize::<Params>("test.api", params, &schema(), Some(&example))
            .expect_err("these params should be refused")
    }

    #[test]
    fn every_thing_serde_says_is_classified_as_its_own_reason() {
        // The words are serde's, and each arm earns its place: a caller told
        // INVALID_VALUE for a missing field would look in the wrong place.
        let cases = [
            ("missing field `api`", Reason::MissingField, None),
            ("unknown field `apy`", Reason::UnknownField, None),
            (
                "invalid type: string \"x\", expected u32",
                Reason::TypeMismatch,
                Some("u32"),
            ),
            (
                "invalid value: integer `0`, expected a positive number",
                Reason::InvalidValue,
                Some("a positive number"),
            ),
            (
                "invalid length 0, expected at least one",
                Reason::InvalidValue,
                Some("at least one"),
            ),
            ("duplicate field `api`", Reason::InvalidValue, None),
            ("something nobody has seen", Reason::InvalidValue, None),
        ];

        for (message, reason, expected) in cases {
            let (read, said) = classify(message);
            assert_eq!(read, reason, "{message}");
            assert_eq!(said, expected, "{message}");
        }
    }

    #[test]
    fn a_pointer_into_an_array_is_followed_only_when_the_schema_has_items() {
        // Both halves of the condition matter. A digit is an index only where the schema
        // says there is a list; a member whose name is a number is still a member.
        let list = json!({
            "type": "object",
            "properties": {
                "names": {"type": "array", "items": {"type": "string"}},
                "7": {"type": "boolean"}
            }
        });

        assert_eq!(
            expected_from_schema(&list, "/names/0").as_deref(),
            Some("string"),
            "a digit under something with items is an index"
        );
        assert_eq!(
            expected_from_schema(&list, "/7").as_deref(),
            Some("boolean"),
            "a digit that names a member is a member"
        );
    }

    #[test]
    fn a_type_written_as_a_list_is_read_as_one_of_them() {
        assert_eq!(
            describe_type(&json!({"type": "string"})).as_deref(),
            Some("string")
        );
        assert_eq!(
            describe_type(&json!({"type": ["string", "null"]})).as_deref(),
            Some("string or null"),
            "schemars writes an optional member this way"
        );
        assert_eq!(
            describe_type(&json!({"type": []})),
            None,
            "a list of nothing describes nothing"
        );
        assert_eq!(describe_type(&json!({})), None);
    }

    /// An API that takes no parameters at all, for the hint that says so.
    #[derive(Debug, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct Nothing {}

    #[test]
    fn the_hint_for_an_unknown_member_says_what_the_api_does_take() {
        let schema = serde_json::to_value(schema_for!(Params)).expect("a schema");

        // Nothing close: the hint lists what there is.
        let hint = hint_for(
            Reason::UnknownField,
            "unknown field `wildly_different`",
            &schema,
            "",
            None,
        );
        assert!(hint.contains("api"), "it lists the members: {hint}");

        // An API that takes nothing at all says so, rather than listing an empty list.
        let empty = serde_json::to_value(schema_for!(Nothing)).expect("a schema");
        let hint = hint_for(
            Reason::UnknownField,
            "unknown field `anything`",
            &empty,
            "",
            None,
        );
        assert!(
            hint.contains("takes none"),
            "an API with no parameters says so: {hint}"
        );
    }

    #[test]
    fn valid_params_come_back_typed() {
        let parsed: Params =
            deserialize("test.api", &json!({"api": "solar.ping"}), &schema(), None).unwrap();
        assert_eq!(parsed.api, "solar.ping");
        assert_eq!(parsed.depth, None);
        assert!(parsed.tools.is_empty());
    }

    #[test]
    fn a_missing_member_points_at_it_and_says_what_it_should_hold() {
        let error = read(&json!({}));
        assert_eq!(error.reason(), Reason::MissingField);
        let detail = &error.details()[0];
        assert_eq!(detail.field.as_deref(), Some("/api"));
        assert_eq!(detail.expected.as_deref(), Some("string"));
        assert_eq!(detail.received, Value::Null);
        assert!(detail.hint.as_deref().unwrap().contains("Add api"));
        assert!(
            detail
                .hint
                .as_deref()
                .unwrap()
                .contains("A call that works")
        );
        assert!(error.message().starts_with("Invalid params for test.api:"));
    }

    #[test]
    fn a_misspelled_member_is_refused_and_the_right_name_is_suggested() {
        let error = read(&json!({"api": "solar.ping", "dept": 1}));
        assert_eq!(error.reason(), Reason::UnknownField);
        let detail = &error.details()[0];
        assert_eq!(detail.field.as_deref(), Some("/dept"));
        assert_eq!(
            detail.expected.as_deref(),
            Some("one of: api, depth, tools")
        );
        let hint = detail.hint.as_deref().unwrap();
        assert!(hint.contains("Did you mean depth?"), "{hint}");
    }

    #[test]
    fn the_wrong_type_names_the_type_the_schema_wanted() {
        let error = read(&json!({"api": 3}));
        assert_eq!(error.reason(), Reason::TypeMismatch);
        let detail = &error.details()[0];
        assert_eq!(detail.field.as_deref(), Some("/api"));
        assert_eq!(detail.expected.as_deref(), Some("a string"));
        assert_eq!(detail.received, json!(3));
    }

    #[test]
    fn a_value_out_of_range_is_an_invalid_value_and_echoes_what_arrived() {
        let error = read(&json!({"api": "x", "depth": 900}));
        assert_eq!(error.reason(), Reason::InvalidValue);
        assert_eq!(error.details()[0].field.as_deref(), Some("/depth"));
        assert_eq!(error.details()[0].received, json!(900));
    }

    #[test]
    fn a_failure_inside_an_array_points_at_the_element() {
        let error = read(&json!({"api": "x", "tools": ["ok", 7]}));
        assert_eq!(error.reason(), Reason::TypeMismatch);
        assert_eq!(error.details()[0].field.as_deref(), Some("/tools/1"));
        assert_eq!(error.details()[0].received, json!(7));
    }

    #[test]
    fn params_that_are_not_an_object_are_still_refused_with_a_complete_error() {
        // The protocol layer already refuses params that are not an object, so this only
        // guards the case where a caller of this module skips that check.
        let error = read(&json!([1, 2]));
        assert_eq!(error.reason(), Reason::TypeMismatch);
        assert!(!error.details().is_empty());
        assert!(error.details()[0].hint.is_some());
    }

    #[test]
    fn the_schema_walker_reads_types_through_a_reference() {
        let schema = json!({
            "type": "object",
            "properties": {"inner": {"$ref": "#/$defs/Inner"}},
            "$defs": {"Inner": {"type": "object", "properties": {"n": {"type": "integer"}}}}
        });
        assert_eq!(
            expected_from_schema(&schema, "/inner/n").as_deref(),
            Some("integer")
        );
        assert_eq!(expected_from_schema(&schema, "/nope").as_deref(), None);
    }
}
