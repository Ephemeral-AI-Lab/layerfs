//! PostgreSQL I/O for the engine-independent storage and history contracts.
#![forbid(unsafe_code)]
#![deny(missing_docs)]
mod client;
mod config;
mod connection;
mod error;
mod params;
mod schema;
mod storage;
mod tls;
mod wire;
pub use config::{PgConfig, TlsProfile};
pub use storage::PgMetadata;
pub use wire::PgDiagnostics;

mod history;
pub use history::PgHistory;
