//! The template every API follows, and the specification every API publishes.
//!
//! There is one trait, one specification type, and no way to write an API that does not
//! declare what it does. Adding an API means writing one file in this shape and adding one
//! line to the registry; the manifest and the tests follow from the declaration.

use schemars::JsonSchema;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::context::Context;
use crate::error::SolarError;
use crate::reason::Reason;
use crate::status::Status;

/// What an API touches outside of its own arguments.
///
/// `None` is exclusive: an API that declares it must declare nothing else, and the registry
/// refuses a specification that claims both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SideEffect {
    /// Reads nothing, writes nothing, starts nothing. A pure function of its parameters
    /// and of the state SOLAR already holds in memory.
    None,
    /// Reads from the file system.
    ReadsFilesystem,
    /// Writes to the file system.
    WritesFilesystem,
    /// Starts an external process.
    SpawnsProcess,
    /// Talks to the network.
    Network,
}

/// How much a caller may rely on an API staying as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Stability {
    /// The shape may still change. Everything is experimental while SOLAR is below 1.0.0.
    Experimental,
    /// The shape only changes with a major version of the API.
    Stable,
    /// Still answers, and will be removed. The description says what to use instead.
    Deprecated,
}

/// How the response of an example is compared with what really comes back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MatchMode {
    /// Deep equality, member for member.
    Exact,
    /// Every member the example names must match; the rest is ignored.
    Subset,
}

/// The token that matches any value at that position inside an example.
pub const ANY: &str = "$any";

/// One `{status, reason}` pair an API declares it may return.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ErrorSpec {
    /// The kind of failure.
    pub status: Status,
    /// Which failure exactly.
    pub reason: Reason,
}

impl ErrorSpec {
    /// Declares a failure by its reason; the status follows from the reason.
    #[must_use]
    pub const fn of(reason: Reason) -> Self {
        Self {
            status: reason.status(),
            reason,
        }
    }
}

/// A request and the response it produces, which the test suite replays.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Example {
    /// A short lower case name, unique within the API.
    pub name: &'static str,
    /// One line saying what the example shows.
    pub description: &'static str,
    /// The `params` object of the request.
    pub params: Value,
    /// The expected `result.data`, where [`ANY`] stands for any value.
    pub response: Value,
    /// How `response` is compared with reality.
    #[serde(rename = "match")]
    pub match_mode: MatchMode,
}

impl Example {
    /// An example compared member by member, ignoring what it does not name.
    #[must_use]
    pub fn subset(
        name: &'static str,
        description: &'static str,
        params: Value,
        response: Value,
    ) -> Self {
        Self {
            name,
            description,
            params,
            response,
            match_mode: MatchMode::Subset,
        }
    }

    /// An example compared by deep equality.
    #[must_use]
    pub fn exact(
        name: &'static str,
        description: &'static str,
        params: Value,
        response: Value,
    ) -> Self {
        Self {
            name,
            description,
            params,
            response,
            match_mode: MatchMode::Exact,
        }
    }
}

/// Everything an API says about itself.
///
/// The specification is the single source of the manifest, of the documentation and of the
/// contract tests. Nothing about an API is written down twice.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct ApiSpec {
    /// One line, no full stop, what the API does.
    pub summary: &'static str,
    /// Prose: what it is for, what it guarantees, what it does not do.
    pub description: &'static str,
    /// Every failure this API can return on top of the ones dispatch itself produces.
    pub errors: Vec<ErrorSpec>,
    /// What it touches outside its arguments.
    pub side_effects: Vec<SideEffect>,
    /// Whether calling twice has the same effect as calling once.
    pub idempotent: bool,
    /// How much a caller may rely on the shape.
    pub stability: Stability,
    /// The SOLAR version this API first appeared in.
    pub since: &'static str,
    /// The wall clock budget of one call, strictly positive, enforced by dispatch.
    pub timeout_ms: u64,
    /// The largest response this API may produce, in bytes of serialised `result.data`.
    ///
    /// Section 8.4 of the contract. [`DEFAULT_MAX_OUTPUT_BYTES`] is what an API declares
    /// unless it has a reason to declare less. Dispatch refuses a larger response with
    /// `RESOURCE_EXHAUSTED` / `OUTPUT_TOO_LARGE`, because a line nothing can buffer is
    /// not a response: large data travels by pagination or by reference.
    pub max_output_bytes: u64,
    /// At least one example, replayed by the test suite.
    pub examples: Vec<Example>,
}

/// The largest response an API may produce unless it declares a smaller one.
///
/// Eight mebibytes, half the request line limit of section 2 of the contract. It is a
/// ceiling, not a target: an API that approaches it is an API that should paginate.
pub const DEFAULT_MAX_OUTPUT_BYTES: u64 = 8 * 1024 * 1024;

/// One API: a name, a version, a parameter type, an output type and a function.
///
/// The associated types carry their own JSON Schema, which is what makes the manifest and
/// the parameter validation impossible to forget. `Params` is always declared with
/// `#[serde(deny_unknown_fields)]`, so a misspelled parameter is an error and never a
/// silent no-op.
///
/// # Example
///
/// ```
/// use schemars::JsonSchema;
/// use serde::{Deserialize, Serialize};
/// use solar_core::api::{Api, ApiSpec, Example, SideEffect, Stability};
/// use solar_core::context::Context;
/// use solar_core::error::SolarError;
/// use serde_json::json;
///
/// #[derive(Debug, Deserialize, JsonSchema)]
/// #[serde(deny_unknown_fields)]
/// pub struct Params {
///     /// How loudly to shout.
///     #[serde(default)]
///     pub times: u8,
/// }
///
/// #[derive(Debug, Serialize, JsonSchema)]
/// pub struct Output {
///     /// The shout.
///     pub shout: String,
/// }
///
/// pub struct Shout;
///
/// impl Api for Shout {
///     const NAME: &'static str = "demo.shout";
///     const VERSION: &'static str = "1.0.0";
///     type Params = Params;
///     type Output = Output;
///
///     fn spec() -> ApiSpec {
///         ApiSpec {
///             summary: "Shouts a number of times",
///             description: "Exists only to show the shape of an API.",
///             errors: Vec::new(),
///             side_effects: vec![SideEffect::None],
///             idempotent: true,
///             stability: Stability::Experimental,
///             since: "0.1.0",
///             timeout_ms: 1_000,
///             max_output_bytes: solar_core::api::DEFAULT_MAX_OUTPUT_BYTES,
///             examples: vec![Example::exact(
///                 "twice",
///                 "Shouts twice",
///                 json!({"times": 2}),
///                 json!({"shout": "!!"}),
///             )],
///         }
///     }
///
///     fn call(_ctx: &Context, params: Params) -> Result<Output, SolarError> {
///         Ok(Output { shout: "!".repeat(usize::from(params.times)) })
///     }
/// }
/// ```
pub trait Api: 'static {
    /// The method name, `namespace.verb_noun`.
    const NAME: &'static str;
    /// The semantic version of this API, independent of the SOLAR version.
    const VERSION: &'static str;

    /// The parameters, declared with `#[serde(deny_unknown_fields)]`.
    type Params: DeserializeOwned + JsonSchema + Send + 'static;
    /// The output, which becomes `result.data`.
    type Output: Serialize + JsonSchema + Send + 'static;

    /// Everything this API says about itself.
    fn spec() -> ApiSpec;

    /// Runs one call.
    ///
    /// # Errors
    ///
    /// Returns any failure the specification declares in `errors`. A failure that is not
    /// declared is a bug that the contract tests are meant to catch.
    fn call(ctx: &Context, params: Self::Params) -> Result<Self::Output, SolarError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_output_size_is_the_number_the_contract_states() {
        // Written as a plain number, because the point is that eight mebibytes is what
        // section 8.4 says, not that one arithmetic expression equals itself.
        assert_eq!(DEFAULT_MAX_OUTPUT_BYTES, 8_388_608);
        assert_eq!(
            DEFAULT_MAX_OUTPUT_BYTES * 2,
            crate::protocol::MAX_REQUEST_BYTES as u64,
            "the default is half the request line limit, which is what makes it a              ceiling a response can always be written under"
        );
    }
}
