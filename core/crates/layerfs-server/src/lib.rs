//! Authorized service operations and one concrete host Server assembly.
//!
//! `service` owns the authorized C1/C2/C5 request handler and its Project Init
//! adapter. `host` owns native process assembly: configured keys, Store paths,
//! the bounded native acceptor and the [`Server`] an SDK or operator composes.
#![forbid(unsafe_code)]
pub mod host;
mod service;
pub use host::{run, Server, ServerConfig};
pub use service::{Grant, Service, StoreAccess};
