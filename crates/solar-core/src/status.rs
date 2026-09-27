//! The eleven canonical status codes and their fixed mapping to JSON-RPC codes.
//!
//! The list is the one used by Google and by gRPC. A caller that already knows those codes
//! needs to learn nothing new, and a caller that does not can read
//! [`docs/ERRORS.md`](../../../docs/ERRORS.md).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The kind of a failure, in the vocabulary shared with Google and gRPC.
///
/// The JSON-RPC integer code of a response is a function of this value, given by
/// [`Status::code`]. The only exceptions are the two envelope level codes, `-32700` and
/// `-32600`, which are described in section 6.1 of the contract.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Status {
    /// The caller sent something SOLAR cannot accept.
    InvalidArgument,
    /// A named thing does not exist: a method, an API, a file.
    NotFound,
    /// Creating something that is already there.
    AlreadyExists,
    /// The system is not in a state where the call can run.
    FailedPrecondition,
    /// The operating system refused access.
    PermissionDenied,
    /// A declared limit was reached.
    ResourceExhausted,
    /// The call ran past its timeout.
    DeadlineExceeded,
    /// A dependency SOLAR needs is not available right now.
    Unavailable,
    /// Valid, understood, not built yet.
    Unimplemented,
    /// A bug in SOLAR. A panic reaches the caller as this.
    Internal,
    /// The caller asked for the call to stop, and it stopped.
    Cancelled,
    /// A failure that could not be classified.
    Unknown,
}

impl Status {
    /// Every status, in the order they appear in the contract.
    pub const ALL: [Status; 12] = [
        Status::InvalidArgument,
        Status::NotFound,
        Status::AlreadyExists,
        Status::FailedPrecondition,
        Status::PermissionDenied,
        Status::ResourceExhausted,
        Status::DeadlineExceeded,
        Status::Unavailable,
        Status::Unimplemented,
        Status::Internal,
        Status::Cancelled,
        Status::Unknown,
    ];

    /// The canonical spelling, in upper case, as it travels on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Status::InvalidArgument => "INVALID_ARGUMENT",
            Status::NotFound => "NOT_FOUND",
            Status::AlreadyExists => "ALREADY_EXISTS",
            Status::FailedPrecondition => "FAILED_PRECONDITION",
            Status::PermissionDenied => "PERMISSION_DENIED",
            Status::ResourceExhausted => "RESOURCE_EXHAUSTED",
            Status::DeadlineExceeded => "DEADLINE_EXCEEDED",
            Status::Unavailable => "UNAVAILABLE",
            Status::Unimplemented => "UNIMPLEMENTED",
            Status::Internal => "INTERNAL",
            Status::Cancelled => "CANCELLED",
            Status::Unknown => "UNKNOWN",
        }
    }

    /// The JSON-RPC code this status maps to, per section 6.1 of the contract.
    #[must_use]
    pub const fn code(self) -> i32 {
        match self {
            Status::InvalidArgument => -32602,
            Status::NotFound => -32601,
            Status::AlreadyExists => -32001,
            Status::FailedPrecondition => -32002,
            Status::PermissionDenied => -32003,
            Status::ResourceExhausted => -32004,
            Status::DeadlineExceeded => -32005,
            Status::Unavailable => -32006,
            Status::Unimplemented => -32007,
            Status::Internal => -32603,
            Status::Cancelled => -32008,
            Status::Unknown => -32099,
        }
    }

    /// The anchor every detail entry points at, for example `docs/ERRORS.md#not_found`.
    #[must_use]
    pub fn docs(self) -> String {
        format!("docs/ERRORS.md#{}", self.as_str().to_lowercase())
    }

    /// The process exit code `solar call` uses for this status, per section 13.
    #[must_use]
    pub const fn exit_code(self) -> u8 {
        match self {
            Status::InvalidArgument => 2,
            Status::NotFound => 3,
            Status::AlreadyExists => 4,
            Status::FailedPrecondition => 5,
            Status::PermissionDenied => 6,
            Status::ResourceExhausted => 7,
            Status::DeadlineExceeded => 8,
            Status::Unavailable => 9,
            Status::Unimplemented => 10,
            Status::Internal => 11,
            Status::Unknown => 12,
            Status::Cancelled => 13,
        }
    }

    /// Parses the canonical upper case spelling. `None` when the name is not canonical.
    #[must_use]
    pub fn from_str_canonical(name: &str) -> Option<Status> {
        Status::ALL.into_iter().find(|s| s.as_str() == name)
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_spelling_round_trips_through_serde() {
        for status in Status::ALL {
            let json = serde_json::to_string(&status).unwrap();
            assert_eq!(json, format!("\"{}\"", status.as_str()));
            let back: Status = serde_json::from_str(&json).unwrap();
            assert_eq!(back, status);
        }
    }

    #[test]
    fn every_status_has_its_own_code_and_exit_code() {
        let mut codes: Vec<i32> = Status::ALL.iter().map(|s| s.code()).collect();
        codes.sort_unstable();
        let unique = codes.len();
        codes.dedup();
        assert_eq!(codes.len(), unique, "two statuses share a JSON-RPC code");

        let mut exits: Vec<u8> = Status::ALL.iter().map(|s| s.exit_code()).collect();
        exits.sort_unstable();
        let unique = exits.len();
        exits.dedup();
        assert_eq!(exits.len(), unique, "two statuses share an exit code");
        assert!(!exits.contains(&0), "no failure may exit zero");
        assert!(
            !exits.contains(&70),
            "70 is reserved for a response that could not be sent"
        );
    }

    #[test]
    fn application_codes_stay_inside_the_range_json_rpc_reserves() {
        for status in Status::ALL {
            let code = status.code();
            let reserved = [-32700, -32600, -32601, -32602, -32603];
            assert!(
                reserved.contains(&code) || (-32099..=-32000).contains(&code),
                "{status} uses {code}, which JSON-RPC 2.0 does not reserve for applications"
            );
        }
    }

    #[test]
    fn parsing_a_canonical_name_gives_the_same_status_back() {
        for status in Status::ALL {
            assert_eq!(Status::from_str_canonical(status.as_str()), Some(status));
        }
        assert_eq!(Status::from_str_canonical("NOPE"), None);
        assert_eq!(Status::from_str_canonical("not_found"), None);
    }
}
