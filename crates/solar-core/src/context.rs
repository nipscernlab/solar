//! What an API can see about the call it is serving.
//!
//! The context is the only thing a handler receives besides its own parameters. It knows
//! which call this is, how much time is left, what else is registered, and it is where
//! warnings are collected.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::cancel::Cancellation;
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
    cancellation: Cancellation,
    session: Option<Arc<crate::session::SessionState>>,
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
            cancellation: Cancellation::new(),
            session: None,
        }
    }

    /// The same context, watching a token somebody else can cancel.
    ///
    /// Dispatch uses this inside a session, where `solar.cancel` holds the other handle.
    /// A context built with [`Context::new`] watches a token nobody else holds, so
    /// [`Context::is_cancelled`] is always false, which is what a one-shot call wants.
    #[must_use]
    pub fn cancellable(mut self, cancellation: Cancellation) -> Self {
        self.cancellation = cancellation;
        self
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

    /// The same context, able to see the session this call is running in.
    ///
    /// Only `solar.cancel` needs this, and only inside a session. A call made by
    /// `solar call`, which is one call and then exit, has no session to see.
    #[must_use]
    pub fn in_session(mut self, session: Arc<crate::session::SessionState>) -> Self {
        self.session = Some(session);
        self
    }

    /// The session this call is running in, when it is running in one.
    #[must_use]
    pub fn session(&self) -> Option<&Arc<crate::session::SessionState>> {
        self.session.as_ref()
    }

    /// Whether the caller has asked for this call to stop.
    ///
    /// A handler checks this at points where stopping is safe, and returns
    /// [`Context::cancelled`] when it is true. A handler that never checks is not a
    /// special case: section 9.4 of the contract says what happens to it.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    /// The token itself, for a handler that hands it to something it is waiting on.
    #[must_use]
    pub const fn cancellation(&self) -> &Cancellation {
        &self.cancellation
    }

    /// The error a handler returns when it stops because it was asked to.
    #[must_use]
    pub fn cancelled(&self) -> crate::error::SolarError {
        Cancellation::as_error("a call the handler stopped when it was asked to")
    }

    /// Records something the caller should know about a call that still succeeded.
    pub fn warn(&self, code: WarningCode, message: impl Into<String>) {
        let warning = Warning::new(code, message);
        let mut warnings = self
            .warnings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        warnings.push(warning);
    }

    /// Every warning recorded so far, in the order they were recorded.
    #[must_use]
    pub fn warnings(&self) -> Vec<Warning> {
        self.warnings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
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
    fn time_really_passes_for_a_call() {
        let ctx = context(Duration::from_millis(500));
        std::thread::sleep(Duration::from_millis(5));
        assert!(
            ctx.elapsed() >= Duration::from_millis(5),
            "elapsed is how long the call has taken, not zero: {:?}",
            ctx.elapsed()
        );
        assert!(
            ctx.remaining() < Duration::from_millis(500),
            "what is left is the budget minus what has passed: {:?}",
            ctx.remaining()
        );
        assert!(ctx.remaining() > Duration::ZERO, "and it is not spent yet");
    }

    #[test]
    fn a_context_sees_the_session_it_was_given_and_no_other() {
        let plain = context(Duration::from_millis(500));
        assert!(
            plain.session().is_none(),
            "a call outside a session has no session to see"
        );

        let session = Arc::new(crate::session::SessionState::new());
        let inside = context(Duration::from_millis(500)).in_session(Arc::clone(&session));
        let seen = inside.session().expect("a call inside a session sees it");
        assert!(
            Arc::ptr_eq(seen, &session),
            "the session it sees is the one it was given, not another one"
        );
    }

    #[test]
    fn a_context_nobody_holds_the_token_of_is_never_cancelled() {
        let ctx = context(Duration::from_millis(500));
        assert!(!ctx.is_cancelled());
        // The handle the context keeps is the only one, so nothing can cancel it.
        assert!(!ctx.cancellation().is_cancelled());
    }

    #[test]
    fn a_cancellable_context_sees_the_cancellation() {
        let token = Cancellation::new();
        let ctx = context(Duration::from_millis(500)).cancellable(token.clone());
        assert!(!ctx.is_cancelled());
        token.cancel();
        assert!(ctx.is_cancelled());
        assert_eq!(
            ctx.cancelled().reason(),
            crate::reason::Reason::CallCancelled
        );
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
