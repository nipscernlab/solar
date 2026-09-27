//! The APIs SOLAR answers to.
//!
//! One file per API, one line per API in [`build_registry`], and nothing else. There is no
//! discovery, no inventory macro and no link time magic: the list below is the list, and
//! it is the only place that has to change when an API is added.
//!
//! # Adding one
//!
//! `cargo xtask new-api <name>` writes the file from the template and adds the line here.
//! The whole procedure, and the reasoning behind it, is `docs/ADDING_AN_API.md`.
//!
//! # Using them
//!
//! ```
//! let dispatcher = solar_apis::dispatcher();
//! let response =
//!     dispatcher.handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"solar.ping"}"#);
//! assert!(response.is_success());
//! ```

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, OnceLock};

use solar_core::dispatch::Dispatcher;
use solar_core::registry::{Registry, RegistryBuilder, RegistryProblem};

pub mod os_release;
pub mod solar_cancel;
pub mod solar_describe;
pub mod solar_manifest;
pub mod solar_ping;
pub mod solar_version;
pub mod system_info;

/// Every API this build answers to.
///
/// This function is the registry. Adding an API is one `.register::<...>()` line, in
/// alphabetical order, and nothing that already exists has to change.
///
/// # Errors
///
/// Returns every rule of the template that a registered API broke. The contract tests
/// fail on the same list, so this reaching a caller means the tests were not run.
pub fn build_registry() -> Result<Registry, Vec<RegistryProblem>> {
    RegistryBuilder::new()
        .register::<solar_cancel::SolarCancel>()
        .register::<solar_describe::SolarDescribe>()
        .register::<solar_manifest::SolarManifest>()
        .register::<solar_ping::SolarPing>()
        .register::<solar_version::SolarVersion>()
        .register::<system_info::SystemInfo>()
        .build()
}

/// The registry of this process, built once.
///
/// # Errors
///
/// Returns the rules of the template that the registered APIs broke. That is a bug, and
/// the contract tests catch it long before a release, but it is returned rather than
/// panicked so that a caller still gets an answer.
pub fn registry() -> Result<&'static Arc<Registry>, &'static Vec<RegistryProblem>> {
    static REGISTRY: OnceLock<Result<Arc<Registry>, Vec<RegistryProblem>>> = OnceLock::new();
    REGISTRY
        .get_or_init(|| build_registry().map(Arc::new))
        .as_ref()
}

/// A dispatcher over the registry of this process.
///
/// A registry that did not build still produces a dispatcher, one that answers every call
/// with the same `INTERNAL` error naming every rule that was broken. Something always
/// answers.
#[must_use]
pub fn dispatcher() -> Dispatcher {
    match registry() {
        Ok(registry) => Dispatcher::new(Arc::clone(registry)),
        Err(problems) => Dispatcher::broken(problems.clone()),
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    //! What the tests of the individual APIs need in order to call them directly.

    use std::sync::Arc;
    use std::time::Duration;

    use solar_core::context::Context;
    use solar_core::protocol::RequestId;

    /// A context over the real registry, with a generous budget.
    pub(crate) fn context(method: &str) -> Context {
        context_with_budget(method, Duration::from_mins(1))
    }

    /// A context over the real registry, with the budget a test wants.
    pub(crate) fn context_with_budget(method: &str, budget: Duration) -> Context {
        let registry = super::registry().expect("the registry of this build must be valid");
        Context::new(
            Some(RequestId::Number(1.into())),
            method,
            budget,
            Arc::clone(registry),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The APIs this stage promised. Later stages add to this list and never edit it:
    /// the check is that each of these is registered, not that nothing else is.
    const THE_APIS_OF_THIS_STAGE: [&str; 5] = [
        "solar.describe",
        "solar.manifest",
        "solar.ping",
        "solar.version",
        "system.info",
    ];

    #[test]
    fn the_registry_of_this_build_satisfies_the_template() {
        match registry() {
            Ok(registry) => assert!(!registry.is_empty()),
            Err(problems) => {
                let listed: Vec<String> = problems.iter().map(ToString::to_string).collect();
                panic!(
                    "the registry does not build:
  {}",
                    listed.join(
                        "
  "
                    )
                );
            }
        }
    }

    #[test]
    fn every_api_this_stage_promised_is_registered() {
        let registry = registry().expect("a valid registry");
        let names: Vec<&str> = registry.names().collect();
        for promised in THE_APIS_OF_THIS_STAGE {
            assert!(
                names.contains(&promised),
                "{promised} is not registered; names are {names:?}"
            );
        }
    }

    #[test]
    fn the_registry_is_sorted_so_that_listings_are_reproducible() {
        let registry = registry().expect("a valid registry");
        let names: Vec<&str> = registry.names().collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
    }

    #[test]
    fn a_dispatcher_answers_a_real_call_end_to_end() {
        let response = dispatcher().handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"solar.ping","params":{"message":"hi"}}"#,
        );
        assert!(response.is_success());
        let line = response.to_line();
        assert!(line.contains(r#""echo":"hi""#), "{line}");
        assert!(line.contains(r#""pong":true"#), "{line}");
    }
}
