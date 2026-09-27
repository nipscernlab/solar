//! The metadata block that travels in every response, successful or not.

use serde::Serialize;

use crate::protocol::RequestId;

/// The protocol this build speaks.
pub const PROTOCOL: &str = "solar/1";

/// The version of this SOLAR build, taken from the crate version at compile time.
pub const SOLAR_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Execution metadata, present in every response.
///
/// It answers the questions a caller asks when something looks wrong: which build answered,
/// which API ran, when it started and how long it took.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Meta {
    /// The `id` that arrived, echoed unchanged, `null` when the request had none.
    pub request_id: Option<RequestId>,
    /// The method that arrived, `null` when there was none.
    pub method: Option<String>,
    /// The semantic version of the API that ran, `null` when no API ran.
    pub api_version: Option<String>,
    /// The version of the SOLAR build that answered.
    pub solar_version: &'static str,
    /// The protocol, always `solar/1` in this version.
    pub protocol: &'static str,
    /// When SOLAR started handling the request: RFC 3339, UTC, microseconds.
    pub started_at: String,
    /// Whole microseconds of SOLAR's own work, on a monotonic clock.
    pub duration_us: u64,
    /// The operating system this build runs on.
    pub os: &'static str,
    /// The processor architecture this build runs on.
    pub arch: &'static str,
}

impl Meta {
    /// Assembles the block from what the request carried and what the call cost.
    #[must_use]
    pub fn new(
        request_id: Option<RequestId>,
        method: Option<String>,
        api_version: Option<String>,
        started_at: String,
        duration_us: u64,
    ) -> Self {
        Self {
            request_id,
            method,
            api_version,
            solar_version: SOLAR_VERSION,
            protocol: PROTOCOL,
            started_at,
            duration_us,
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_member_is_present_even_when_it_is_null() {
        let meta = Meta::new(
            None,
            None,
            None,
            "1970-01-01T00:00:00.000000Z".to_owned(),
            0,
        );
        let json = serde_json::to_value(&meta).unwrap();
        for member in [
            "request_id",
            "method",
            "api_version",
            "solar_version",
            "protocol",
            "started_at",
            "duration_us",
            "os",
            "arch",
        ] {
            assert!(json.get(member).is_some(), "{member} is missing from meta");
        }
        assert_eq!(json["request_id"], serde_json::Value::Null);
        assert_eq!(json["protocol"], "solar/1");
    }

    #[test]
    fn the_solar_version_is_a_semantic_version() {
        let parts: Vec<&str> = SOLAR_VERSION.split('.').collect();
        assert_eq!(parts.len(), 3, "{SOLAR_VERSION} is not major.minor.patch");
        assert!(parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())));
    }
}
