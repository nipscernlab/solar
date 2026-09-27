//! The error type every SOLAR call can fail with, and the detail entries it carries.
//!
//! An error answers five questions: what was expected, what arrived, where, why, and what
//! to do about it. The first four live in [`ErrorDetail`], the fifth in its `hint`. The
//! status is never stored: it is derived from the reason, so the two can never disagree.

use serde::Serialize;
use serde_json::Value;

use crate::reason::Reason;
use crate::status::Status;

/// One entry of the `details` array of an error.
///
/// Every member is always serialised, `null` where it does not apply, so that a caller can
/// read the same shape every time instead of testing for absence.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ErrorDetail {
    /// Where the problem is: a JSON pointer into `params`, or a short name such as `method`.
    pub field: Option<String>,
    /// What that position should have held, or the suggested value for a suggestion.
    pub expected: Option<String>,
    /// What arrived there, `null` when nothing did.
    pub received: Value,
    /// What to do about it, in the imperative.
    pub hint: Option<String>,
    /// A link into the error catalogue, always `docs/ERRORS.md#<status in lowercase>`.
    pub docs: String,
}

impl ErrorDetail {
    /// An empty detail entry whose `docs` already points at the section for `status`.
    #[must_use]
    pub fn new(status: Status) -> Self {
        Self {
            field: None,
            expected: None,
            received: Value::Null,
            hint: None,
            docs: status.docs(),
        }
    }

    /// Sets where the problem is.
    #[must_use]
    pub fn field(mut self, field: impl Into<String>) -> Self {
        self.field = Some(field.into());
        self
    }

    /// Sets what was expected at that position.
    #[must_use]
    pub fn expected(mut self, expected: impl Into<String>) -> Self {
        self.expected = Some(expected.into());
        self
    }

    /// Sets what arrived at that position.
    #[must_use]
    pub fn received(mut self, received: impl Into<Value>) -> Self {
        self.received = received.into();
        self
    }

    /// Sets what the caller should do next.
    #[must_use]
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// A failure of a SOLAR call, ready to be turned into an error response.
///
/// The [`Status`] is derived from the [`Reason`] and is never stored separately. The
/// JSON-RPC code is derived from the status, except for the two envelope level codes that
/// JSON-RPC 2.0 reserves for a parse error and for a malformed request.
#[derive(Debug, Clone, PartialEq)]
pub struct SolarError {
    reason: Reason,
    message: String,
    details: Vec<ErrorDetail>,
    envelope_code: Option<i32>,
}

impl SolarError {
    /// A new error. The message is one sentence of English prose, capitalised, with a full
    /// stop at the end, written for a person reading a terminal.
    #[must_use]
    pub fn new(reason: Reason, message: impl Into<String>) -> Self {
        Self {
            reason,
            message: message.into(),
            details: Vec::new(),
            envelope_code: None,
        }
    }

    /// Adds one detail entry.
    #[must_use]
    pub fn with_detail(mut self, detail: ErrorDetail) -> Self {
        self.details.push(detail);
        self
    }

    /// Adds several detail entries.
    #[must_use]
    pub fn with_details(mut self, details: impl IntoIterator<Item = ErrorDetail>) -> Self {
        self.details.extend(details);
        self
    }

    /// Marks this error as a failure of the JSON-RPC envelope itself, which JSON-RPC 2.0
    /// requires to carry the code `-32600`.
    ///
    /// The mark is ignored, and the code stays the one the status table gives, when the
    /// reason is not one of the envelope reasons listed in section 6.1 of the contract. An
    /// invalid code can therefore never reach the wire.
    #[must_use]
    pub fn as_envelope_failure(mut self) -> Self {
        if Self::is_envelope_reason(self.reason) {
            self.envelope_code = Some(-32600);
        }
        self
    }

    /// Whether a reason is allowed to carry the envelope code `-32600`.
    #[must_use]
    pub const fn is_envelope_reason(reason: Reason) -> bool {
        matches!(
            reason,
            Reason::MissingField
                | Reason::TypeMismatch
                | Reason::InvalidValue
                | Reason::NotificationNotSupported
                | Reason::BatchEmpty
        )
    }

    /// The reason, the fine grained half of the error.
    #[must_use]
    pub const fn reason(&self) -> Reason {
        self.reason
    }

    /// The status, derived from the reason.
    #[must_use]
    pub const fn status(&self) -> Status {
        self.reason.status()
    }

    /// The sentence written for a person.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The detail entries, never empty once the error reaches the wire.
    #[must_use]
    pub fn details(&self) -> &[ErrorDetail] {
        &self.details
    }

    /// The JSON-RPC code, from the status table or from the envelope exception.
    #[must_use]
    pub fn code(&self) -> i32 {
        match self.envelope_code {
            Some(code) => code,
            None if self.reason == Reason::ParseError => -32700,
            None => self.status().code(),
        }
    }

    /// Returns the error with at least one detail entry, adding a bare one that points at
    /// the catalogue when the caller supplied none.
    ///
    /// The contract promises that `details` is never empty, and this is where that promise
    /// is kept, at the last moment before the error is serialised.
    #[must_use]
    pub fn ensure_detailed(mut self) -> Self {
        if self.details.is_empty() {
            let status = self.status();
            let reason = self.reason;
            self.details.push(
                ErrorDetail::new(status)
                    .hint(format!("Read {} for what {reason} means.", status.docs())),
            );
        }
        self
    }
}

impl std::fmt::Display for SolarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} / {}: {}", self.status(), self.reason, self.message)
    }
}

impl std::error::Error for SolarError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_code_comes_from_the_status_table() {
        let error = SolarError::new(Reason::MethodNotFound, "Method not found: x.y.");
        assert_eq!(error.status(), Status::NotFound);
        assert_eq!(error.code(), -32601);
    }

    #[test]
    fn a_parse_error_always_carries_the_json_rpc_parse_code() {
        let error = SolarError::new(Reason::ParseError, "The line is not valid JSON.");
        assert_eq!(error.status(), Status::InvalidArgument);
        assert_eq!(error.code(), -32700);
    }

    #[test]
    fn an_envelope_failure_carries_minus_32600_only_for_envelope_reasons() {
        let envelope =
            SolarError::new(Reason::MissingField, "Missing field.").as_envelope_failure();
        assert_eq!(envelope.code(), -32600);

        let inside_params = SolarError::new(Reason::MissingField, "Missing field.");
        assert_eq!(inside_params.code(), -32602);

        let not_allowed =
            SolarError::new(Reason::MethodNotFound, "Method not found.").as_envelope_failure();
        assert_eq!(
            not_allowed.code(),
            -32601,
            "the mark must be ignored for this reason"
        );
    }

    #[test]
    fn a_bare_error_gains_a_detail_pointing_at_the_catalogue() {
        let error = SolarError::new(Reason::HandlerPanic, "Boom.").ensure_detailed();
        assert_eq!(error.details().len(), 1);
        assert_eq!(error.details()[0].docs, "docs/ERRORS.md#internal");
        assert!(
            error.details()[0]
                .hint
                .as_deref()
                .unwrap()
                .contains("HANDLER_PANIC")
        );
    }

    #[test]
    fn a_detail_serialises_with_all_five_members_present() {
        let detail = ErrorDetail::new(Status::InvalidArgument)
            .field("/api")
            .expected("string");
        let json = serde_json::to_value(&detail).unwrap();
        for member in ["field", "expected", "received", "hint", "docs"] {
            assert!(
                json.get(member).is_some(),
                "{member} is missing from a serialised detail"
            );
        }
        assert_eq!(json["received"], Value::Null);
        assert_eq!(json["docs"], "docs/ERRORS.md#invalid_argument");
    }
}
