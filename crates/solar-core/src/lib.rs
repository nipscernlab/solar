//! The core of SOLAR: the protocol, the errors, the API template, the registry, dispatch.
//!
//! SOLAR is the central API of the Constellation project of NIPS-CERN. Every interface, the
//! `solar` command line interface, the AURORA IDE and artificial intelligence agents, talks
//! to SOLAR and to nothing else.
//!
//! This crate knows nothing about any particular API. It knows what an API is, how one is
//! described, how a call is read, checked, run and answered, and what an answer looks like.
//! The APIs themselves live in `solar-apis`.
//!
//! # The shape of everything
//!
//! - [`protocol`] reads a line into a [`protocol::Request`] and writes a
//!   [`protocol::Response`] back.
//! - [`api`] is the template every API follows: one trait, one specification.
//! - [`registry`] holds the APIs and checks them against that template.
//! - [`dispatch`] turns a request into a response and lets nothing end in silence.
//! - [`error`], [`status`] and [`reason`] are how a failure is told.
//! - [`manifest`] is everything this build can do, in one document.
//!
//! # Reading a call from start to finish
//!
//! ```
//! use std::sync::Arc;
//! use solar_core::dispatch::Dispatcher;
//! use solar_core::registry::RegistryBuilder;
//!
//! let registry = Arc::new(RegistryBuilder::new().build().expect("an empty registry is valid"));
//! let dispatcher = Dispatcher::new(registry);
//!
//! let response = dispatcher.handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"solar.ping"}"#);
//! // Nothing is registered in this example, so the answer is a NOT_FOUND that suggests
//! // nothing, and it is still a complete, well formed response.
//! assert!(!response.is_success());
//! assert!(response.to_line().starts_with(r#"{"jsonrpc":"2.0","id":1,"error":"#));
//! ```
//!
//! The contract these types implement is `docs/CONTRACT.md`, and it is normative.

pub mod api;
pub mod build_info;
pub mod clock;
pub mod context;
pub mod dispatch;
pub mod error;
pub mod logging;
pub mod manifest;
pub mod matching;
pub mod meta;
pub mod params;
pub mod protocol;
pub mod reason;
pub mod registry;
pub mod server;
pub mod status;
pub mod text;
pub mod warning;

pub use api::{Api, ApiSpec, ErrorSpec, Example, MatchMode, SideEffect, Stability};
pub use context::Context;
pub use dispatch::Dispatcher;
pub use error::{ErrorDetail, SolarError};
pub use meta::{Meta, PROTOCOL, SOLAR_VERSION};
pub use protocol::{Request, RequestId, Response};
pub use reason::Reason;
pub use registry::{Registry, RegistryBuilder, RegistryProblem};
pub use status::Status;
pub use warning::{Warning, WarningCode};
