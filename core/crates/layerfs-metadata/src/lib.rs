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
mod work;
pub use config::{PgConfig, TlsProfile};
pub use storage::PgMetadata;
pub use wire::PgDiagnostics;
pub use work::{PgStatementWork, PgWork};

mod history;
pub use history::PgHistory;
