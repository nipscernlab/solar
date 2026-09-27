//! What an API can see about the call it is serving.
//!
//! The context is the only thing a handler receives besides its own parameters. It knows
//! which call this is, how much time is left, what else is registered, and it is where
//! warnings are collected.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::protocol::RequestId;
use crate::registry::Registry;
use crate::warning::{Warning, WarningCode};

/// The call a handler is serving.
///
/// A context is created once per call and shared with the worker thread the handler runs
/// on, which is why the warnings sit behind a lock.
#[derive(Debug)]
pub struct Context {
    request_id: Option<RequestId>,
    method: String,
    started: Instant,
    budget: Duration,
    registry: Arc<Registry>,
    warnings: Mutex<Vec<Warning>>,
}

impl Context {
    /// Builds the context of one call.
    #[must_use]
    pub fn new(
        request_id: Option<RequestId>,
        method: impl Into<String>,
        budget: Duration,
        registry: Arc<Registry>,
    ) -> Self {
        Self {
            request_id,
            method: method.into(),
            started: Instant::now(),
            budget,
            registry,
            warnings: Mutex::new(Vec::new()),
        }
    }

    /// The identifier of the request, echoed into `meta.request_id`.
    #[must_use]
    pub const fn request_id(&self) -> Option<&RequestId> {
        self.request_id.as_ref()
    }

    /// The method being served.
    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }

    /// Everything that is registered, so that an API can answer about the others.
    #[must_use]
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// How long this call has already taken.
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// How much of the budget is left, `Duration::ZERO` once it is spent.
    ///
    /// A handler that starts an external program gives it less than this, so that the
    /// program is killed before dispatch gives up on the handler.
    #[must_use]
    pub fn remaining(&self) -> Duration {
        self.budget.saturating_sub(self.started.elapsed())
    }

    /// The whole budget of this call.
    #[must_use]
    pub const fn budget(&self) -> Duration {
        self.budget
    }

    /// Records something the caller should know about a call that still succeeded.
    pub fn warn(&self, code: WarningCode, message: impl Into<String>) {
        let warning = Warning::new(code, message);
        let mut warnings = self
            .warnings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        warnings.push(warning);
    }

    /// Every warning recorded so far, in the order they were recorded.
    #[must_use]
    pub fn warnings(&self) -> Vec<Warning> {
        self.warnings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::RegistryBuilder;

    fn context(budget: Duration) -> Context {
        let registry = Arc::new(RegistryBuilder::new().build().unwrap());
        Context::new(
            Some(RequestId::Number(1.into())),
            "solar.ping",
            budget,
            registry,
        )
    }

    #[test]
    fn warnings_come_back_in_the_order_they_were_recorded() {
        let ctx = context(Duration::from_secs(1));
        assert!(ctx.warnings().is_empty());
        ctx.warn(WarningCode::BuildMetadataIncomplete, "First.");
        ctx.warn(WarningCode::BuildMetadataIncomplete, "Second.");
        let warnings = ctx.warnings();
        assert_eq!(warnings.len(), 2);
        assert_eq!(warnings[0].message, "First.");
        assert_eq!(warnings[1].code, WarningCode::BuildMetadataIncomplete);
    }

    #[test]
    fn a_spent_budget_leaves_zero_rather_than_going_negative() {
        let ctx = context(Duration::ZERO);
        assert_eq!(ctx.remaining(), Duration::ZERO);
        assert_eq!(ctx.budget(), Duration::ZERO);
    }

    #[test]
    fn the_context_knows_which_call_it_is_serving() {
        let ctx = context(Duration::from_millis(500));
        assert_eq!(ctx.method(), "solar.ping");
        assert_eq!(ctx.request_id(), Some(&RequestId::Number(1.into())));
        assert!(ctx.remaining() <= Duration::from_millis(500));
        assert!(ctx.registry().is_empty());
    }
}
