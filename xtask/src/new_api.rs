//! `cargo xtask new-api <name>`: the whole procedure for adding an API, mechanised.
//!
//! The point is not to save typing. It is that the file, the struct name, the module line
//! and the registration line are derived from the method name by a rule, so that an API
//! added by a person and one added by an agent come out identical, and neither of them has
//! to remember the four places that would otherwise have to agree.

use std::path::{Path, PathBuf};

use solar_core::text::is_valid_method_name;

/// Writes the new API and registers it.
///
/// # Errors
///
/// Returns a sentence to print when the name is not a valid method name, when an API of
/// that name already exists, or when a file could not be written.
pub(crate) fn run(root: &Path, name: &str) -> Result<(), String> {
    if !is_valid_method_name(name) {
        return Err(format!(
            "{name} is not a valid method name. A name is namespace.verb_noun, lower case, \
             for example build.run_target."
        ));
    }

    let module = name.replace('.', "_");
    let structure = pascal_case(name);
    let file: PathBuf = root
        .join("crates/solar-apis/src")
        .join(format!("{module}.rs"));
    let lib = root.join("crates/solar-apis/src/lib.rs");

    if file.exists() {
        return Err(format!("{} already exists.", file.display()));
    }

    let source = std::fs::read_to_string(&lib)
        .map_err(|failure| format!("{} could not be read: {failure}", lib.display()))?;
    if source.contains(&format!("{module}::{structure}")) {
        return Err(format!(
            "{name} is already registered in {}.",
            lib.display()
        ));
    }

    let tests: PathBuf = root
        .join("crates/solar-apis/tests")
        .join(format!("{module}.rs"));
    if tests.exists() {
        return Err(format!("{} already exists.", tests.display()));
    }

    let registered = register(&source, &module, &structure)?;
    std::fs::write(&file, template(name, &structure).as_bytes())
        .map_err(|failure| format!("{} could not be written: {failure}", file.display()))?;
    std::fs::write(&tests, test_template(name, &module, &structure).as_bytes())
        .map_err(|failure| format!("{} could not be written: {failure}", tests.display()))?;
    std::fs::write(&lib, registered.as_bytes())
        .map_err(|failure| format!("{} could not be written: {failure}", lib.display()))?;

    println!("new-api: wrote {}", file.display());
    println!("new-api: wrote {}", tests.display());
    println!("new-api: registered {name} in {}", lib.display());
    println!();
    println!("Next, in this order:");
    println!("  1. fill in the parameters, the output, the description and the examples");
    println!("  2. write the tests of {name} in {}", tests.display());
    println!("  3. cargo nextest run -p solar-apis   the contract tests check the template");
    println!("  4. cargo xtask manifest              the manifest is generated, never written");
    Ok(())
}

/// `solar.ping` becomes `SolarPing`, `build.run_target` becomes `BuildRunTarget`.
fn pascal_case(name: &str) -> String {
    name.split(['.', '_'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut characters = word.chars();
            match characters.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + characters.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// Adds the module line and the registration line, both in alphabetical order.
fn register(source: &str, module: &str, structure: &str) -> Result<String, String> {
    let module_line = format!("pub mod {module};\n");
    let register_line = format!("        .register::<{module}::{structure}>()\n");

    let with_module = insert_sorted(source, &module_line, "pub mod ")
        .ok_or_else(|| "lib.rs has no block of `pub mod` lines to extend".to_owned())?;
    insert_sorted(&with_module, &register_line, "        .register::<")
        .ok_or_else(|| "lib.rs has no block of `.register::<...>()` lines to extend".to_owned())
}

/// Puts one line into the sorted block of lines that start with `prefix`.
fn insert_sorted(source: &str, line: &str, prefix: &str) -> Option<String> {
    let mut lines: Vec<String> = source.lines().map(|line| format!("{line}\n")).collect();
    let first = lines
        .iter()
        .position(|existing| existing.starts_with(prefix))?;
    let after = lines
        .iter()
        .rposition(|existing| existing.starts_with(prefix))
        .map_or(first + 1, |last| last + 1);

    let at = (first..after).find(|index| lines.get(*index).is_some_and(|l| l.as_str() > line));
    lines.insert(at.unwrap_or(after), line.to_owned());

    let mut joined: String = lines.concat();
    if !source.ends_with('\n') {
        joined.pop();
    }
    Some(joined)
}

/// The file a new API starts life as.
///
/// It compiles, it passes the contract tests, and every line of it is meant to be
/// replaced. Starting from something that is already correct is what stops a new API from
/// drifting away from the template.
fn template(name: &str, structure: &str) -> String {
    TEMPLATE
        .replace("__NAME__", name)
        .replace("__STRUCT__", structure)
}

const TEMPLATE: &str = r#"//! `__NAME__`: one line saying what this API is for.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use solar_core::api::{Api, ApiSpec, DEFAULT_MAX_OUTPUT_BYTES, Example, SideEffect, Stability};
use solar_core::context::Context;
use solar_core::error::SolarError;

/// The parameters of `__NAME__`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Replace this with a real parameter. An API that takes none keeps an empty struct,
    /// which still refuses any member a caller sends by mistake.
    #[serde(default)]
    pub placeholder: Option<String>,
}

/// The output of `__NAME__`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Output {
    /// Replace this with what the API really returns.
    pub done: bool,
}

/// One line about the API, which is what `cargo doc` shows.
#[derive(Debug)]
pub struct __STRUCT__;

impl Api for __STRUCT__ {
    const NAME: &'static str = "__NAME__";
    const VERSION: &'static str = "1.0.0";
    type Params = Params;
    type Output = Output;

    fn spec() -> ApiSpec {
        ApiSpec {
            summary: "Says in one line what this API does, without a full stop",
            description: "\
Replace this paragraph. Say what the API is for, what it guarantees, what it refuses to \
do, and anything a caller would otherwise have to find out by experiment. This text is \
what an agent reads before deciding to call, so it is worth more than the code it \
describes.",
            errors: Vec::new(),
            side_effects: vec![SideEffect::None],
            idempotent: true,
            stability: Stability::Experimental,
            since: "0.1.0",
            timeout_ms: 1_000,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            examples: vec![Example::exact(
                "plain",
                "The simplest call there is",
                json!({}),
                json!({"done": true}),
            )],
        }
    }

    fn call(_ctx: &Context, _params: Params) -> Result<Output, SolarError> {
        Ok(Output { done: true })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::context;

    #[test]
    fn it_answers() {
        let output =
            __STRUCT__::call(&context("__NAME__"), Params { placeholder: None }).unwrap();
        assert!(output.done);
    }
}
"#;

/// The test file a new API starts life with.
///
/// The examples of an API are already tests, replayed by the contract suite. This file is
/// for what an example cannot say: the failures, the edge of each parameter, and whatever
/// the API is really for. It starts with one test that passes, so the suite is green
/// before the work begins and red only for a reason.
fn test_template(name: &str, module: &str, structure: &str) -> String {
    TEST_TEMPLATE
        .replace("__NAME__", name)
        .replace("__MODULE__", module)
        .replace("__STRUCT__", structure)
}

const TEST_TEMPLATE: &str = r#"//! The tests of `__NAME__`.
//!
//! The examples in the specification are already replayed by `tests/contract.rs`. What
//! belongs here is everything an example cannot say: every failure the API declares, the
//! edge of every parameter, and the behaviour the API exists for.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test reports failure by panicking; integration tests are their own crate"
)]

use serde_json::{Value, json};
use solar_core::api::Api as _;

/// The response to one call of this API, as JSON, through the real dispatcher.
fn call(params: &Value) -> Value {
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": solar_apis::__MODULE__::__STRUCT__::NAME,
        "params": params,
    });
    let answered = solar_apis::dispatcher().handle_line(&request.to_string()).to_line();
    serde_json::from_str(&answered).expect("a response is JSON")
}

#[test]
fn it_answers() {
    let answered = call(&json!({}));
    assert!(answered.get("result").is_some(), "{answered}");
}

// Write the rest here:
//
//   - one test per failure the specification declares in `errors`
//   - one test per parameter, at its edge: absent, empty, the largest value allowed
//   - the tests that say what this API is for
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_becomes_a_module_and_a_struct_by_one_rule() {
        assert_eq!(pascal_case("solar.ping"), "SolarPing");
        assert_eq!(pascal_case("system.info"), "SystemInfo");
        assert_eq!(pascal_case("build.run_target"), "BuildRunTarget");
        assert_eq!(pascal_case("a.b.c"), "ABC");
    }

    #[test]
    fn a_line_lands_in_alphabetical_order_inside_its_block() {
        let source = "pub mod alpha;\npub mod gamma;\n\nfn other() {}\n";
        let with_beta = insert_sorted(source, "pub mod beta;\n", "pub mod ").unwrap();
        assert_eq!(
            with_beta,
            "pub mod alpha;\npub mod beta;\npub mod gamma;\n\nfn other() {}\n"
        );

        let appended = insert_sorted(source, "pub mod zeta;\n", "pub mod ").unwrap();
        assert!(appended.starts_with("pub mod alpha;\npub mod gamma;\npub mod zeta;\n"));
    }

    #[test]
    fn a_block_that_is_not_there_is_reported_rather_than_guessed() {
        assert!(insert_sorted("fn main() {}\n", "pub mod x;\n", "pub mod ").is_none());
    }

    #[test]
    fn the_test_skeleton_carries_the_name_the_module_and_the_struct() {
        let rendered = test_template("build.run_target", "build_run_target", "BuildRunTarget");
        assert!(rendered.contains("solar_apis::build_run_target::BuildRunTarget::NAME"));
        assert!(rendered.contains("The tests of `build.run_target`"));
        assert!(!rendered.contains("__NAME__"));
        assert!(!rendered.contains("__MODULE__"));
        assert!(!rendered.contains("__STRUCT__"));
    }

    #[test]
    fn the_template_carries_the_name_and_the_struct_everywhere() {
        let rendered = template("build.run_target", "BuildRunTarget");
        assert!(rendered.contains("const NAME: &'static str = \"build.run_target\";"));
        assert!(rendered.contains("impl Api for BuildRunTarget {"));
        assert!(!rendered.contains("__NAME__"));
        assert!(!rendered.contains("__STRUCT__"));
    }
}
