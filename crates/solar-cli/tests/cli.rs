//! The binary, run as a caller would run it.
//!
//! These tests spawn the real `solar`, so they cover the things a unit test cannot: what
//! lands on standard output, what lands on standard error, and the exit code. The rule
//! that standard output carries protocol and nothing else is checked here, because here is
//! the only place it can be.

// A test reports failure by panicking, so the lints that forbid it in production code are
// lifted here. Integration tests are their own crate, which is why `clippy.toml` does not
// cover them.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test reports failure by panicking; integration tests are their own crate, \n              so clippy.toml does not cover them"
)]

use std::io::Write;
use std::process::{Command, Output, Stdio};

use serde_json::Value;

/// The binary cargo just built.
const SOLAR: &str = env!("CARGO_BIN_EXE_solar");

/// Runs `solar` with arguments and no input.
fn solar(arguments: &[&str]) -> Output {
    Command::new(SOLAR)
        .args(arguments)
        .output()
        .expect("the binary must run")
}

/// Runs `solar` with arguments, an environment variable and no input.
fn solar_with_env(arguments: &[&str], key: &str, value: &str) -> Output {
    Command::new(SOLAR)
        .args(arguments)
        .env(key, value)
        .output()
        .expect("the binary must run")
}

/// Feeds lines to `solar serve --stdio` and collects what comes back.
fn serve(input: &str) -> Output {
    let mut child = Command::new(SOLAR)
        .args(["serve", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary must run");
    child
        .stdin
        .take()
        .expect("stdin was piped")
        .write_all(input.as_bytes())
        .expect("the session must accept input");
    child.wait_with_output().expect("the session must end")
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("output is UTF-8")
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("output is UTF-8")
}

fn parse(line: &str) -> Value {
    serde_json::from_str(line.trim())
        .unwrap_or_else(|failure| panic!("{line:?} is not JSON: {failure}"))
}

#[test]
fn call_prints_one_envelope_on_one_line_and_exits_zero() {
    let output = solar(&["call", "solar.ping", "{\"message\":\"hi\"}"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(
        stderr_of(&output).is_empty(),
        "nothing belongs on stderr by default"
    );

    let text = stdout_of(&output);
    assert_eq!(text.lines().count(), 1, "one call, one line");
    let response = parse(&text);
    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 1);
    assert_eq!(response["result"]["data"]["echo"], "hi");
    assert_eq!(response["result"]["meta"]["method"], "solar.ping");
    assert_eq!(response["result"]["meta"]["protocol"], "solar/1");
    assert!(response["result"]["meta"]["duration_us"].is_number());
}

#[test]
fn logging_never_touches_stdout() {
    let output = solar_with_env(&["call", "solar.ping"], "SOLAR_LOG", "trace");
    assert_eq!(output.status.code(), Some(0));
    let text = stdout_of(&output);
    assert_eq!(
        text.lines().count(),
        1,
        "logging must not add a line to stdout"
    );
    assert!(parse(&text)["result"].is_object());
}

#[test]
fn pretty_indents_the_same_envelope() {
    let plain = solar(&["call", "solar.ping"]);
    let pretty = solar(&["call", "solar.ping", "--pretty"]);
    assert!(stdout_of(&pretty).lines().count() > 10);
    assert_eq!(
        parse(&stdout_of(&plain))["result"]["data"]["pong"],
        parse(&stdout_of(&pretty))["result"]["data"]["pong"]
    );
}

#[test]
fn an_unknown_method_exits_with_the_code_for_not_found() {
    let output = solar(&["call", "solar.pign"]);
    assert_eq!(output.status.code(), Some(3));
    let response = parse(&stdout_of(&output));
    assert_eq!(response["error"]["code"], -32601);
    assert_eq!(response["error"]["data"]["reason"], "METHOD_NOT_FOUND");
    assert_eq!(
        response["error"]["data"]["details"][0]["expected"],
        "solar.ping"
    );
}

#[test]
fn parameters_that_are_not_json_are_still_answered_with_an_envelope() {
    let output = solar(&["call", "solar.ping", "{not json"]);
    assert_eq!(output.status.code(), Some(2));
    let response = parse(&stdout_of(&output));
    assert_eq!(response["error"]["data"]["reason"], "PARSE_ERROR");
    assert_eq!(response["error"]["data"]["details"][0]["field"], "params");
}

#[test]
fn parameters_can_come_from_standard_input_when_a_shell_mangles_quotes() {
    let mut child = Command::new(SOLAR)
        .args(["call", "solar.ping", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("the binary must run");
    child
        .stdin
        .take()
        .expect("stdin was piped")
        .write_all(
            b"{\"message\":\"from stdin\"}
",
        )
        .expect("the call must accept input");
    let output = child.wait_with_output().expect("the call must end");

    assert_eq!(output.status.code(), Some(0));
    let response = parse(&stdout_of(&output));
    assert_eq!(response["result"]["data"]["echo"], "from stdin");
}

#[test]
fn the_hint_for_mangled_parameters_names_a_form_that_works() {
    let output = solar(&["call", "solar.ping", "{message:hi}"]);
    assert_eq!(output.status.code(), Some(2));
    let response = parse(&stdout_of(&output));
    let hint = response["error"]["data"]["details"][0]["hint"]
        .as_str()
        .unwrap();
    assert!(hint.contains("PowerShell"), "{hint}");
    assert!(hint.contains("standard input"), "{hint}");
}

#[test]
fn an_unknown_parameter_is_refused_with_the_name_that_was_meant() {
    let output = solar(&["call", "solar.ping", "{\"mesage\":\"hi\"}"]);
    assert_eq!(output.status.code(), Some(2));
    let response = parse(&stdout_of(&output));
    assert_eq!(response["error"]["data"]["reason"], "UNKNOWN_FIELD");
    let hint = response["error"]["data"]["details"][0]["hint"]
        .as_str()
        .unwrap();
    assert!(hint.contains("Did you mean message?"), "{hint}");
}

#[test]
fn a_notification_is_refused_because_every_call_gets_a_response() {
    let output = serve("{\"jsonrpc\":\"2.0\",\"method\":\"solar.ping\"}\n");
    let response = parse(&stdout_of(&output));
    assert_eq!(response["id"], Value::Null);
    assert_eq!(response["error"]["code"], -32600);
    assert_eq!(
        response["error"]["data"]["reason"],
        "NOTIFICATION_NOT_SUPPORTED"
    );
}

#[test]
fn a_batch_answers_with_one_line_holding_an_array() {
    let output = serve(
        "[{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"solar.ping\"},\
         {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"solar.version\"}]\n",
    );
    let stdout = stdout_of(&output);
    assert_eq!(stdout.lines().count(), 1, "one message, one line: {stdout}");
    let answers = parse(&stdout);
    let answers = answers.as_array().expect("a batch answers with an array");
    assert_eq!(answers.len(), 2);
    assert_eq!(answers[0]["id"], 1);
    assert_eq!(answers[0]["result"]["data"]["pong"], true);
    assert_eq!(answers[1]["id"], 2);
    assert!(answers[1]["result"]["data"]["solar_version"].is_string());
}

#[test]
fn an_empty_batch_is_refused_with_a_single_response() {
    let output = serve("[]\n");
    let response = parse(&stdout_of(&output));
    assert!(response.is_object(), "not an array: {response}");
    assert_eq!(response["error"]["data"]["status"], "INVALID_ARGUMENT");
    assert_eq!(response["error"]["data"]["reason"], "BATCH_EMPTY");
}

#[test]
fn a_session_answers_every_line_in_the_order_they_arrived() {
    use std::fmt::Write as _;
    let mut input = String::new();
    for id in 1..=4 {
        let _ = writeln!(
            input,
            "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"method\":\"solar.ping\"}}"
        );
    }
    let output = serve(&input);
    assert_eq!(
        output.status.code(),
        Some(0),
        "a session that ends cleanly exits zero"
    );

    let ids: Vec<i64> = stdout_of(&output)
        .lines()
        .map(|line| parse(line)["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, vec![1, 2, 3, 4]);
}

#[test]
fn a_session_survives_a_broken_line_and_keeps_going() {
    let output = serve("not json\n\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"solar.ping\"}\n");
    let lines: Vec<Value> = stdout_of(&output).lines().map(parse).collect();
    assert_eq!(lines.len(), 2, "the blank line is not a message");
    assert_eq!(lines[0]["error"]["code"], -32700);
    assert_eq!(lines[1]["result"]["data"]["pong"], true);
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn a_byte_order_mark_does_not_cost_a_caller_its_first_request() {
    // Windows PowerShell writes one in front of the first line it sends to a native
    // program. RFC 8259 allows a parser to ignore it, and SOLAR does.
    let input = "\u{feff}{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"solar.ping\"}\n\
                 {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"solar.ping\"}\n";
    let output = serve(input);
    let ids: Vec<i64> = stdout_of(&output)
        .lines()
        .map(|line| parse(line)["id"].as_i64().unwrap_or(-1))
        .collect();
    assert_eq!(
        ids,
        vec![1, 2],
        "the first request was lost to the byte order mark"
    );
}

/// Runs a session, recording it into a file, and gives back the path.
fn serve_recording(input: &str, into: &std::path::Path) -> Output {
    let mut child = Command::new(SOLAR)
        .args(["serve", "--stdio", "--record"])
        .arg(into)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary must run");
    child
        .stdin
        .take()
        .expect("stdin was piped")
        .write_all(input.as_bytes())
        .expect("the session must accept input");
    child.wait_with_output().expect("the session must end")
}

#[test]
fn a_session_can_be_recorded_and_played_back() {
    let directory = std::env::temp_dir().join(format!("solar-replay-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("a directory for the recording");
    let recording = directory.join("session.ndjson");

    let input = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"solar.ping\",\"params\":{\"message\":\"one\"}}\n\
                 {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"nope.nope\"}\n";
    let served = serve_recording(input, &recording);
    assert_eq!(served.status.code(), Some(0));
    assert_eq!(stdout_of(&served).lines().count(), 2);

    // Two requests and two responses. A session reads on a thread of its own, so the
    // recording is a chronological log rather than strict alternation: what is promised
    // is that every line that crossed is in it, with the time it crossed.
    let written = std::fs::read_to_string(&recording).expect("the recording exists");
    let entries: Vec<Value> = written.lines().map(parse).collect();
    assert_eq!(entries.len(), 4);
    let directions: Vec<&str> = entries
        .iter()
        .map(|entry| entry["direction"].as_str().unwrap())
        .collect();
    assert_eq!(directions.iter().filter(|d| **d == "in").count(), 2);
    assert_eq!(directions.iter().filter(|d| **d == "out").count(), 2);
    assert_eq!(directions[0], "in", "a session records what arrives first");
    assert!(entries[0]["at"].as_str().unwrap().ends_with('Z'));

    // Played back against the same build, nothing differs.
    let replayed = Command::new(SOLAR)
        .arg("replay")
        .arg(&recording)
        .output()
        .expect("the binary must run");
    assert_eq!(replayed.status.code(), Some(0), "{}", stdout_of(&replayed));
    assert!(stdout_of(&replayed).contains("2 requests sent, 0 answers differ"));

    // An answer that really changed is reported, and the exit code says so.
    let tampered = directory.join("tampered.ndjson");
    std::fs::write(
        &tampered,
        written.replace(r#"\"echo\":\"one\""#, r#"\"echo\":\"ONE\""#),
    )
    .expect("the tampered recording is written");
    let caught = Command::new(SOLAR)
        .arg("replay")
        .arg(&tampered)
        .output()
        .expect("runs");
    assert!(
        stdout_of(&caught).contains("1 answer differs"),
        "{}",
        stdout_of(&caught)
    );
    assert_eq!(caught.status.code(), Some(5), "FAILED_PRECONDITION");

    std::fs::remove_dir_all(&directory).ok();
}

#[test]
fn replaying_something_that_is_not_a_recording_says_so() {
    let directory = std::env::temp_dir().join(format!("solar-replay-bad-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("a directory");
    let nonsense = directory.join("nonsense.ndjson");
    std::fs::write(&nonsense, "this is not a recording\n").expect("written");

    let output = Command::new(SOLAR)
        .arg("replay")
        .arg(&nonsense)
        .output()
        .expect("runs");
    assert_eq!(output.status.code(), Some(2), "INVALID_ARGUMENT");
    assert!(stderr_of(&output).contains("is not a recording"));

    let missing = Command::new(SOLAR)
        .arg("replay")
        .arg(directory.join("nope"))
        .output()
        .expect("runs");
    assert_eq!(missing.status.code(), Some(3), "NOT_FOUND");

    std::fs::remove_dir_all(&directory).ok();
}

#[test]
fn json_logs_are_one_object_per_line_naming_the_call() {
    let output = Command::new(SOLAR)
        .args(["call", "solar.ping"])
        .env("SOLAR_LOG", "trace")
        .env("SOLAR_LOG_FORMAT", "json")
        .output()
        .expect("the binary must run");
    assert_eq!(output.status.code(), Some(0));

    let diagnostics = stderr_of(&output);
    assert!(!diagnostics.is_empty(), "trace level writes something");
    for line in diagnostics.lines() {
        let entry: Value = serde_json::from_str(line)
            .unwrap_or_else(|failure| panic!("{line:?} is not one JSON object: {failure}"));
        for member in [
            "time",
            "level",
            "request_id",
            "method",
            "duration_us",
            "message",
        ] {
            assert!(
                entry.get(member).is_some(),
                "{member} is missing from {line}"
            );
        }
        assert_eq!(
            entry["method"], "solar.ping",
            "every line names the call it belongs to"
        );
        assert_eq!(entry["request_id"], 1);
    }
    // The line that reports the answer carries how long it took.
    assert!(
        diagnostics.lines().any(|line| {
            serde_json::from_str::<Value>(line).is_ok_and(|entry| entry["duration_us"].is_number())
        }),
        "one line reports the duration: {diagnostics}"
    );
    assert_eq!(
        stdout_of(&output).lines().count(),
        1,
        "logging never touches stdout"
    );
}

#[test]
fn a_session_with_no_transport_is_refused() {
    let output = solar(&["serve"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr_of(&output).contains("--stdio"));
    assert!(stdout_of(&output).is_empty());
}

#[test]
fn list_names_every_api_with_its_summary() {
    let output = solar(&["list"]);
    assert_eq!(output.status.code(), Some(0));
    let text = stdout_of(&output);
    for name in [
        "solar.ping",
        "solar.version",
        "solar.manifest",
        "solar.describe",
        "system.info",
    ] {
        assert!(text.contains(name), "{name} is missing from `solar list`");
    }
    assert!(text.contains("solar/1"));
}

#[test]
fn describe_lays_one_api_out_for_a_person() {
    let output = solar(&["describe", "solar.ping"]);
    assert_eq!(output.status.code(), Some(0));
    let text = stdout_of(&output);
    assert!(text.starts_with("solar.ping 1.0.0"));
    assert!(text.contains("parameters"));
    assert!(text.contains("message"));
    assert!(text.contains("examples"));
    assert!(text.contains("solar call solar.ping"));
}

#[test]
fn describing_something_that_is_not_there_explains_itself_on_stderr() {
    let output = solar(&["describe", "solar.pign"]);
    assert_eq!(output.status.code(), Some(3));
    assert!(stdout_of(&output).is_empty(), "an error is not a result");
    let text = stderr_of(&output);
    assert!(text.contains("NOT_FOUND / API_NOT_FOUND"));
    assert!(text.contains("Did you mean \"solar.ping\"?"));
    assert!(text.contains("docs/ERRORS.md#not_found"));
}

#[test]
fn manifest_is_a_json_document_when_it_goes_into_a_pipe() {
    let output = solar(&["manifest"]);
    assert_eq!(output.status.code(), Some(0));
    let manifest = parse(&stdout_of(&output));
    assert_eq!(manifest["protocol"], "solar/1");
    assert!(manifest["apis"].as_array().unwrap().len() >= 5);

    let one = parse(&stdout_of(&solar(&["manifest", "--api", "solar.ping"])));
    assert_eq!(one["apis"].as_array().unwrap().len(), 1);
    assert_eq!(one["apis"][0]["name"], "solar.ping");
}

#[test]
fn version_reports_the_three_versions_and_the_build() {
    let output = solar(&["version"]);
    assert_eq!(output.status.code(), Some(0));
    let text = stdout_of(&output);
    assert!(text.contains("protocol       solar/1"));
    assert!(text.contains("target"));
    assert!(text.contains("compiler       rustc"));
}

#[test]
fn the_binary_explains_itself() {
    let output = solar(&["--help"]);
    assert_eq!(output.status.code(), Some(0));
    let text = stdout_of(&output);
    for command in ["call", "serve", "list", "describe", "manifest", "version"] {
        assert!(text.contains(command), "{command} is missing from the help");
    }
    assert!(text.contains("Exit codes"));
}

#[test]
fn a_command_that_does_not_exist_is_a_usage_error() {
    let output = solar(&["nonsense"]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "a misuse exits like an invalid argument"
    );
}

#[test]
fn a_log_level_that_does_not_exist_is_refused_rather_than_ignored() {
    let output = solar(&["--log", "loud", "call", "solar.ping"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr_of(&output).contains("is not a level"));
}

#[test]
fn call_logs_the_traffic_at_trace_level_and_nothing_by_default() {
    // The defect this pins down: `solar call` used to leave standard error empty at
    // trace level, while `solar serve --stdio` logged, and the README promised both.
    let silent = solar(&["call", "solar.ping"]);
    assert_eq!(silent.status.code(), Some(0));
    assert!(
        stderr_of(&silent).is_empty(),
        "by default standard error carries nothing: {}",
        stderr_of(&silent)
    );

    let traced = solar_with_env(&["call", "solar.ping"], "SOLAR_LOG", "trace");
    assert_eq!(traced.status.code(), Some(0));
    let diagnostics = stderr_of(&traced);
    assert!(
        diagnostics.contains("--> "),
        "the request line is logged: {diagnostics}"
    );
    assert!(
        diagnostics.contains("<-- "),
        "the response line is logged: {diagnostics}"
    );
    assert!(
        diagnostics.contains("solar.ping answered OK"),
        "{diagnostics}"
    );
    assert_eq!(
        stdout_of(&traced).lines().count(),
        1,
        "logging must never add a line to standard output"
    );

    let flagged = solar(&["--log", "trace", "call", "solar.ping"]);
    assert!(stderr_of(&flagged).contains("--> "), "--log trace logs too");
}

#[test]
fn asking_for_logs_puts_them_on_standard_error() {
    let output = solar_with_env(&["serve", "--stdio"], "SOLAR_LOG", "trace");
    assert_eq!(output.status.code(), Some(0));
    assert!(
        stdout_of(&output).is_empty(),
        "an empty session writes no protocol"
    );
    assert!(
        stderr_of(&output).contains("the session ended"),
        "{}",
        stderr_of(&output)
    );
}

#[test]
fn the_log_level_changes_while_the_session_runs_and_stdout_never_moves() {
    // The second thing ZENITH asked for: an interface that wants to show more has to be
    // able to ask for it without restarting SOLAR and losing the session.
    let input = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"solar.ping"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"solar.set_log_level","params":{"level":"trace"}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":3,"method":"solar.ping"}"#,
        "\n",
    );
    let output = serve(input);
    assert_eq!(output.status.code(), Some(0));

    let stdout = stdout_of(&output);
    let answers: Vec<Value> = stdout.lines().map(parse).collect();
    assert_eq!(
        answers.len(),
        3,
        "three requests, three responses: {stdout}"
    );
    assert_eq!(answers[1]["result"]["data"]["previous"], "off");
    assert_eq!(answers[1]["result"]["data"]["current"], "trace");
    assert_eq!(answers[1]["result"]["data"]["changed"], true);

    // The first ping ran at the level the session started with, which is off, and the
    // third ran at trace. Standard error therefore names the third call and not the first.
    let diagnostics = stderr_of(&output);
    assert!(
        diagnostics.contains("\"id\":3") || diagnostics.contains("id\":3"),
        "the call made after the change is logged: {diagnostics}"
    );
    assert!(
        !diagnostics.contains("\"id\":1,\"method\":\"solar.ping\""),
        "the call made before it is not: {diagnostics}"
    );

    // And the promise that matters more: nothing of this reached standard output.
    for line in stdout.lines() {
        let value: Value = parse(line);
        assert!(
            value.get("jsonrpc").is_some(),
            "standard output carries protocol only: {line}"
        );
    }
}

#[test]
fn asking_for_a_level_nobody_can_read_is_refused_and_names_the_six() {
    let output = serve(concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"solar.set_log_level","params":{"level":"loud"}}"#,
        "\n",
    ));
    let response = parse(&stdout_of(&output));
    assert_eq!(response["error"]["data"]["reason"], "INVALID_VALUE");
    assert_eq!(response["error"]["data"]["status"], "INVALID_ARGUMENT");
    let expected = response["error"]["data"]["details"][0]["expected"]
        .as_str()
        .unwrap_or_default();
    for level in ["off", "error", "warn", "info", "debug", "trace"] {
        assert!(expected.contains(level), "{level} is not in {expected:?}");
    }
}
