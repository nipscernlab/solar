//! From a line of JSON to a response, with nothing in between that can end in silence.
//!
//! Dispatch is the only place that decides what happens to a call. It reads the envelope,
//! finds the API, checks the parameters, runs the handler on a worker thread, and turns
//! whatever comes back, including a panic and including nothing at all, into a response.

use std::any::Any;
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{RecvTimeoutError, sync_channel};
use std::sync::{Arc, Once};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::clock;
use crate::context::Context;
use crate::error::{ErrorDetail, SolarError};
use crate::logging;
use crate::meta::Meta;
use crate::protocol::{Request, Response, parse_request};
use crate::reason::Reason;
use crate::registry::{Registry, RegistryProblem};
use crate::status::Status;

thread_local! {
    /// Set on a worker thread, so that the panic hook knows the panic belongs to a call.
    static IN_DISPATCH: Cell<bool> = const { Cell::new(false) };
    /// Where the last panic on this thread happened, filled in by the hook.
    static LAST_PANIC_LOCATION: Cell<Option<(&'static str, u32, u32)>> = const { Cell::new(None) };
}

static HOOK: Once = Once::new();

/// Installs the panic hook that lets dispatch report where a handler panicked.
///
/// The hook records the location of a panic that happened inside a call and stays silent on
/// standard output; it forwards anything else to the hook that was already there, so that a
/// panic outside a call, in a test for instance, is still reported the usual way.
///
/// Calling this more than once is harmless: only the first call does anything.
pub fn install_panic_hook() {
    HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if IN_DISPATCH.get() {
                let location = info
                    .location()
                    .map(|at| (at.file(), at.line(), at.column()));
                LAST_PANIC_LOCATION.set(location.map(|(file, line, column)| {
                    (
                        Box::leak(file.to_owned().into_boxed_str()) as &'static str,
                        line,
                        column,
                    )
                }));
                logging::error(&format!("a handler panicked: {info}"));
            } else {
                previous(info);
            }
        }));
    });
}

/// What a panicking handler left behind.
#[derive(Debug, Clone)]
struct PanicRecord {
    message: String,
    location: Option<(&'static str, u32, u32)>,
}

/// Reads the payload of a panic as text, whatever it was thrown as.
fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&'static str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "a panic with a payload that is not a string".to_owned()
    }
}

/// What the dispatcher has to work with.
#[derive(Debug)]
enum State {
    /// A registry that passed every check.
    Ready(Arc<Registry>),
    /// A registry that did not, kept so that every call can say why.
    Broken(Vec<RegistryProblem>),
}

/// Turns requests into responses.
///
/// A dispatcher is cheap to clone through its `Arc` and is safe to share: it holds nothing
/// that changes between calls.
#[derive(Debug)]
pub struct Dispatcher {
    state: State,
}

impl Dispatcher {
    /// A dispatcher over a registry that built successfully.
    #[must_use]
    pub fn new(registry: Arc<Registry>) -> Self {
        install_panic_hook();
        Self {
            state: State::Ready(registry),
        }
    }

    /// A dispatcher over a registry that did not build.
    ///
    /// Every call it receives fails with the same `INTERNAL` error naming every rule that
    /// was broken. This exists so that a mistake in one API is reported to the caller
    /// instead of taking the process down at startup.
    #[must_use]
    pub fn broken(problems: Vec<RegistryProblem>) -> Self {
        install_panic_hook();
        Self {
            state: State::Broken(problems),
        }
    }

    /// The registry, when there is a usable one.
    #[must_use]
    pub fn registry(&self) -> Option<&Arc<Registry>> {
        match &self.state {
            State::Ready(registry) => Some(registry),
            State::Broken(_) => None,
        }
    }

    /// Reads one line and answers it. This is the whole protocol in one call.
    #[must_use]
    pub fn handle_line(&self, line: &str) -> Response {
        let started_at = clock::now_rfc3339_micros();
        let start = Instant::now();

        match parse_request(line) {
            Ok(request) => self.run(request, started_at, start),
            Err(failed) => {
                let meta = Meta::new(
                    failed.id.clone(),
                    failed.method.clone(),
                    None,
                    started_at,
                    elapsed_us(start),
                );
                Response::failure(failed.id, failed.error, meta)
            }
        }
    }

    /// Answers a request that has already passed the envelope checks.
    #[must_use]
    pub fn dispatch(&self, request: Request) -> Response {
        let started_at = clock::now_rfc3339_micros();
        self.run(request, started_at, Instant::now())
    }

    fn run(&self, request: Request, started_at: String, start: Instant) -> Response {
        let Request { id, method, params } = request;
        let registry = match &self.state {
            State::Ready(registry) => registry,
            State::Broken(problems) => {
                let meta = Meta::new(
                    Some(id.clone()),
                    Some(method),
                    None,
                    started_at,
                    elapsed_us(start),
                );
                return Response::failure(Some(id), Registry::build_failure(problems), meta);
            }
        };

        let Some(entry) = registry.get(&method) else {
            let error = registry.not_found(&method, Reason::MethodNotFound, "method");
            let meta = Meta::new(
                Some(id.clone()),
                Some(method),
                None,
                started_at,
                elapsed_us(start),
            );
            return Response::failure(Some(id), error, meta);
        };

        let api_version = Some(entry.version().to_owned());
        let params = Value::Object(params);
        let typed = match entry.validate(&params) {
            Ok(typed) => typed,
            Err(error) => {
                let meta = Meta::new(
                    Some(id.clone()),
                    Some(method),
                    api_version,
                    started_at,
                    elapsed_us(start),
                );
                return Response::failure(Some(id), error, meta);
            }
        };

        let budget = Duration::from_millis(entry.spec().timeout_ms);
        let ctx = Arc::new(Context::new(
            Some(id.clone()),
            method.clone(),
            budget,
            Arc::clone(registry),
        ));

        let outcome = run_with_budget(&ctx, entry.name(), typed, budget);
        let duration_us = elapsed_us(start);
        let warnings = ctx.warnings();
        let meta = Meta::new(
            Some(id.clone()),
            Some(method.clone()),
            api_version,
            started_at,
            duration_us,
        );

        match outcome {
            Ok(data) => Response::success(Some(id), data, meta, warnings),
            Err(error) => Response::failure(Some(id), error, meta),
        }
    }
}

/// Whole microseconds since `start`, saturating rather than wrapping.
fn elapsed_us(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX)
}

/// Runs a handler on a worker thread and waits for it, at most for `budget`.
///
/// Three things can come back: an output, an error the handler built, or a panic. A fourth
/// case is that nothing comes back in time, and that is what the budget is for. The thread
/// is abandoned rather than killed, which section 9 of the contract states plainly,
/// because there is no safe way to kill a thread in Rust.
fn run_with_budget(
    ctx: &Arc<Context>,
    name: &'static str,
    params: Box<dyn Any + Send>,
    budget: Duration,
) -> Result<Value, SolarError> {
    let (sender, receiver) = sync_channel::<Result<Result<Value, SolarError>, PanicRecord>>(1);
    let worker_ctx = Arc::clone(ctx);

    let spawned = thread::Builder::new()
        .name(format!("solar:{name}"))
        .spawn(move || {
            IN_DISPATCH.set(true);
            LAST_PANIC_LOCATION.set(None);
            let outcome =
                catch_unwind(AssertUnwindSafe(|| match worker_ctx.registry().get(name) {
                    Some(entry) => entry.invoke(&worker_ctx, params),
                    None => Err(SolarError::new(
                        Reason::InvariantBroken,
                        format!("{name} vanished from the registry while it was running."),
                    )),
                }));
            let message = match outcome {
                Ok(result) => Ok(result),
                Err(payload) => Err(PanicRecord {
                    message: panic_message(payload.as_ref()),
                    location: LAST_PANIC_LOCATION.get(),
                }),
            };
            let _ = sender.send(message);
        });

    let handle = match spawned {
        Ok(handle) => handle,
        Err(failure) => {
            return Err(SolarError::new(
                Reason::ThreadSpawnFailed,
                format!("SOLAR could not start the worker thread for {name}: {failure}."),
            )
            .with_detail(
                ErrorDetail::new(Status::Unavailable)
                    .field("method")
                    .expected("a thread from the operating system")
                    .received(Value::String(failure.to_string()))
                    .hint("The machine is out of threads or out of memory. Retry once the load drops."),
            ));
        }
    };

    match receiver.recv_timeout(budget) {
        Ok(Ok(result)) => {
            let _ = handle.join();
            result
        }
        Ok(Err(panic)) => {
            let _ = handle.join();
            Err(panic_error(name, &panic))
        }
        Err(RecvTimeoutError::Timeout) => Err(timeout_error(name, budget)),
        Err(RecvTimeoutError::Disconnected) => Err(SolarError::new(
            Reason::InvariantBroken,
            format!("The worker thread of {name} ended without answering."),
        )
        .with_detail(
            ErrorDetail::new(Status::Internal)
                .field("method")
                .expected("an answer from the worker thread")
                .hint("Report this with the exact request that caused it."),
        )),
    }
}

/// The `INTERNAL` error a panicking handler becomes.
fn panic_error(name: &str, panic: &PanicRecord) -> SolarError {
    let where_at = match panic.location {
        Some((file, line, column)) => format!("{file}:{line}:{column}"),
        None => "an unknown location".to_owned(),
    };
    SolarError::new(
        Reason::HandlerPanic,
        format!("{name} panicked: {}.", panic.message.trim_end_matches('.')),
    )
    .with_detail(
        ErrorDetail::new(Status::Internal)
            .field("method")
            .expected("a result or a declared error")
            .received(Value::String(panic.message.clone()))
            .hint(format!(
                "This is a bug in SOLAR, at {where_at}. The session survived it; report it \
                 with the request that caused it."
            )),
    )
}

/// The `DEADLINE_EXCEEDED` error a handler that ran too long becomes.
fn timeout_error(name: &str, budget: Duration) -> SolarError {
    SolarError::new(
        Reason::HandlerTimeout,
        format!("{name} did not finish within {} ms.", budget.as_millis()),
    )
    .with_detail(
        ErrorDetail::new(Status::DeadlineExceeded)
            .field("method")
            .expected(format!("an answer within {} ms", budget.as_millis()))
            .received(Value::String(name.to_owned()))
            .hint(
                "The budget of an API is the timeout_ms of its specification, which \
                 solar.describe reports. The work was abandoned, not undone.",
            ),
    )
}
