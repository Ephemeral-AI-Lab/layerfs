//! Authenticated owner of one DurableStore and its Sync server handlers.

#![forbid(unsafe_code)]

mod auth;
pub mod cli;
mod client;
mod error;
mod protocol;
mod server;
mod transport;

pub use auth::{AuthenticatedSession, Service};
pub use client::RemoteEndpoint;
pub use error::{Result, ServiceError};
pub use protocol::limits::{MAX_WIRE_BYTES, MIN_BEARER_BYTES};
pub use transport::listener::serve_loopback;

pub const COMPONENT: &str = "layerfs-service";
