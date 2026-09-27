//! Laying out an answer for a person.
//!
//! Nothing here decides anything: every value printed came from a real response, and the
//! only job of this module is to put it on the screen in an order a person reads. The JSON
//! is always one `solar call` away when the layout hides something.

use std::io::Write;

use serde_json::Value;

/// The width of the name column of `solar list`.
const NAME_WIDTH: usize = 18;

/// `solar list`: one line per API, widest first.
pub(crate) fn list<W: Write>(out: &mut W, manifest: &Value) -> std::io::Result<()> {
    let apis = manifest
        .get("apis")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    writeln!(
        out,
        "SOLAR {} speaks {}, and answers to {} {}:",
        text(manifest, "solar_version"),
        text(manifest, "protocol"),
        apis.len(),
        if apis.len() == 1 { "API" } else { "APIs" }
    )?;
    writeln!(out)?;

    for api in &apis {
        writeln!(
            out,
            "  {:<NAME_WIDTH$}  {:<7}  {}",
            text(api, "name"),
            text(api, "version"),
            text(api, "summary")
        )?;
    }

    writeln!(out)?;
    writeln!(
        out,
        "  solar describe <method>   everything about one of them"
    )?;
    writeln!(
        out,
        "  solar call <method>       the whole envelope, as an agent sees it"
    )
}

/// `solar describe`: everything about one API, in the order a caller needs it.
pub(crate) fn describe<W: Write>(out: &mut W, api: &Value) -> std::io::Result<()> {
    writeln!(out, "{} {}", text(api, "name"), text(api, "version"))?;
    writeln!(out, "{}", text(api, "summary"))?;
    writeln!(out)?;

    for paragraph in text(api, "description").split("\n\n") {
        writeln!(out, "{paragraph}")?;
        writeln!(out)?;
    }

    let effects = api
        .get("side_effects")
        .and_then(Value::as_array)
        .map_or_else(
            || "unknown".to_owned(),
            |values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<&str>>()
                    .join(", ")
            },
        );
    writeln!(
        out,
        "stability {}   since {}   timeout {} ms   idempotent {}   touches {effects}",
        text(api, "stability"),
        text(api, "since"),
        api.get("timeout_ms")
            .map_or("?".to_owned(), ToString::to_string),
        api.get("idempotent")
            .map_or("?".to_owned(), ToString::to_string)
    )?;

    writeln!(out)?;
    writeln!(out, "parameters")?;
    parameters(out, api.get("params_schema").unwrap_or(&Value::Null))?;

    if let Some(errors) = api.get("errors").and_then(Value::as_array)
        && !errors.is_empty()
    {
        writeln!(out)?;
        writeln!(out, "may fail with")?;
        for failure in errors {
            writeln!(
                out,
                "  {} / {}",
                text(failure, "status"),
                text(failure, "reason")
            )?;
        }
    }

    if let Some(examples) = api.get("examples").and_then(Value::as_array) {
        writeln!(out)?;
        writeln!(out, "examples")?;
        for example in examples {
            writeln!(
                out,
                "  {}: {}",
                text(example, "name"),
                text(example, "description")
            )?;
            runnable_lines(out, text(api, "name"), example.get("params"))?;
        }
    }
    Ok(())
}

/// The lines that run one example, pastable in the shell each is labelled with.
///
/// No single inline form survives every shell: Windows PowerShell strips plain double
/// quotes, `cmd.exe` does not treat single quotes as quotes, and the form that satisfies
/// both of those breaks in PowerShell the moment the JSON holds a space. Each line below
/// was verified in its shell, and the two Windows lines use `-`, which reads the
/// parameters from standard input, where no shell rewrites them.
fn runnable_lines<W: Write>(
    out: &mut W,
    method: &str,
    params: Option<&Value>,
) -> std::io::Result<()> {
    let params = params.and_then(Value::as_object);
    if params.is_none_or(serde_json::Map::is_empty) {
        // Absent parameters are `{}` by contract, and a bare call works in every shell.
        return writeln!(out, "    solar call {method}");
    }
    let json = Value::Object(params.cloned().unwrap_or_default()).to_string();
    let bash = json.replace(APOSTROPHE, BASH_QUOTED_APOSTROPHE);
    writeln!(out, "    bash        solar call {method} '{bash}'")?;
    writeln!(out, "    powershell  '{json}' | solar call {method} -")?;
    writeln!(out, "    cmd         echo {json}| solar call {method} -")
}

/// A single quote, which ends a single quoted string in `bash`.
const APOSTROPHE: char = '\'';

/// How a single quote is written inside a single quoted `bash` string: close it, escape
/// one quote, open again.
const BASH_QUOTED_APOSTROPHE: &str = r"'\''";

/// The members of a parameter schema, one per line, required ones marked.
fn parameters<W: Write>(out: &mut W, schema: &Value) -> std::io::Result<()> {
    let properties = schema.get("properties").and_then(Value::as_object);
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    match properties {
        None => writeln!(out, "  none"),
        Some(members) if members.is_empty() => writeln!(out, "  none"),
        Some(members) => {
            for (name, member) in members {
                let kind = type_of(member);
                let mark = if required.contains(&name.as_str()) {
                    "required"
                } else {
                    "optional"
                };
                writeln!(
                    out,
                    "  {name:<12} {kind:<18} {mark}  {}",
                    text(member, "description")
                )?;
            }
            Ok(())
        }
    }
}

/// `solar version`: the three versions and the build, one per line.
///
/// On a terminal the mark is drawn beside the first four lines, which is the only place
/// in this binary where it appears. `docs/brand/README.md` says why.
pub(crate) fn version<W: Write>(
    out: &mut W,
    data: &Value,
    decorate: bool,
    colour: bool,
) -> std::io::Result<()> {
    if decorate {
        let beside = vec![
            "SOLAR".to_owned(),
            "The central API of the Constellation".to_owned(),
            "NIPS-CERN".to_owned(),
            text(data, "solar_version").to_owned(),
        ];
        write!(out, "{}", crate::banner::beside(&beside, colour))?;
        writeln!(out)?;
    }
    writeln!(out, "solar          {}", text(data, "solar_version"))?;
    writeln!(out, "protocol       {}", text(data, "protocol"))?;
    writeln!(
        out,
        "manifest       {}",
        text(data, "manifest_schema_version")
    )?;
    let build = data.get("build").cloned().unwrap_or(Value::Null);
    writeln!(out, "commit         {}", text(&build, "git_commit_short"))?;
    writeln!(out, "dirty          {}", text(&build, "git_dirty"))?;
    writeln!(out, "profile        {}", text(&build, "profile"))?;
    writeln!(out, "target         {}", text(&build, "target"))?;
    writeln!(out, "compiler       {}", text(&build, "rustc_version"))
}

/// The type a schema declares, as a caller would say it.
fn type_of(member: &Value) -> String {
    match member.get("type") {
        Some(Value::String(name)) => name.clone(),
        Some(Value::Array(names)) => names
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<&str>>()
            .join(" or "),
        _ => "value".to_owned(),
    }
}

/// A member of an object as text, or a dash when it is absent.
fn text<'a>(value: &'a Value, member: &str) -> &'a str {
    value.get(member).and_then(Value::as_str).unwrap_or("-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rendered(method: &str, params: &Value) -> String {
        let mut out = Vec::new();
        runnable_lines(&mut out, method, Some(params)).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn empty_parameters_give_one_line_that_works_in_every_shell() {
        let lines = rendered("solar.ping", &json!({}));
        assert_eq!(lines, "    solar call solar.ping\n");
    }

    #[test]
    fn parameters_give_one_pastable_line_per_shell() {
        let lines = rendered("solar.ping", &json!({"message": "hi"}));
        assert!(lines.contains(r#"bash        solar call solar.ping '{"message":"hi"}'"#));
        assert!(lines.contains(r#"powershell  '{"message":"hi"}' | solar call solar.ping -"#));
        assert!(lines.contains(r#"cmd         echo {"message":"hi"}| solar call solar.ping -"#));
    }

    #[test]
    fn an_apostrophe_in_the_parameters_survives_the_bash_quoting() {
        let lines = rendered("solar.ping", &json!({"message": "it's"}));
        let bash_line = lines.lines().find(|line| line.contains("bash")).unwrap();
        // Close the quote, escape one apostrophe, open again: the shell reassembles it's.
        assert!(
            bash_line.ends_with(r#"'{"message":"it'\''s"}'"#),
            "{bash_line}"
        );
    }
}
