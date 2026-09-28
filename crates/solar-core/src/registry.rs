//! The list of APIs, built once, checked once, then read on every call.
//!
//! Registration is explicit: `solar-apis` names every API in one place, and nothing is
//! discovered by magic. What is not explicit is the checking, which happens here and covers
//! every rule of section 8 of the contract, so that an API that breaks the template cannot
//! reach a caller.

use std::any::Any;
use std::sync::OnceLock;

use schemars::{JsonSchema, schema_for};
use serde_json::Value;

use crate::api::{Api, ApiSpec, SideEffect, Stability};
use crate::context::Context;
use crate::error::{ErrorDetail, SolarError};
use crate::meta::SOLAR_VERSION;
use crate::params;
use crate::reason::Reason;
use crate::status::Status;
use crate::text::{is_valid_method_name, suggestions};

/// Reads a parameter object into the boxed parameter type of one API.
type ValidateFn =
    fn(&str, &Value, &Value, Option<&Value>) -> Result<Box<dyn Any + Send>, SolarError>;

/// Runs one API on parameters that were already read.
type InvokeFn = fn(&Context, Box<dyn Any + Send>) -> Result<Value, SolarError>;

/// A rule of the template that one API broke.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryProblem {
    /// The API the problem is about.
    pub api: String,
    /// The rule that was broken, in the words of the contract.
    pub rule: &'static str,
    /// What exactly was wrong.
    pub detail: String,
}

impl std::fmt::Display for RegistryProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {} ({})", self.api, self.detail, self.rule)
    }
}

/// One registered API, with its specification, its schemas and its two entry points.
///
/// Schemas are generated on first use rather than at registration, because most calls never
/// need them and the ones that do are not on the hot path.
pub struct ApiEntry {
    name: &'static str,
    version: &'static str,
    spec: ApiSpec,
    first_example: Option<Value>,
    params_schema: OnceLock<Value>,
    output_schema: OnceLock<Value>,
    build_params_schema: fn() -> Value,
    build_output_schema: fn() -> Value,
    validate: ValidateFn,
    invoke: InvokeFn,
}

impl std::fmt::Debug for ApiEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiEntry")
            .field("name", &self.name)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

impl ApiEntry {
    /// The method name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// The semantic version of this API.
    #[must_use]
    pub const fn version(&self) -> &'static str {
        self.version
    }

    /// Everything the API says about itself.
    #[must_use]
    pub const fn spec(&self) -> &ApiSpec {
        &self.spec
    }

    /// The JSON Schema of the parameters, generated on first use.
    #[must_use]
    pub fn params_schema(&self) -> &Value {
        self.params_schema.get_or_init(self.build_params_schema)
    }

    /// The JSON Schema of the output, generated on first use.
    #[must_use]
    pub fn output_schema(&self) -> &Value {
        self.output_schema.get_or_init(self.build_output_schema)
    }

    /// Reads a parameter object into the type this API declared.
    ///
    /// # Errors
    ///
    /// Returns `INVALID_ARGUMENT` with the reason that fits what was wrong.
    pub fn validate(&self, params: &Value) -> Result<Box<dyn Any + Send>, SolarError> {
        (self.validate)(
            self.name,
            params,
            self.params_schema(),
            self.first_example.as_ref(),
        )
    }

    /// Runs the API on parameters that [`ApiEntry::validate`] already accepted.
    ///
    /// # Errors
    ///
    /// Returns whatever the API returns, or `INTERNAL` when its output cannot be serialised.
    pub fn invoke(&self, ctx: &Context, params: Box<dyn Any + Send>) -> Result<Value, SolarError> {
        (self.invoke)(ctx, params)
    }
}

/// Reads parameters for `A`, then boxes them so that dispatch can carry them untyped.
fn validate_for<A: Api>(
    method: &str,
    params: &Value,
    schema: &Value,
    example: Option<&Value>,
) -> Result<Box<dyn Any + Send>, SolarError> {
    let typed: A::Params = params::deserialize(method, params, schema, example)?;
    Ok(Box::new(typed))
}

/// Unboxes the parameters of `A`, runs it, and turns its output into JSON.
fn invoke_for<A: Api>(ctx: &Context, params: Box<dyn Any + Send>) -> Result<Value, SolarError> {
    let typed = params.downcast::<A::Params>().map_err(|_| {
        SolarError::new(
            Reason::InvariantBroken,
            format!(
                "The parameters handed to {} were of the wrong type.",
                A::NAME
            ),
        )
        .with_detail(
            ErrorDetail::new(Status::Internal)
                .field("params")
                .expected(std::any::type_name::<A::Params>())
                .hint("Report this: a registry entry was built from mismatched halves."),
        )
    })?;
    let output = A::call(ctx, *typed)?;
    serde_json::to_value(output).map_err(|failure| {
        SolarError::new(
            Reason::SerializationFailed,
            format!(
                "The output of {} could not be turned into JSON: {failure}.",
                A::NAME
            ),
        )
        .with_detail(
            ErrorDetail::new(Status::Internal)
                .expected("a serialisable output")
                .hint("Report this with the exact request that caused it."),
        )
    })
}

/// Generates the JSON Schema of a type, as JSON.
fn schema_of<T: JsonSchema>() -> Value {
    serde_json::to_value(schema_for!(T)).unwrap_or(Value::Null)
}

/// Collects APIs and checks them against the template.
#[derive(Debug, Default)]
pub struct RegistryBuilder {
    entries: Vec<ApiEntry>,
}

impl RegistryBuilder {
    /// An empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Adds one API. This is the one line `cargo xtask new-api` writes for you.
    #[must_use]
    pub fn register<A: Api>(mut self) -> Self {
        let spec = A::spec();
        // The example a hint shows is the first one that actually passes something, so
        // that "a call that works" is worth reading even when the first example is bare.
        let first_example = spec
            .examples
            .iter()
            .find(|example| {
                example
                    .params
                    .as_object()
                    .is_some_and(|members| !members.is_empty())
            })
            .or_else(|| spec.examples.first())
            .map(|example| example.params.clone());
        self.entries.push(ApiEntry {
            name: A::NAME,
            version: A::VERSION,
            spec,
            first_example,
            params_schema: OnceLock::new(),
            output_schema: OnceLock::new(),
            build_params_schema: schema_of::<A::Params>,
            build_output_schema: schema_of::<A::Output>,
            validate: validate_for::<A>,
            invoke: invoke_for::<A>,
        });
        self
    }

    /// Checks every entry against the template and sorts them by name.
    ///
    /// # Errors
    ///
    /// Returns every problem it found, not just the first, so that a contributor sees the
    /// whole list at once.
    pub fn build(mut self) -> Result<Registry, Vec<RegistryProblem>> {
        let mut problems = Vec::new();

        for entry in &self.entries {
            check_entry(entry, &mut problems);
        }

        self.entries.sort_by_key(|entry| entry.name);
        for pair in self.entries.windows(2) {
            if let [left, right] = pair
                && left.name == right.name
            {
                problems.push(RegistryProblem {
                    api: left.name.to_owned(),
                    rule: "a name is unique across the whole registry",
                    detail: format!("{} is registered twice", left.name),
                });
            }
        }

        if problems.is_empty() {
            Ok(Registry {
                entries: self.entries,
            })
        } else {
            Err(problems)
        }
    }
}

/// Checks one entry against every rule of section 8 of the contract.
#[allow(
    clippy::too_many_lines,
    reason = "one paragraph per rule of the contract; splitting it would scatter the list"
)]
fn check_entry(entry: &ApiEntry, problems: &mut Vec<RegistryProblem>) {
    let mut note = |rule: &'static str, detail: String| {
        problems.push(RegistryProblem {
            api: entry.name.to_owned(),
            rule,
            detail,
        });
    };
    let spec = &entry.spec;

    if !is_valid_method_name(entry.name) {
        note(
            "a name matches ^[a-z]+(\\.[a-z]+(_[a-z]+)*)+$",
            format!("{} is not a valid method name", entry.name),
        );
    }
    if !is_semver(entry.version) {
        note(
            "version is a semantic version",
            format!("version {} is not x.y.z", entry.version),
        );
    }
    if !is_semver(spec.since) {
        note(
            "since is a semantic version",
            format!("since {} is not x.y.z", spec.since),
        );
    }
    if spec.summary.trim().is_empty() {
        note("every API has a summary", "the summary is empty".to_owned());
    }
    if spec.summary.ends_with('.') {
        note(
            "a summary is one line without a full stop",
            format!("summary {:?} ends in a full stop", spec.summary),
        );
    }
    if spec.description.trim().is_empty() {
        note(
            "every API has a description",
            "the description is empty".to_owned(),
        );
    }
    if spec.timeout_ms == 0 {
        note(
            "timeout_ms is strictly positive",
            "timeout_ms is zero".to_owned(),
        );
    }
    if spec.side_effects.is_empty() {
        note(
            "side_effects says what the API touches",
            "side_effects is empty; declare none explicitly".to_owned(),
        );
    }
    if spec.side_effects.contains(&SideEffect::None) && spec.side_effects.len() > 1 {
        note(
            "none is exclusive among side effects",
            format!(
                "side_effects claims none and {} other values",
                spec.side_effects.len() - 1
            ),
        );
    }
    let mut effects = spec.side_effects.clone();
    effects.sort_unstable();
    let unique = effects.len();
    effects.dedup();
    if effects.len() != unique {
        note(
            "side_effects has no repeats",
            "a side effect is declared twice".to_owned(),
        );
    }
    if spec.examples.is_empty() {
        note(
            "every API carries at least one example",
            "there are no examples".to_owned(),
        );
    }
    let mut names: Vec<&str> = spec.examples.iter().map(|example| example.name).collect();
    names.sort_unstable();
    let unique = names.len();
    names.dedup();
    if names.len() != unique {
        note(
            "example names are unique within an API",
            "two examples share a name".to_owned(),
        );
    }
    for example in &spec.examples {
        if !example.params.is_object() {
            note(
                "the params of an example are an object",
                format!("example {} has params that are not an object", example.name),
            );
        }
        if example.description.trim().is_empty() {
            note(
                "every example says what it shows",
                format!("example {} has no description", example.name),
            );
        }
    }
    if solar_is_before_1_0() && spec.stability != Stability::Experimental {
        note(
            "every API is experimental while SOLAR is below 1.0.0",
            format!("stability is {:?} in SOLAR {SOLAR_VERSION}", spec.stability),
        );
    }
}

/// Whether this SOLAR build is still below its first stable release.
fn solar_is_before_1_0() -> bool {
    is_before_1_0(SOLAR_VERSION)
}

/// The same question about any version, so that both answers can be tested.
///
/// The build is below 1.0.0 today and will not always be, and a rule nothing can check
/// until the day it changes is a rule that changes wrongly on that day.
fn is_before_1_0(version: &str) -> bool {
    version.split('.').next().is_some_and(|major| major == "0")
}

/// Whether a string is `major.minor.patch`, the only shape SOLAR versions take.
#[must_use]
pub fn is_semver(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

/// Every API this build answers to, sorted by name.
#[derive(Debug)]
pub struct Registry {
    entries: Vec<ApiEntry>,
}

impl Registry {
    /// The entry for a name, or `None` when nothing answers to it.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ApiEntry> {
        self.entries
            .binary_search_by(|entry| entry.name.cmp(name))
            .ok()
            .and_then(|index| self.entries.get(index))
    }

    /// Every entry, sorted by name.
    #[must_use]
    pub fn entries(&self) -> &[ApiEntry] {
        &self.entries
    }

    /// Every registered name, sorted.
    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.entries.iter().map(|entry| entry.name)
    }

    /// How many APIs are registered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The registered names closest to `name`, nearest first.
    #[must_use]
    pub fn suggestions(&self, name: &str) -> Vec<(&'static str, usize)> {
        suggestions(name, self.names())
    }

    /// The `NOT_FOUND` error for a name nothing answers to, with suggestions.
    ///
    /// `reason` is `METHOD_NOT_FOUND` when the call itself does not exist, and
    /// `API_NOT_FOUND` when an existing call was asked about an API that does not.
    #[must_use]
    pub fn not_found(&self, name: &str, reason: Reason, field: &str) -> SolarError {
        let close = self.suggestions(name);
        let status = reason.status();
        let error = SolarError::new(
            reason,
            match reason {
                Reason::MethodNotFound => format!("Method not found: {name}."),
                _ => format!("No API is registered under the name {name}."),
            },
        );

        if close.is_empty() {
            return error.with_detail(
                ErrorDetail::new(status)
                    .field(field.to_owned())
                    .expected("a registered method name")
                    .received(Value::String(name.to_owned()))
                    .hint(format!(
                        "Nothing close to {name} is registered. Call solar.manifest for the \
                         {} names this build answers to.",
                        self.len()
                    )),
            );
        }

        error.with_details(close.into_iter().map(|(candidate, distance)| {
            ErrorDetail::new(status)
                .field(field.to_owned())
                .expected(candidate)
                .received(Value::String(name.to_owned()))
                .hint(format!(
                    "Did you mean \"{candidate}\"? The edit distance is {distance}."
                ))
        }))
    }

    /// Turns the problems a failed build reported into the error a caller receives.
    ///
    /// A broken registry is a bug, and every call fails with the same `INTERNAL` error
    /// naming every rule that was broken. SOLAR still answers.
    #[must_use]
    pub fn build_failure(problems: &[RegistryProblem]) -> SolarError {
        let error = SolarError::new(
            Reason::InvariantBroken,
            format!(
                "The API registry does not satisfy the template: {} {} broken.",
                problems.len(),
                if problems.len() == 1 {
                    "rule is"
                } else {
                    "rules are"
                }
            ),
        );
        error.with_details(problems.iter().map(|problem| {
            ErrorDetail::new(Status::Internal)
                .field(problem.api.clone())
                .expected(problem.rule)
                .received(Value::String(problem.detail.clone()))
                .hint("Read docs/ADDING_AN_API.md; the contract tests report the same list.")
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{ApiSpec, DEFAULT_MAX_OUTPUT_BYTES, Example};
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    #[derive(Debug, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(super) struct EchoParams {
        /// What to echo.
        pub(super) text: String,
    }

    #[derive(Debug, Serialize, JsonSchema)]
    pub(super) struct EchoOutput {
        /// What was echoed.
        pub(super) text: String,
    }

    pub(super) struct Echo;

    fn echo_spec() -> ApiSpec {
        ApiSpec {
            summary: "Echoes a string",
            description: "A test API.",
            errors: Vec::new(),
            side_effects: vec![SideEffect::None],
            idempotent: true,
            stability: Stability::Experimental,
            since: "0.1.0",
            timeout_ms: 1_000,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            examples: vec![Example::exact(
                "plain",
                "Echoes hi",
                json!({"text": "hi"}),
                json!({"text": "hi"}),
            )],
        }
    }

    impl Api for Echo {
        const NAME: &'static str = "test.echo";
        const VERSION: &'static str = "1.0.0";
        type Params = EchoParams;
        type Output = EchoOutput;
        fn spec() -> ApiSpec {
            echo_spec()
        }
        fn call(_ctx: &Context, params: EchoParams) -> Result<EchoOutput, SolarError> {
            Ok(EchoOutput { text: params.text })
        }
    }

    pub(super) struct Twin;
    impl Api for Twin {
        const NAME: &'static str = "test.echo";
        const VERSION: &'static str = "1.0.0";
        type Params = EchoParams;
        type Output = EchoOutput;
        fn spec() -> ApiSpec {
            echo_spec()
        }
        fn call(_ctx: &Context, params: EchoParams) -> Result<EchoOutput, SolarError> {
            Ok(EchoOutput { text: params.text })
        }
    }

    pub(super) struct Broken;
    impl Api for Broken {
        const NAME: &'static str = "Broken";
        const VERSION: &'static str = "one";
        type Params = EchoParams;
        type Output = EchoOutput;
        fn spec() -> ApiSpec {
            ApiSpec {
                summary: "",
                description: "",
                errors: Vec::new(),
                side_effects: vec![SideEffect::None, SideEffect::Network],
                idempotent: true,
                stability: Stability::Stable,
                since: "zero",
                timeout_ms: 0,
                max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
                examples: Vec::new(),
            }
        }
        fn call(_ctx: &Context, params: EchoParams) -> Result<EchoOutput, SolarError> {
            Ok(EchoOutput { text: params.text })
        }
    }

    #[test]
    fn what_counts_as_before_the_first_stable_release() {
        // Everything is experimental until SOLAR 1.0.0, record 7, so this decides whether
        // a stable API is allowed. It answers true today and will not always, and a rule
        // nothing checks until the day it changes is a rule that changes wrongly.
        assert!(is_before_1_0("0.1.0"));
        assert!(is_before_1_0("0.3.0"));
        assert!(is_before_1_0("0.99.0"));

        assert!(!is_before_1_0("1.0.0"));
        assert!(!is_before_1_0("2.0.0"));
        assert!(!is_before_1_0("10.0.0"), "ten is not zero");

        assert!(
            is_before_1_0(SOLAR_VERSION),
            "and this build really is below it"
        );
    }

    #[test]
    fn a_registry_of_one_api_builds_and_answers() {
        let registry = RegistryBuilder::new().register::<Echo>().build().unwrap();
        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());
        let entry = registry.get("test.echo").unwrap();
        assert_eq!(entry.name(), "test.echo");
        assert_eq!(entry.version(), "1.0.0");
        assert_eq!(entry.spec().timeout_ms, 1_000);
        assert!(registry.get("test.nothing").is_none());
    }

    #[test]
    fn schemas_are_generated_on_first_use_and_then_cached() {
        let registry = RegistryBuilder::new().register::<Echo>().build().unwrap();
        let entry = registry.get("test.echo").unwrap();
        let first = entry.params_schema().clone();
        let second = entry.params_schema().clone();
        assert_eq!(first, second);
        assert_eq!(first["additionalProperties"], json!(false));
        assert_eq!(
            entry.output_schema()["properties"]["text"]["type"],
            "string"
        );
    }

    #[test]
    fn a_duplicate_name_is_refused() {
        let problems = RegistryBuilder::new()
            .register::<Echo>()
            .register::<Twin>()
            .build()
            .unwrap_err();
        assert!(
            problems
                .iter()
                .any(|problem| problem.detail.contains("registered twice"))
        );
    }

    #[test]
    fn every_broken_rule_is_reported_at_once() {
        let problems = RegistryBuilder::new()
            .register::<Broken>()
            .build()
            .unwrap_err();
        let rules: Vec<&str> = problems.iter().map(|problem| problem.rule).collect();
        for expected in [
            "a name matches ^[a-z]+(\\.[a-z]+(_[a-z]+)*)+$",
            "version is a semantic version",
            "since is a semantic version",
            "every API has a summary",
            "every API has a description",
            "timeout_ms is strictly positive",
            "none is exclusive among side effects",
            "every API carries at least one example",
            "every API is experimental while SOLAR is below 1.0.0",
        ] {
            assert!(
                rules.contains(&expected),
                "{expected} was not reported, got {rules:?}"
            );
        }
        let error = Registry::build_failure(&problems);
        assert_eq!(error.reason(), Reason::InvariantBroken);
        assert_eq!(error.details().len(), problems.len());
    }

    #[test]
    fn an_unknown_method_suggests_the_closest_names() {
        let registry = RegistryBuilder::new().register::<Echo>().build().unwrap();
        let error = registry.not_found("test.eco", Reason::MethodNotFound, "method");
        assert_eq!(error.status(), Status::NotFound);
        assert_eq!(error.details()[0].expected.as_deref(), Some("test.echo"));
        assert!(
            error.details()[0]
                .hint
                .as_deref()
                .unwrap()
                .contains("Did you mean")
        );

        let hopeless = registry.not_found("nothing.like_it", Reason::ApiNotFound, "/api");
        assert_eq!(hopeless.details().len(), 1);
        assert!(
            hopeless.details()[0]
                .hint
                .as_deref()
                .unwrap()
                .contains("solar.manifest")
        );
        assert_eq!(hopeless.details()[0].field.as_deref(), Some("/api"));
    }

    #[test]
    fn a_registered_api_validates_and_runs() {
        let registry =
            std::sync::Arc::new(RegistryBuilder::new().register::<Echo>().build().unwrap());
        let entry = registry.get("test.echo").unwrap();
        let params = entry.validate(&json!({"text": "hi"})).unwrap();
        let ctx = Context::new(
            None,
            "test.echo",
            std::time::Duration::from_secs(1),
            registry.clone(),
        );
        assert_eq!(entry.invoke(&ctx, params).unwrap(), json!({"text": "hi"}));

        let refused = entry.validate(&json!({"nope": 1})).unwrap_err();
        assert_eq!(refused.reason(), Reason::UnknownField);
    }

    #[test]
    fn semantic_versions_are_recognised() {
        assert!(is_semver("0.1.0"));
        assert!(is_semver("10.20.30"));
        assert!(!is_semver("1.0"));
        assert!(!is_semver("1.0.0-rc.1"));
        assert!(!is_semver("v1.0.0"));
        assert!(!is_semver(""));
    }
}
