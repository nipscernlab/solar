//! The token a call carries, and the outcomes cancelling one can have.
//!
//! Cancelling is cooperative: SOLAR never kills a thread, for the same reason section 10
//! of the contract gives for abandoning one. A token is a flag a handler reads at points
//! where stopping is safe, and section 9.4 says what a handler does with it.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

use crate::error::{ErrorDetail, SolarError};
use crate::reason::Reason;
use crate::status::Status;

/// The flag one call watches, shared between the handler and whoever cancels it.
///
/// Cloning gives another handle on the same flag. A token that nobody holds the other end
/// of is never cancelled, which is what a call outside a session gets.
///
/// ```
/// use solar_core::cancel::Cancellation;
///
/// let token = Cancellation::new();
/// assert!(!token.is_cancelled());
/// token.cancel();
/// assert!(token.is_cancelled());
/// ```
#[derive(Debug, Clone, Default)]
pub struct Cancellation {
    flag: Arc<AtomicBool>,
}

impl Cancellation {
    /// A token that has not been cancelled.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks the call watching this token to stop.
    ///
    /// Asking twice is asking once: the flag only ever goes from false to true.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    /// Whether the caller has asked for this call to stop.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    /// The error a handler returns when it stops because it was asked to.
    #[must_use]
    pub fn as_error(reason_it_stopped: &str) -> SolarError {
        SolarError::new(
            Reason::CallCancelled,
            "The caller cancelled this call, and it stopped.",
        )
        .with_detail(
            ErrorDetail::new(Status::Cancelled)
                .expected("a call the caller was still waiting for")
                .received(reason_it_stopped)
                .hint(
                    "Nothing was finished, so nothing has to be undone. Call again to ask again.",
                ),
        )
    }
}

/// What cancelling one call turned out to do, which is what `solar.cancel` reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CancelOutcome {
    /// The call had not started. It will not start, and it has already been answered
    /// with `CANCELLED`.
    CancelledWhileQueued,
    /// The call is running. Its handler has been told, and it will end with its own
    /// result or with `CANCELLED`, whichever it reaches first.
    CancellationRequested,
    /// The call was answered before the cancellation arrived. Nothing changed.
    AlreadyFinished,
    /// No call with that identifier has been seen in this session. Nothing changed.
    Unknown,
}

impl CancelOutcome {
    /// The spelling that travels on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            CancelOutcome::CancelledWhileQueued => "cancelled_while_queued",
            CancelOutcome::CancellationRequested => "cancellation_requested",
            CancelOutcome::AlreadyFinished => "already_finished",
            CancelOutcome::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for CancelOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_starts_uncancelled_and_only_ever_goes_one_way() {
        let token = Cancellation::new();
        assert!(!token.is_cancelled());
        token.cancel();
        assert!(token.is_cancelled());
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn a_clone_watches_the_same_flag() {
        let token = Cancellation::new();
        let other = token.clone();
        other.cancel();
        assert!(
            token.is_cancelled(),
            "a clone is another handle, not another flag"
        );
    }

    #[test]
    fn two_separate_tokens_are_two_separate_flags() {
        let token = Cancellation::new();
        let unrelated = Cancellation::new();
        token.cancel();
        assert!(!unrelated.is_cancelled());
    }

    #[test]
    fn the_error_a_cancelled_call_returns_carries_the_cancelled_status() {
        let error = Cancellation::as_error("cancelled while queued");
        assert_eq!(error.reason(), Reason::CallCancelled);
        assert_eq!(error.status(), Status::Cancelled);
        assert_eq!(error.code(), -32008);
        assert_eq!(error.details().len(), 1);
    }

    #[test]
    fn every_outcome_spells_itself_the_same_way_in_json_and_in_text() {
        for outcome in [
            CancelOutcome::CancelledWhileQueued,
            CancelOutcome::CancellationRequested,
            CancelOutcome::AlreadyFinished,
            CancelOutcome::Unknown,
        ] {
            let json = serde_json::to_string(&outcome).expect("an outcome is JSON");
            assert_eq!(json, format!("\"{}\"", outcome.as_str()));
            assert_eq!(outcome.to_string(), outcome.as_str());
        }
    }
}
