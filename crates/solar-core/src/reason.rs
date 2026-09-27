//! The closed catalogue of reasons, each one bound to exactly one [`Status`].
//!
//! A reason is the fine grained half of an error: the status says what kind of failure it
//! was, the reason says which failure exactly. The catalogue is closed on purpose. Adding a
//! reason means adding a variant here and a section to `docs/ERRORS.md`, and
//! `tests/docs.rs` fails until both exist.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::status::Status;

macro_rules! reasons {
    ($($variant:ident => $wire:literal, $status:expr, $doc:literal;)+) => {
        /// Why a call failed, one constant per documented failure.
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
            JsonSchema,
        )]
        pub enum Reason {
            $(
                #[doc = $doc]
                #[serde(rename = $wire)]
                $variant,
            )+
        }

        impl Reason {
            /// Every reason in the catalogue.
            pub const ALL: &'static [Reason] = &[$(Reason::$variant),+];

            /// The canonical spelling, in upper case, as it travels on the wire.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self { $(Reason::$variant => $wire),+ }
            }

            /// The status this reason belongs to. A reason belongs to exactly one.
            #[must_use]
            pub const fn status(self) -> Status {
                match self { $(Reason::$variant => $status),+ }
            }
        }
    };
}

reasons! {
    ParseError => "PARSE_ERROR", Status::InvalidArgument,
        "The line is not valid JSON, or not valid UTF-8.";
    MissingField => "MISSING_FIELD", Status::InvalidArgument,
        "A member the contract requires is absent.";
    UnknownField => "UNKNOWN_FIELD", Status::InvalidArgument,
        "A member inside `params` that the API does not declare.";
    TypeMismatch => "TYPE_MISMATCH", Status::InvalidArgument,
        "The member is there and holds the wrong kind of value.";
    InvalidValue => "INVALID_VALUE", Status::InvalidArgument,
        "The type is right and the value is not usable.";
    NotificationNotSupported => "NOTIFICATION_NOT_SUPPORTED", Status::InvalidArgument,
        "The message has no `id`, and SOLAR does not accept notifications.";
    BatchEmpty => "BATCH_EMPTY", Status::InvalidArgument,
        "The message is an empty JSON array, which asks for nothing.";
    DuplicateId => "DUPLICATE_ID", Status::InvalidArgument,
        "Two elements of one batch carry the same id.";
    IdInFlight => "ID_IN_FLIGHT", Status::InvalidArgument,
        "The id of the request belongs to a call this session has not answered yet.";
    MethodNotFound => "METHOD_NOT_FOUND", Status::NotFound,
        "The `method` of the request is not registered.";
    ApiNotFound => "API_NOT_FOUND", Status::NotFound,
        "An API was asked about by name and is not registered.";
    MessageTooLarge => "MESSAGE_TOO_LARGE", Status::ResourceExhausted,
        "A request line went past the 16 MiB limit.";
    BatchTooLarge => "BATCH_TOO_LARGE", Status::ResourceExhausted,
        "A batch holds more elements than one line may carry.";
    QueueFull => "QUEUE_FULL", Status::ResourceExhausted,
        "The session is holding as many unanswered requests as it may hold.";
    OutputTooLarge => "OUTPUT_TOO_LARGE", Status::ResourceExhausted,
        "The API produced a response larger than the size it declares.";
    HandlerTimeout => "HANDLER_TIMEOUT", Status::DeadlineExceeded,
        "The API did not finish within the `timeout_ms` it declares.";
    ThreadSpawnFailed => "THREAD_SPAWN_FAILED", Status::Unavailable,
        "SOLAR could not start the worker thread a call runs on.";
    EnvironmentUnavailable => "ENVIRONMENT_UNAVAILABLE", Status::Unavailable,
        "SOLAR could not read something about its own process that it needs.";
    TooManyAbandoned => "TOO_MANY_ABANDONED", Status::Unavailable,
        "As many abandoned handlers are still alive as the process allows.";
    HandlerPanic => "HANDLER_PANIC", Status::Internal,
        "An API panicked, and dispatch caught the unwind.";
    SerializationFailed => "SERIALIZATION_FAILED", Status::Internal,
        "The API produced a value that could not be turned into JSON.";
    InvariantBroken => "INVARIANT_BROKEN", Status::Internal,
        "SOLAR checked something that cannot be false, and it was false.";
    CallCancelled => "CALL_CANCELLED", Status::Cancelled,
        "The caller cancelled the call, and it stopped.";
}

impl Reason {
    /// Parses the canonical upper case spelling. `None` when it is not in the catalogue.
    #[must_use]
    pub fn from_str_canonical(name: &str) -> Option<Reason> {
        Reason::ALL.iter().copied().find(|r| r.as_str() == name)
    }
}

impl std::fmt::Display for Reason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_spelling_round_trips_through_serde() {
        for reason in Reason::ALL {
            let json = serde_json::to_string(reason).unwrap();
            assert_eq!(json, format!("\"{}\"", reason.as_str()));
            let back: Reason = serde_json::from_str(&json).unwrap();
            assert_eq!(back, *reason);
        }
    }

    #[test]
    fn spellings_are_unique_and_upper_snake_case() {
        let mut seen: Vec<&str> = Reason::ALL.iter().map(|r| r.as_str()).collect();
        let total = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), total, "two reasons share a spelling");
        for reason in Reason::ALL {
            let text = reason.as_str();
            assert!(
                text.chars().all(|c| c.is_ascii_uppercase() || c == '_'),
                "{text} is not an upper case constant"
            );
            assert!(
                !text.starts_with('_') && !text.ends_with('_'),
                "{text} has a loose underscore"
            );
        }
    }

    #[test]
    fn parsing_a_canonical_name_gives_the_same_reason_back() {
        for reason in Reason::ALL {
            assert_eq!(Reason::from_str_canonical(reason.as_str()), Some(*reason));
        }
        assert_eq!(Reason::from_str_canonical("NOT_A_REASON"), None);
    }
}
