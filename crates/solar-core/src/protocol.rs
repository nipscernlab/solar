//! The JSON-RPC 2.0 envelope, with the two deviations SOLAR makes on purpose.
//!
//! Parsing is written by hand against [`serde_json::Value`] rather than derived, because
//! the point of this layer is the quality of the error it produces when the envelope is
//! wrong, and a derived `Deserialize` cannot say what a caller needs to hear.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::{ErrorDetail, SolarError};
use crate::meta::{Meta, PROTOCOL};
use crate::reason::Reason;
use crate::status::Status;
use crate::text::{is_valid_method_name, truncate};
use crate::warning::Warning;

/// The largest request line SOLAR reads, in bytes, not counting the newline.
pub const MAX_REQUEST_BYTES: usize = 16 * 1024 * 1024;

/// How much of a value is echoed back in `received` before it is cut.
pub const MAX_RECEIVED_BYTES: usize = 200;

/// The identifier of a request: a number or a string, never null.
///
/// JSON-RPC 2.0 also allows `null`, which it treats as a notification. SOLAR does not
/// accept notifications, so `null` is refused at the door and this type cannot hold it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    /// A numeric identifier, the usual choice of a programmatic caller.
    Number(serde_json::Number),
    /// A string identifier.
    Text(String),
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestId::Number(number) => write!(f, "{number}"),
            RequestId::Text(text) => write!(f, "{text}"),
        }
    }
}

/// A request that passed every envelope check.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// The identifier, echoed into the response and into `meta.request_id`.
    pub id: RequestId,
    /// The method name, already checked against the naming rule.
    pub method: String,
    /// The parameters, always an object, `{}` when the member was absent.
    pub params: Map<String, Value>,
}

/// An envelope that did not pass, together with whatever could still be recovered from it.
///
/// The `id` and the `method` are kept when they were readable, so that the response can
/// still echo them and `meta` can still name the method that was attempted.
#[derive(Debug, Clone, PartialEq)]
pub struct RequestError {
    /// The identifier, when the message carried a usable one.
    pub id: Option<RequestId>,
    /// The method, when the message carried a usable one.
    pub method: Option<String>,
    /// What went wrong.
    pub error: SolarError,
}

impl RequestError {
    /// An envelope failure with nothing recoverable.
    fn bare(error: SolarError) -> Self {
        Self {
            id: None,
            method: None,
            error,
        }
    }
}

/// Shortens a value so that it can be echoed inside an error without carrying megabytes.
///
/// Scalars pass through untouched unless they are long strings. A composite value larger
/// than [`MAX_RECEIVED_BYTES`] is replaced by a one line description of itself, because a
/// caller reading an error wants to know what kind of thing arrived, not to receive it back.
#[must_use]
pub fn brief(value: &Value) -> Value {
    match value {
        Value::String(text) => {
            let (kept, cut) = truncate(text, MAX_RECEIVED_BYTES);
            if cut {
                Value::String(format!("{kept} [...]"))
            } else {
                value.clone()
            }
        }
        Value::Array(items) => {
            let rendered = value.to_string();
            if rendered.len() <= MAX_RECEIVED_BYTES {
                value.clone()
            } else {
                Value::String(format!(
                    "[an array of {} items, {} bytes]",
                    items.len(),
                    rendered.len()
                ))
            }
        }
        Value::Object(members) => {
            let rendered = value.to_string();
            if rendered.len() <= MAX_RECEIVED_BYTES {
                value.clone()
            } else {
                Value::String(format!(
                    "[an object with {} members, {} bytes]",
                    members.len(),
                    rendered.len()
                ))
            }
        }
        other => other.clone(),
    }
}

/// The name of a JSON value as a caller would say it.
#[must_use]
pub fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Reads one line into a [`Request`], or into the error that line deserves.
///
/// The members are checked in the order `id`, `jsonrpc`, `method`, `params`. The identifier
/// comes first so that every later error can still be addressed to the right call.
///
/// # Errors
///
/// Returns a [`RequestError`] when the line is not valid JSON, is a batch, is not an
/// object, has no usable `id`, does not declare `"jsonrpc": "2.0"`, has no valid `method`,
/// or carries a `params` that is not an object.
pub fn parse_request(line: &str) -> Result<Request, RequestError> {
    let value: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(parse) => {
            let (excerpt, _) = truncate(line, MAX_RECEIVED_BYTES);
            return Err(RequestError::bare(
                SolarError::new(
                    Reason::ParseError,
                    format!("The request is not valid JSON: {parse}."),
                )
                .with_detail(
                    ErrorDetail::new(Status::InvalidArgument)
                        .expected("a single JSON object on one line")
                        .received(excerpt)
                        .hint(
                            "Send exactly one JSON object per line. A newline inside a \
                             message ends it, so escape newlines inside strings as \\n.",
                        ),
                ),
            ));
        }
    };

    if value.is_array() {
        return Err(RequestError::bare(
            SolarError::new(
                Reason::BatchNotSupported,
                "Batch requests are not implemented in solar/1.",
            )
            .with_detail(
                ErrorDetail::new(Status::Unimplemented)
                    .expected("a single JSON object")
                    .received(Value::String("array".to_owned()))
                    .hint(
                        "Send one request per line. `solar serve --stdio` keeps the session \
                         open and answers each line in order.",
                    ),
            ),
        ));
    }

    let Some(object) = value.as_object() else {
        return Err(RequestError::bare(
            SolarError::new(
                Reason::TypeMismatch,
                format!("A request must be a JSON object, and a {} arrived.", type_name(&value)),
            )
            .with_detail(
                ErrorDetail::new(Status::InvalidArgument)
                    .expected("object")
                    .received(brief(&value))
                    .hint("Wrap the call in an object, e.g. {\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"solar.ping\"}."),
            )
            .as_envelope_failure(),
        ));
    };

    let id = read_id(object)?;
    check_jsonrpc(object, &id)?;
    let method = read_method(object, &id)?;
    let params = read_params(object, &id, &method)?;

    Ok(Request { id, method, params })
}

/// Reads `id`, refusing both a missing identifier and one of the wrong type.
fn read_id(object: &Map<String, Value>) -> Result<RequestId, RequestError> {
    match object.get("id") {
        None | Some(Value::Null) => Err(RequestError::bare(
            SolarError::new(
                Reason::NotificationNotSupported,
                "Every SOLAR call needs an id, because every call gets a response.",
            )
            .with_detail(
                ErrorDetail::new(Status::InvalidArgument)
                    .field("id")
                    .expected("a number or a string")
                    .received(object.get("id").cloned().unwrap_or(Value::Null))
                    .hint(
                        "Add an id. JSON-RPC 2.0 calls a message without one a \
                         notification and forbids a reply; SOLAR answers every call, so it \
                         refuses notifications instead.",
                    ),
            )
            .as_envelope_failure(),
        )),
        Some(Value::Number(number)) => Ok(RequestId::Number(number.clone())),
        Some(Value::String(text)) => Ok(RequestId::Text(text.clone())),
        Some(other) => Err(RequestError::bare(
            SolarError::new(
                Reason::TypeMismatch,
                format!(
                    "The id must be a number or a string, and a {} arrived.",
                    type_name(other)
                ),
            )
            .with_detail(
                ErrorDetail::new(Status::InvalidArgument)
                    .field("id")
                    .expected("a number or a string")
                    .received(brief(other))
                    .hint("Use a number, e.g. \"id\": 1, or a string, e.g. \"id\": \"a3f\"."),
            )
            .as_envelope_failure(),
        )),
    }
}

/// Checks that the message declares JSON-RPC 2.0.
fn check_jsonrpc(object: &Map<String, Value>, id: &RequestId) -> Result<(), RequestError> {
    let fail = |error: SolarError| RequestError {
        id: Some(id.clone()),
        method: None,
        error: error.as_envelope_failure(),
    };
    match object.get("jsonrpc") {
        None => Err(fail(
            SolarError::new(
                Reason::MissingField,
                "The request is missing the jsonrpc member.",
            )
            .with_detail(
                ErrorDetail::new(Status::InvalidArgument)
                    .field("jsonrpc")
                    .expected("\"2.0\"")
                    .hint("Add \"jsonrpc\": \"2.0\" to the request."),
            ),
        )),
        Some(Value::String(version)) if version == "2.0" => Ok(()),
        Some(Value::String(version)) => Err(fail(
            SolarError::new(
                Reason::InvalidValue,
                format!("The only JSON-RPC version SOLAR speaks is 2.0, and {version} arrived."),
            )
            .with_detail(
                ErrorDetail::new(Status::InvalidArgument)
                    .field("jsonrpc")
                    .expected("\"2.0\"")
                    .received(Value::String(version.clone()))
                    .hint("Set \"jsonrpc\": \"2.0\"."),
            ),
        )),
        Some(other) => Err(fail(
            SolarError::new(
                Reason::TypeMismatch,
                format!(
                    "The jsonrpc member must be a string, and a {} arrived.",
                    type_name(other)
                ),
            )
            .with_detail(
                ErrorDetail::new(Status::InvalidArgument)
                    .field("jsonrpc")
                    .expected("\"2.0\"")
                    .received(brief(other))
                    .hint("Set \"jsonrpc\": \"2.0\", with the quotes."),
            ),
        )),
    }
}

/// Reads `method`, checking it against the naming rule of section 4 of the contract.
fn read_method(object: &Map<String, Value>, id: &RequestId) -> Result<String, RequestError> {
    let fail = |error: SolarError, method: Option<String>| RequestError {
        id: Some(id.clone()),
        method,
        error: error.as_envelope_failure(),
    };
    match object.get("method") {
        None => Err(fail(
            SolarError::new(
                Reason::MissingField,
                "The request is missing the method member.",
            )
            .with_detail(
                ErrorDetail::new(Status::InvalidArgument)
                    .field("method")
                    .expected("a registered method name, e.g. \"solar.ping\"")
                    .hint(
                        "Add \"method\": \"solar.ping\". Call solar.manifest for the whole list.",
                    ),
            ),
            None,
        )),
        Some(Value::String(name)) if is_valid_method_name(name) => Ok(name.clone()),
        Some(Value::String(name)) => {
            let lowered = name.to_lowercase();
            let hint = if lowered != *name && is_valid_method_name(&lowered) {
                format!("Method names are lower case: write \"{lowered}\".")
            } else {
                "A method name is namespace.verb_noun, lower case, e.g. \"tools.detect\"."
                    .to_owned()
            };
            Err(fail(
                SolarError::new(
                    Reason::InvalidValue,
                    format!("The method name {name:?} does not match the naming rule."),
                )
                .with_detail(
                    ErrorDetail::new(Status::InvalidArgument)
                        .field("method")
                        .expected("^[a-z]+(\\.[a-z]+(_[a-z]+)*)+$")
                        .received(brief(&Value::String(name.clone())))
                        .hint(hint),
                ),
                Some(name.clone()),
            ))
        }
        Some(other) => Err(fail(
            SolarError::new(
                Reason::TypeMismatch,
                format!(
                    "The method must be a string, and a {} arrived.",
                    type_name(other)
                ),
            )
            .with_detail(
                ErrorDetail::new(Status::InvalidArgument)
                    .field("method")
                    .expected("string")
                    .received(brief(other))
                    .hint("Write the method as a string, e.g. \"method\": \"solar.ping\"."),
            ),
            None,
        )),
    }
}

/// Reads `params`, which is optional and, when present, is an object.
fn read_params(
    object: &Map<String, Value>,
    id: &RequestId,
    method: &str,
) -> Result<Map<String, Value>, RequestError> {
    match object.get("params") {
        None => Ok(Map::new()),
        Some(Value::Object(members)) => Ok(members.clone()),
        Some(other) => Err(RequestError {
            id: Some(id.clone()),
            method: Some(method.to_owned()),
            error: SolarError::new(
                Reason::TypeMismatch,
                format!(
                    "The params of {method} must be an object, and a {} arrived.",
                    type_name(other)
                ),
            )
            .with_detail(
                ErrorDetail::new(Status::InvalidArgument)
                    .field("params")
                    .expected("object")
                    .received(brief(other))
                    .hint(
                        "Pass parameters by name, e.g. {\"message\":\"hi\"}. Omit params \
                         entirely when the API takes none; null is not an object.",
                    ),
            )
            .as_envelope_failure(),
        }),
    }
}

/// The body of a successful response.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SuccessBody {
    /// The output of the API, shaped by its `output_schema`.
    pub data: Value,
    /// Execution metadata.
    pub meta: Meta,
    /// Everything the caller should know that did not prevent success.
    pub warnings: Vec<Warning>,
}

/// The body of an error response.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ErrorBody {
    /// The JSON-RPC code, from the table in section 6.1 of the contract.
    pub code: i32,
    /// One sentence of English prose, written for a person reading a terminal.
    pub message: String,
    /// Everything a machine needs in order to react.
    pub data: ErrorData,
}

/// The machine readable half of an error response.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ErrorData {
    /// The kind of failure.
    pub status: Status,
    /// Which failure exactly.
    pub reason: Reason,
    /// At least one entry, never empty.
    pub details: Vec<ErrorDetail>,
    /// Execution metadata, the same block a successful response carries.
    pub meta: Meta,
}

/// One response, exactly one of `result` or `error`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Response {
    /// Always `"2.0"`.
    pub jsonrpc: &'static str,
    /// The identifier of the request, `null` when it could not be read.
    pub id: Option<RequestId>,
    /// Present on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<SuccessBody>,
    /// Present on failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorBody>,
}

impl Response {
    /// A successful response.
    #[must_use]
    pub fn success(id: Option<RequestId>, data: Value, meta: Meta, warnings: Vec<Warning>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(SuccessBody {
                data,
                meta,
                warnings,
            }),
            error: None,
        }
    }

    /// A failed response. The error gains its fallback detail entry here, so that the
    /// promise that `details` is never empty holds however the error was built.
    #[must_use]
    pub fn failure(id: Option<RequestId>, error: SolarError, meta: Meta) -> Self {
        let error = error.ensure_detailed();
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(ErrorBody {
                code: error.code(),
                message: error.message().to_owned(),
                data: ErrorData {
                    status: error.status(),
                    reason: error.reason(),
                    details: error.details().to_vec(),
                    meta,
                },
            }),
        }
    }

    /// Whether this response reports success.
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.result.is_some()
    }

    /// The status of a failed response, `None` when it succeeded.
    #[must_use]
    pub fn status(&self) -> Option<Status> {
        self.error.as_ref().map(|error| error.data.status)
    }

    /// The single line that goes on standard output, without the newline.
    ///
    /// Serialisation of a response cannot fail in practice, since every value inside one
    /// came from `serde_json` in the first place. Should it ever fail, this returns a hand
    /// written `INTERNAL` envelope rather than nothing at all: the one promise SOLAR makes
    /// above all others is that a call always gets an answer.
    #[must_use]
    pub fn to_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|error| Self::fallback_line(&error.to_string()))
    }

    /// The same response, indented, for a person.
    #[must_use]
    pub fn to_pretty(&self) -> String {
        serde_json::to_string_pretty(self)
            .unwrap_or_else(|error| Self::fallback_line(&error.to_string()))
    }

    /// The last resort envelope, built without serde so that it cannot fail in turn.
    fn fallback_line(detail: &str) -> String {
        let escaped: String = detail
            .chars()
            .flat_map(|c| match c {
                '"' => vec!['\\', '"'],
                '\\' => vec!['\\', '\\'],
                '\n' | '\r' | '\t' => vec![' '],
                c if (c as u32) < 0x20 => vec![' '],
                c => vec![c],
            })
            .collect();
        format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{{\"code\":-32603,\
             \"message\":\"The response could not be serialised.\",\"data\":{{\
             \"status\":\"INTERNAL\",\"reason\":\"SERIALIZATION_FAILED\",\"details\":[{{\
             \"field\":null,\"expected\":\"a serialisable response\",\"received\":\"{escaped}\",\
             \"hint\":\"Report this with the exact request that caused it.\",\
             \"docs\":\"docs/ERRORS.md#internal\"}}],\"meta\":null}}}}}}"
        )
    }
}

/// The protocol name, re-exported for callers that build their own envelopes.
#[must_use]
pub const fn protocol_name() -> &'static str {
    PROTOCOL
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err_of(line: &str) -> RequestError {
        parse_request(line).expect_err("this line should not parse")
    }

    #[test]
    fn a_well_formed_request_parses() {
        let request =
            parse_request(r#"{"jsonrpc":"2.0","id":1,"method":"solar.ping","params":{"a":1}}"#)
                .unwrap();
        assert_eq!(request.id, RequestId::Number(1.into()));
        assert_eq!(request.method, "solar.ping");
        assert_eq!(request.params.get("a"), Some(&Value::from(1)));
    }

    #[test]
    fn absent_params_are_an_empty_object() {
        let request = parse_request(r#"{"jsonrpc":"2.0","id":"x","method":"solar.ping"}"#).unwrap();
        assert!(request.params.is_empty());
        assert_eq!(request.id, RequestId::Text("x".to_owned()));
    }

    #[test]
    fn unknown_members_of_the_envelope_are_ignored_as_json_rpc_requires() {
        let request =
            parse_request(r#"{"jsonrpc":"2.0","id":1,"method":"solar.ping","extra":true}"#)
                .unwrap();
        assert_eq!(request.method, "solar.ping");
    }

    #[test]
    fn broken_json_is_a_parse_error_with_the_json_rpc_code() {
        let failed = err_of("{not json");
        assert_eq!(failed.error.reason(), Reason::ParseError);
        assert_eq!(failed.error.code(), -32700);
        assert_eq!(failed.id, None);
        assert!(!failed.error.details().is_empty());
    }

    #[test]
    fn a_batch_is_unimplemented_rather_than_invalid() {
        let failed = err_of(r#"[{"jsonrpc":"2.0","id":1,"method":"solar.ping"}]"#);
        assert_eq!(failed.error.reason(), Reason::BatchNotSupported);
        assert_eq!(failed.error.status(), Status::Unimplemented);
        assert_eq!(failed.error.code(), -32007);
    }

    #[test]
    fn a_notification_is_refused_with_the_invalid_request_code() {
        for line in [
            r#"{"jsonrpc":"2.0","method":"solar.ping"}"#,
            r#"{"jsonrpc":"2.0","id":null,"method":"solar.ping"}"#,
        ] {
            let failed = err_of(line);
            assert_eq!(failed.error.reason(), Reason::NotificationNotSupported);
            assert_eq!(failed.error.code(), -32600);
            assert_eq!(failed.id, None, "the response must carry id: null");
        }
    }

    #[test]
    fn an_id_of_the_wrong_type_is_refused_before_anything_else() {
        let failed = err_of(r#"{"jsonrpc":"1.0","id":[1],"method":"NOPE"}"#);
        assert_eq!(failed.error.reason(), Reason::TypeMismatch);
        assert_eq!(failed.error.details()[0].field.as_deref(), Some("id"));
    }

    #[test]
    fn the_wrong_json_rpc_version_is_refused_and_the_id_still_comes_back() {
        let failed = err_of(r#"{"jsonrpc":"1.0","id":7,"method":"solar.ping"}"#);
        assert_eq!(failed.error.reason(), Reason::InvalidValue);
        assert_eq!(failed.error.code(), -32600);
        assert_eq!(failed.id, Some(RequestId::Number(7.into())));
    }

    #[test]
    fn a_missing_jsonrpc_member_is_a_missing_field() {
        let failed = err_of(r#"{"id":7,"method":"solar.ping"}"#);
        assert_eq!(failed.error.reason(), Reason::MissingField);
        assert_eq!(failed.error.details()[0].field.as_deref(), Some("jsonrpc"));
    }

    #[test]
    fn a_method_name_outside_the_rule_is_refused_with_a_usable_hint() {
        let failed = err_of(r#"{"jsonrpc":"2.0","id":1,"method":"Solar.Ping"}"#);
        assert_eq!(failed.error.reason(), Reason::InvalidValue);
        assert_eq!(failed.method.as_deref(), Some("Solar.Ping"));
        let hint = failed.error.details()[0].hint.clone().unwrap();
        assert!(hint.contains("solar.ping"), "{hint}");
    }

    #[test]
    fn params_that_are_not_an_object_are_refused_and_name_the_method() {
        let failed = err_of(r#"{"jsonrpc":"2.0","id":1,"method":"solar.ping","params":[1,2]}"#);
        assert_eq!(failed.error.reason(), Reason::TypeMismatch);
        assert_eq!(failed.method.as_deref(), Some("solar.ping"));
        assert!(failed.error.message().contains("solar.ping"));
    }

    #[test]
    fn null_params_are_refused_because_null_is_not_an_object() {
        let failed = err_of(r#"{"jsonrpc":"2.0","id":1,"method":"solar.ping","params":null}"#);
        assert_eq!(failed.error.reason(), Reason::TypeMismatch);
        assert!(
            failed.error.details()[0]
                .hint
                .clone()
                .unwrap()
                .contains("null is not an object")
        );
    }
}

#[cfg(test)]
mod response_tests {
    use super::*;
    use crate::warning::{Warning, WarningCode};

    fn meta() -> Meta {
        Meta::new(
            Some(RequestId::Number(1.into())),
            Some("solar.ping".to_owned()),
            Some("1.0.0".to_owned()),
            "2026-09-26T21:41:03.123456Z".to_owned(),
            42,
        )
    }

    #[test]
    fn a_success_carries_data_meta_and_warnings_and_no_error() {
        let response = Response::success(
            Some(RequestId::Number(1.into())),
            Value::from(true),
            meta(),
            vec![],
        );
        let json: Value = serde_json::from_str(&response.to_line()).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 1);
        assert!(
            json.get("error").is_none(),
            "a success must not carry an error member"
        );
        let result = &json["result"];
        assert_eq!(result["data"], Value::from(true));
        assert!(result.get("meta").is_some());
        assert_eq!(result["warnings"], Value::Array(vec![]));
    }

    #[test]
    fn warnings_travel_beside_a_successful_result() {
        let warnings = vec![Warning::new(
            WarningCode::BuildMetadataIncomplete,
            "The commit is unknown.",
        )];
        let response = Response::success(None, Value::Null, meta(), warnings);
        let json: Value = serde_json::from_str(&response.to_line()).unwrap();
        assert_eq!(
            json["result"]["warnings"][0]["code"],
            "BUILD_METADATA_INCOMPLETE"
        );
    }

    #[test]
    fn a_failure_carries_the_four_members_of_data_and_no_result() {
        let error = SolarError::new(Reason::MethodNotFound, "Method not found: x.y.");
        let response = Response::failure(Some(RequestId::Text("a".to_owned())), error, meta());
        let json: Value = serde_json::from_str(&response.to_line()).unwrap();
        assert!(
            json.get("result").is_none(),
            "a failure must not carry a result member"
        );
        assert_eq!(json["error"]["code"], -32601);
        let data = &json["error"]["data"];
        assert_eq!(data["status"], "NOT_FOUND");
        assert_eq!(data["reason"], "METHOD_NOT_FOUND");
        assert_eq!(
            data.as_object().unwrap().len(),
            4,
            "data has exactly four members"
        );
        assert!(
            !data["details"].as_array().unwrap().is_empty(),
            "details is never empty"
        );
    }

    #[test]
    fn a_response_is_one_single_line() {
        let response = Response::success(None, Value::from("a\nb"), meta(), vec![]);
        let line = response.to_line();
        assert!(
            !line.contains('\n'),
            "a response must never contain a raw newline"
        );
    }

    #[test]
    fn the_fallback_envelope_is_valid_json_even_with_hostile_input() {
        let line = Response::fallback_line("a \"quote\", a \\ and a \n newline");
        let json: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(json["error"]["data"]["reason"], "SERIALIZATION_FAILED");
        assert_eq!(json["error"]["code"], -32603);
    }

    #[test]
    fn a_long_string_is_cut_before_it_is_echoed_back() {
        let long = "x".repeat(MAX_RECEIVED_BYTES * 2);
        let shortened = brief(&Value::String(long));
        let text = shortened.as_str().unwrap();
        assert!(text.len() < MAX_RECEIVED_BYTES + 16);
        assert!(text.ends_with("[...]"));
    }

    #[test]
    fn a_big_composite_value_is_described_rather_than_echoed() {
        let array = Value::Array((0..500).map(Value::from).collect());
        let shortened = brief(&array);
        assert!(
            shortened
                .as_str()
                .unwrap()
                .starts_with("[an array of 500 items")
        );

        let small = Value::Array(vec![Value::from(1)]);
        assert_eq!(brief(&small), small);
    }
}
