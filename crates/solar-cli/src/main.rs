//! `solar`: the command line interface of SOLAR.
//!
//! It is two things at once. It is the debugger of the protocol, through `solar call`,
//! which prints the whole response envelope and nothing else, so that what a human sees is
//! byte for byte what an agent would receive. And it is the way a person uses SOLAR,
//! through `solar list`, `solar describe` and the rest, which are the same APIs underneath
//! with their output laid out to be read.
//!
//! There is no second implementation anywhere in here: every command builds a request,
//! hands it to the same dispatcher a session would use, and renders what comes back.

// The command line interface is where SOLAR talks to a person, so it prints to standard
// output on purpose. The rule that keeps stdout clear of everything but protocol applies
// to the library, and to `call` and `serve`, which print protocol and nothing else.
#![allow(
    clippy::print_stdout,
    reason = "the command line interface is where SOLAR talks to a person; `call` and               `serve` print protocol and nothing else, and a test enforces that"
)]

use std::io::{IsTerminal, Write};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use solar_core::clock;
use solar_core::dispatch::Dispatcher;
use solar_core::error::{ErrorDetail, SolarError};
use solar_core::logging::{self, Level};
use solar_core::meta::Meta;
use solar_core::protocol::{RequestId, Response};
use solar_core::reason::Reason;
use solar_core::recording::Direction;
use solar_core::status::Status;

mod banner;
mod render;

/// The exit code for a response that could not be written at all.
const CANNOT_ANSWER: u8 = 70;

#[derive(Debug, Parser)]
#[command(
    name = "solar",
    version,
    about = "The central API of the Constellation project, from NIPS-CERN",
    long_about = "SOLAR speaks JSON-RPC 2.0 over standard input and output, one message \
                  per line. `solar call` prints the whole envelope, which is what any \
                  other interface would receive; the other commands are the same APIs \
                  with their output laid out for a person.\n\nThe contract is \
                  docs/CONTRACT.md and the errors are docs/ERRORS.md.",
    after_help = "Exit codes: 0 success, 2 invalid argument, 3 not found, 5 failed \
                  precondition, 8 deadline exceeded, 9 unavailable, 10 unimplemented, \
                  11 internal. The whole table is in docs/CONTRACT.md."
)]
struct Cli {
    /// Indent the JSON that goes to standard output.
    #[arg(long, global = true)]
    pretty: bool,

    /// Diagnostics on standard error: off, error, warn, info, debug or trace.
    #[arg(long, global = true, value_name = "LEVEL")]
    log: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run one call and print the whole response envelope.
    Call {
        /// The method, for example solar.ping.
        method: String,
        /// The parameters, as one JSON object. Omit for none, or pass - to read them
        /// from standard input, which no shell can mangle.
        params: Option<String>,
    },
    /// Answer requests on standard input until it ends, one JSON message per line.
    Serve {
        /// Use standard input and output. The only transport in solar/1.
        #[arg(long)]
        stdio: bool,
        /// Write every line in and every line out to this file, as NDJSON. Attach it to
        /// a bug report; `solar replay` plays it back.
        #[arg(long, value_name = "FILE")]
        record: Option<std::path::PathBuf>,
    },

    /// Send the requests of a recording again and report where the answers differ.
    Replay {
        /// A file written by `solar serve --stdio --record`.
        file: std::path::PathBuf,
    },
    /// List every API this build answers to.
    List,
    /// Describe one API, with its parameters, its errors and its examples.
    Describe {
        /// The method to describe, for example solar.ping.
        method: String,
    },
    /// Print the manifest.
    Manifest {
        /// Narrow it to one API.
        #[arg(long, value_name = "METHOD")]
        api: Option<String>,
    },
    /// Print the version of SOLAR and how this binary was built.
    Version,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    if let Some(level) = cli.log.as_deref() {
        let Some(level) = Level::parse(level) else {
            eprintln!(
                "solar: --log {level} is not a level. Use off, error, warn, info, debug or                  trace."
            );
            return ExitCode::from(Status::InvalidArgument.exit_code());
        };
        logging::set_level(level);
    }

    let dispatcher = solar_apis::dispatcher();

    let outcome = match cli.command {
        Command::Call { method, params } => {
            call(&dispatcher, &method, params.as_deref(), cli.pretty)
        }
        Command::Serve { stdio, record } => serve(&dispatcher, stdio, record.as_deref()),
        Command::Replay { file } => replay(&dispatcher, &file),
        Command::List => list(&dispatcher),
        Command::Describe { method } => describe(&dispatcher, &method),
        Command::Manifest { api } => manifest(&dispatcher, api.as_deref(), cli.pretty),
        Command::Version => version(&dispatcher),
    };

    match outcome {
        Ok(code) => code,
        Err(failure) => {
            eprintln!("solar: the answer could not be written: {failure}");
            ExitCode::from(CANNOT_ANSWER)
        }
    }
}

/// Runs one call and prints the envelope, which is the whole point of this command.
fn call(
    dispatcher: &Dispatcher,
    method: &str,
    params: Option<&str>,
    pretty: bool,
) -> std::io::Result<ExitCode> {
    // Every diagnostic line of this call names the call, as in a session.
    let call = logging::Call {
        request_id: Some(json!(1)),
        method: Some(method.to_owned()),
    };
    let response = logging::during_call(call.clone(), || match read_params(method, params) {
        Ok(params) => {
            let request = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
            let line = request.to_string();
            // One call logs what a session would log for it: the line in, the line out.
            logging::trace(&format!("--> {line}"));
            dispatcher.handle_line(&line)
        }
        Err(response) => *response,
    });
    let duration_us = response_duration_us(&response);
    logging::during_call(call, || {
        logging::trace(&format!("<-- {}", response.to_line()));
        logging::log_with(
            logging::Level::Info,
            &format!(
                "{method} answered {}",
                response
                    .status()
                    .map_or_else(|| "OK".to_owned(), |status| status.to_string())
            ),
            Some(duration_us),
        );
    });

    let text = if pretty {
        response.to_pretty()
    } else {
        response.to_line()
    };
    let mut out = std::io::stdout().lock();
    writeln!(out, "{text}")?;
    out.flush()?;
    Ok(exit_code_of(&response))
}

/// What to do about a shell that will not leave the quotes alone.
///
/// Windows PowerShell 5.1 strips double quotes before a native program sees them, so the
/// obvious form is the one that does not work there. The escaped form below was tried on
/// the machine this was written on; `--%` was tried too and did not help.
const SHELL_HINT: &str = r#"Pass the object as one argument. Windows PowerShell removes double quotes before the program sees them, so escape them there: '{\"message\":\"hi\"}'. A single - reads the parameters from standard input instead."#;

/// Reads the parameters given on the command line, or the response that refuses them.
///
/// A single `-` means standard input, read to its end, which is the way out of a shell
/// that mangles quotes.
fn read_params(method: &str, params: Option<&str>) -> Result<Value, Box<Response>> {
    let Some(argument) = params else {
        return Ok(json!({}));
    };

    let text = if argument == "-" {
        let mut typed = String::new();
        match std::io::Read::read_to_string(&mut std::io::stdin().lock(), &mut typed) {
            Ok(_) => typed,
            Err(failure) => {
                return Err(refused(
                    method,
                    "-",
                    &format!("standard input could not be read: {failure}"),
                ));
            }
        }
    } else {
        argument.to_owned()
    };

    let trimmed = solar_core::text::strip_bom(text.trim()).trim();
    serde_json::from_str::<Value>(trimmed).map_err(|failure| {
        refused(
            method,
            trimmed,
            &format!("they are not valid JSON: {failure}"),
        )
    })
}

/// The response that refuses what arrived on the command line.
fn refused(method: &str, received: &str, why: &str) -> Box<Response> {
    let error = SolarError::new(
        Reason::ParseError,
        format!("The parameters given on the command line cannot be used: {why}."),
    )
    .with_detail(
        ErrorDetail::new(Status::InvalidArgument)
            .field("params")
            .expected("one JSON object, for example {\"message\":\"hi\"}")
            .received(received)
            .hint(SHELL_HINT),
    );
    let meta = Meta::new(
        Some(RequestId::Number(1.into())),
        Some(method.to_owned()),
        None,
        clock::now_rfc3339_micros(),
        0,
    );
    Box::new(Response::failure(
        Some(RequestId::Number(1.into())),
        error,
        meta,
    ))
}

/// Answers on standard input until it ends.
fn serve(
    dispatcher: &Dispatcher,
    stdio: bool,
    record: Option<&std::path::Path>,
) -> std::io::Result<ExitCode> {
    if !stdio {
        eprintln!(
            "solar: serve needs --stdio, which is the only transport in solar/1. There is no \
             port to listen on, by design."
        );
        return Ok(ExitCode::from(Status::InvalidArgument.exit_code()));
    }

    let input = std::io::stdin();
    let output = std::io::stdout();
    if input.is_terminal() {
        logging::info("reading from a terminal: one JSON object per line, Ctrl+Z or Ctrl+D to end");
    }
    match record {
        None => {
            solar_core::server::serve(input.lock(), output.lock(), dispatcher)?;
        }
        Some(path) => {
            let file = std::fs::File::create(path)?;
            let mut writer = std::io::BufWriter::new(file);
            logging::info(&format!("recording this session into {}", path.display()));
            solar_core::server::serve_recording(
                input.lock(),
                output.lock(),
                dispatcher,
                Some(&mut writer),
            )?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Sends the requests of a recording again and reports where the answers differ.
///
/// The volatile members of `meta`, which differ between any two runs, are replaced on
/// both sides before comparing; everything else, including the whole of `data`, is
/// compared as it stands.
#[allow(
    clippy::unnecessary_wraps,
    reason = "every command of this binary returns the same type, so main can treat               them alike; this one happens to have nothing that can fail on output"
)]
fn replay(dispatcher: &Dispatcher, file: &std::path::Path) -> std::io::Result<ExitCode> {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(failure) => {
            eprintln!("solar: {} could not be read: {failure}", file.display());
            return Ok(ExitCode::from(Status::NotFound.exit_code()));
        }
    };
    let entries = match solar_core::recording::read(&text) {
        Ok(entries) => entries,
        Err(why) => {
            eprintln!("solar: {} is not a recording: {why}", file.display());
            return Ok(ExitCode::from(Status::InvalidArgument.exit_code()));
        }
    };

    let mut sent = 0usize;
    let mut differing = 0usize;
    let mut expected: Option<String> = None;

    for entry in entries {
        match entry.direction {
            Direction::In => {
                let answered = dispatcher.handle_line(&entry.line).to_line();
                expected = Some(answered);
                sent += 1;
            }
            Direction::Out => {
                let Some(answered) = expected.take() else {
                    // A response with no request before it: the recording is not a
                    // session, and replaying it would prove nothing.
                    eprintln!("solar: the recording has an answer before any request");
                    return Ok(ExitCode::from(Status::InvalidArgument.exit_code()));
                };
                let then = solar_core::recording::without_volatile_values(&entry.line);
                let now = solar_core::recording::without_volatile_values(&answered);
                if then != now {
                    differing += 1;
                    println!("differs, request {sent}:");
                    println!("  recorded  {}", entry.line);
                    println!("  now       {answered}");
                }
            }
        }
    }

    println!(
        "replay: {sent} request{} sent, {differing} answer{} differ{}",
        if sent == 1 { "" } else { "s" },
        if differing == 1 { "" } else { "s" },
        if differing == 1 { "s" } else { "" }
    );

    if differing == 0 {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::from(Status::FailedPrecondition.exit_code()))
    }
}

/// The `duration_us` a response carries, for the log line about it.
fn response_duration_us(response: &Response) -> u64 {
    let meta = response
        .result
        .as_ref()
        .map(|body| &body.meta)
        .or_else(|| response.error.as_ref().map(|body| &body.data.meta));
    meta.map_or(0, |meta| meta.duration_us)
}

/// The exit code a response deserves, from the table in section 12 of the contract.
fn exit_code_of(response: &Response) -> ExitCode {
    match response.status() {
        None => ExitCode::SUCCESS,
        Some(status) => ExitCode::from(status.exit_code()),
    }
}

/// Makes one call for a command that renders the answer for a person.
///
/// The call goes through the same dispatcher as everything else, so a human command can
/// never disagree with the protocol: there is only one implementation.
fn human_call(
    dispatcher: &Dispatcher,
    method: &str,
    params: &Value,
) -> Result<Value, Box<Response>> {
    let request = json!({"jsonrpc": "2.0", "id": method, "method": method, "params": params});
    let response = dispatcher.handle_line(&request.to_string());
    match &response.result {
        Some(body) => Ok(body.data.clone()),
        None => Err(Box::new(response)),
    }
}

/// Prints a failed response for a person, on standard error.
fn report(response: &Response) -> ExitCode {
    if let Some(error) = &response.error {
        eprintln!("solar: {}", error.message);
        eprintln!("  {} / {}", error.data.status, error.data.reason);
        for detail in &error.data.details {
            let position = detail.field.as_deref().unwrap_or("-");
            match (&detail.expected, &detail.hint) {
                (Some(expected), Some(hint)) => {
                    eprintln!("  {position}: expected {expected}");
                    eprintln!("    {hint}");
                }
                (Some(expected), None) => eprintln!("  {position}: expected {expected}"),
                (None, Some(hint)) => eprintln!("  {position}: {hint}"),
                (None, None) => {}
            }
        }
        eprintln!("  see {}", error.data.status.docs());
    }
    exit_code_of(response)
}

/// `solar list`
fn list(dispatcher: &Dispatcher) -> std::io::Result<ExitCode> {
    match human_call(dispatcher, "solar.manifest", &json!({})) {
        Err(response) => Ok(report(&response)),
        Ok(data) => {
            let mut out = std::io::stdout().lock();
            render::list(&mut out, &data)?;
            out.flush()?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// `solar describe <method>`
fn describe(dispatcher: &Dispatcher, method: &str) -> std::io::Result<ExitCode> {
    match human_call(dispatcher, "solar.describe", &json!({"api": method})) {
        Err(response) => Ok(report(&response)),
        Ok(data) => {
            let mut out = std::io::stdout().lock();
            render::describe(&mut out, &data)?;
            out.flush()?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// `solar manifest`
fn manifest(dispatcher: &Dispatcher, api: Option<&str>, pretty: bool) -> std::io::Result<ExitCode> {
    let params = match api {
        Some(name) => json!({"api": name}),
        None => json!({}),
    };
    match human_call(dispatcher, "solar.manifest", &params) {
        Err(response) => Ok(report(&response)),
        Ok(data) => {
            // A person at a terminal wants the document laid out; a pipe wants one line.
            let indent = pretty || std::io::stdout().is_terminal();
            let text = if indent {
                serde_json::to_string_pretty(&data).unwrap_or_else(|_| data.to_string())
            } else {
                data.to_string()
            };
            let mut out = std::io::stdout().lock();
            writeln!(out, "{text}")?;
            out.flush()?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// `solar version`
fn version(dispatcher: &Dispatcher) -> std::io::Result<ExitCode> {
    match human_call(dispatcher, "solar.version", &json!({})) {
        Err(response) => Ok(report(&response)),
        Ok(data) => {
            // The mark belongs in output meant for a person, never in a pipe.
            let decorate = std::io::stdout().is_terminal();
            let mut out = std::io::stdout().lock();
            render::version(&mut out, &data, decorate, banner::wants_colour())?;
            out.flush()?;
            Ok(ExitCode::SUCCESS)
        }
    }
}
