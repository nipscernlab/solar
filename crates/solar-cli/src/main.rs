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
#![allow(clippy::print_stdout)]

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
use solar_core::status::Status;

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
        /// The parameters, as one JSON object. Omit for none.
        params: Option<String>,
    },
    /// Answer requests on standard input until it ends, one JSON message per line.
    Serve {
        /// Use standard input and output. The only transport in solar/1.
        #[arg(long)]
        stdio: bool,
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
        match Level::parse(level) {
            Some(level) => logging::set_level(level),
            None => {
                eprintln!(
                    "solar: --log {level} is not a level. Use off, error, warn, info, debug \
                     or trace."
                );
                return ExitCode::from(Status::InvalidArgument.exit_code() as u8);
            }
        }
    }

    let dispatcher = solar_apis::dispatcher();

    let outcome = match cli.command {
        Command::Call { method, params } => {
            call(&dispatcher, &method, params.as_deref(), cli.pretty)
        }
        Command::Serve { stdio } => serve(&dispatcher, stdio),
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
    let response = match read_params(method, params) {
        Ok(params) => {
            let request = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
            dispatcher.handle_line(&request.to_string())
        }
        Err(response) => *response,
    };

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

/// Reads the parameters given on the command line, or the response that refuses them.
fn read_params(method: &str, params: Option<&str>) -> Result<Value, Box<Response>> {
    let Some(text) = params else {
        return Ok(json!({}));
    };
    serde_json::from_str::<Value>(text).map_err(|failure| {
        let error = SolarError::new(
            Reason::ParseError,
            format!("The parameters given on the command line are not valid JSON: {failure}."),
        )
        .with_detail(
            ErrorDetail::new(Status::InvalidArgument)
                .field("params")
                .expected("one JSON object, for example {\"message\":\"hi\"}")
                .received(text)
                .hint(
                    "Quote the whole object as one argument. A shell that eats double \
                     quotes needs them escaped, or single quotes around the object.",
                ),
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
    })
}

/// Answers on standard input until it ends.
fn serve(dispatcher: &Dispatcher, stdio: bool) -> std::io::Result<ExitCode> {
    if !stdio {
        eprintln!(
            "solar: serve needs --stdio, which is the only transport in solar/1. There is no \
             port to listen on, by design."
        );
        return Ok(ExitCode::from(Status::InvalidArgument.exit_code() as u8));
    }

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    if stdin.is_terminal() {
        logging::info("reading from a terminal: one JSON object per line, Ctrl+Z or Ctrl+D to end");
    }
    solar_core::server::serve(stdin.lock(), stdout.lock(), dispatcher)?;
    Ok(ExitCode::SUCCESS)
}

/// The exit code a response deserves, from the table in section 12 of the contract.
fn exit_code_of(response: &Response) -> ExitCode {
    match response.status() {
        None => ExitCode::SUCCESS,
        Some(status) => ExitCode::from(status.exit_code() as u8),
    }
}

/// Makes one call for a command that renders the answer for a person.
///
/// The call goes through the same dispatcher as everything else, so a human command can
/// never disagree with the protocol: there is only one implementation.
fn human_call(
    dispatcher: &Dispatcher,
    method: &str,
    params: Value,
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
    match human_call(dispatcher, "solar.manifest", json!({})) {
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
    match human_call(dispatcher, "solar.describe", json!({"api": method})) {
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
    match human_call(dispatcher, "solar.manifest", params) {
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
    match human_call(dispatcher, "solar.version", json!({})) {
        Err(response) => Ok(report(&response)),
        Ok(data) => {
            let mut out = std::io::stdout().lock();
            render::version(&mut out, &data)?;
            out.flush()?;
            Ok(ExitCode::SUCCESS)
        }
    }
}
