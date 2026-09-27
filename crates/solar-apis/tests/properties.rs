//! The first principle, proved against inputs nobody thought of.
//!
//! The example tests check the cases somebody imagined. These check the promise itself:
//! *whatever* arrives on the wire, one well formed response comes back, and nothing
//! panics and nothing goes silent. proptest generates the inputs and, when one fails,
//! shrinks it to the smallest line that still fails, which is the line worth reading.
//!
//! The failing cases proptest finds are written to `proptest-regressions/` beside this
//! file, versioned, and replayed on every later run: a bug found once is never found
//! twice.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test reports failure by panicking; integration tests are their own crate"
)]

use proptest::prelude::*;
use serde_json::{Value, json};
use solar_core::dispatch::Dispatcher;
use solar_core::status::Status;

/// One dispatcher for the whole file: building it per case would measure the registry
/// rather than the protocol.
fn dispatcher() -> &'static Dispatcher {
    static DISPATCHER: std::sync::OnceLock<Dispatcher> = std::sync::OnceLock::new();
    DISPATCHER.get_or_init(solar_apis::dispatcher)
}

/// Every registered method name, for generating calls that exist.
fn method_names() -> Vec<&'static str> {
    solar_apis::registry()
        .expect("a valid registry")
        .names()
        .collect()
}

/// Checks the envelope rules that hold for every response there can ever be.
fn is_a_well_formed_response(line: &str) -> Result<Value, String> {
    if line.contains('\n') {
        return Err("the response contains a newline, which would break the framing".to_owned());
    }
    let value: Value =
        serde_json::from_str(line).map_err(|failure| format!("not JSON: {failure}"))?;
    let object = value.as_object().ok_or("the response is not an object")?;

    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err("the response does not declare JSON-RPC 2.0".to_owned());
    }
    if !object.contains_key("id") {
        return Err("the response has no id member".to_owned());
    }
    match (object.get("result"), object.get("error")) {
        (Some(_), Some(_)) => {
            return Err("the response carries both a result and an error".to_owned());
        }
        (None, None) => return Err("the response carries neither a result nor an error".to_owned()),
        (Some(result), None) => {
            for member in ["data", "meta", "warnings"] {
                if result.get(member).is_none() {
                    return Err(format!("the result has no {member}"));
                }
            }
        }
        (None, Some(error)) => {
            let data = error.get("data").ok_or("the error has no data")?;
            let status = data
                .get("status")
                .and_then(Value::as_str)
                .ok_or("no status")?;
            Status::from_str_canonical(status).ok_or(format!("{status} is not canonical"))?;
            data.get("reason")
                .and_then(Value::as_str)
                .ok_or("no reason")?;
            let details = data
                .get("details")
                .and_then(Value::as_array)
                .ok_or("no details")?;
            if details.is_empty() {
                return Err("details is empty, which the contract forbids".to_owned());
            }
            error.get("code").and_then(Value::as_i64).ok_or("no code")?;
        }
    }
    Ok(value)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    /// The first principle: any line at all is answered, exactly once, in one line.
    #[test]
    fn any_line_gets_exactly_one_well_formed_response(line in ".*") {
        let response = dispatcher().handle_line(&line);
        let text = response.to_line();
        prop_assert!(
            is_a_well_formed_response(&text).is_ok(),
            "{:?} for input {:?}",
            is_a_well_formed_response(&text),
            line
        );
    }

    /// The same, for input that at least looks like JSON, which reaches deeper code.
    #[test]
    fn any_json_value_gets_a_well_formed_response(value in any_json()) {
        let response = dispatcher().handle_line(&value.to_string());
        let text = response.to_line();
        prop_assert!(
            is_a_well_formed_response(&text).is_ok(),
            "{:?} for input {}",
            is_a_well_formed_response(&text),
            value
        );
    }

    /// A well formed call always comes back with the id it was given.
    #[test]
    fn the_id_always_comes_back(id in 0i64..1_000_000, index in 0usize..16) {
        let names = method_names();
        let method = names[index % names.len()];
        let line = json!({"jsonrpc": "2.0", "id": id, "method": method}).to_string();
        let answered = is_a_well_formed_response(&dispatcher().handle_line(&line).to_line())
            .expect("a well formed response");
        prop_assert_eq!(answered["id"].as_i64(), Some(id));
        prop_assert_eq!(answered["error"]["data"]["meta"]["request_id"].as_i64()
            .or_else(|| answered["result"]["meta"]["request_id"].as_i64()), Some(id));
    }

    /// A string id comes back as the same string, however odd it is.
    #[test]
    fn a_string_id_comes_back_unchanged(id in ".{0,40}") {
        let line = json!({"jsonrpc": "2.0", "id": id, "method": "solar.ping"}).to_string();
        let answered = is_a_well_formed_response(&dispatcher().handle_line(&line).to_line())
            .expect("a well formed response");
        prop_assert_eq!(answered["id"].as_str(), Some(id.as_str()));
    }

    /// Parameters the schema refuses are always INVALID_ARGUMENT, and always say where.
    #[test]
    fn bad_parameters_are_invalid_argument_and_name_the_field(
        name in "[a-z]{1,10}",
        value in any_json(),
    ) {
        // `solar.describe` takes exactly one member, `api`, and it must be a string.
        prop_assume!(name != "api" || !value.is_string());
        let line = json!({
            "jsonrpc": "2.0", "id": 1, "method": "solar.describe",
            "params": {name.clone(): value.clone()},
        }).to_string();

        let answered = is_a_well_formed_response(&dispatcher().handle_line(&line).to_line())
            .expect("a well formed response");
        let data = &answered["error"]["data"];
        prop_assert_eq!(
            data["status"].as_str(),
            Some("INVALID_ARGUMENT"),
            "{} = {} should be refused: {}",
            name,
            value,
            answered
        );
        let field = data["details"][0]["field"].as_str().unwrap_or_default();
        prop_assert!(
            field.contains(&name) || field == "/api" || field == "params",
            "the error does not point at {}: {}",
            name,
            answered
        );
    }
}

/// Arbitrary JSON, a few levels deep, of every kind a caller can send.
fn any_json() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::from),
        any::<i64>().prop_map(Value::from),
        ".{0,20}".prop_map(Value::from),
    ];
    leaf.prop_recursive(3, 16, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(Value::from),
            prop::collection::hash_map("[a-z]{1,6}", inner, 0..4)
                .prop_map(|members| Value::Object(members.into_iter().collect())),
        ]
    })
}
