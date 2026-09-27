//! Warnings: what a caller should know about a call that nevertheless succeeded.
//!
//! The codes are closed and documented in `docs/ERRORS.md`, exactly like reasons, and the
//! same test keeps the code and the catalogue in step.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

macro_rules! warning_codes {
    ($($variant:ident => $wire:literal, $doc:literal;)+) => {
        /// The closed set of warning codes.
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
            JsonSchema,
        )]
        pub enum WarningCode {
            $(
                #[doc = $doc]
                #[serde(rename = $wire)]
                $variant,
            )+
        }

        impl WarningCode {
            /// Every warning code in the catalogue.
            pub const ALL: &'static [WarningCode] = &[$(WarningCode::$variant),+];

            /// The canonical spelling, in upper case, as it travels on the wire.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self { $(WarningCode::$variant => $wire),+ }
            }
        }
    };
}

warning_codes! {
    BuildMetadataIncomplete => "BUILD_METADATA_INCOMPLETE",
        "Part of the build metadata was not available when SOLAR was compiled.";
}

impl std::fmt::Display for WarningCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One warning attached to a successful response.
///
/// Warnings never change the shape of `data`. They exist so that a caller, and above all an
/// agent, is never left guessing why a result looks thinner than it expected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Warning {
    /// The closed constant that says what kind of warning this is.
    pub code: WarningCode,
    /// One sentence of English prose, capitalised, ending in a full stop.
    pub message: String,
}

impl Warning {
    /// Builds a warning from a code and a message.
    #[must_use]
    pub fn new(code: WarningCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_spelling_round_trips_through_serde() {
        for code in WarningCode::ALL {
            let json = serde_json::to_string(code).unwrap();
            assert_eq!(json, format!("\"{}\"", code.as_str()));
            let back: WarningCode = serde_json::from_str(&json).unwrap();
            assert_eq!(back, *code);
        }
    }

    #[test]
    fn a_warning_serialises_to_code_and_message() {
        let warning = Warning::new(
            WarningCode::BuildMetadataIncomplete,
            "The commit is unknown.",
        );
        let json = serde_json::to_value(&warning).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "code": "BUILD_METADATA_INCOMPLETE",
                "message": "The commit is unknown."
            })
        );
    }
}
