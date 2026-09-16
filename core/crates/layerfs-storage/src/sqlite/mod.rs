//! Embedded persistence: connection profile, schema, bounded queries and cleanup.
//!
//! Entry module: declarations and re-exports only.

pub mod cleanup;
pub mod connection;
pub mod lookup;
pub mod schema;
pub mod write;

pub use cleanup::CleanupReport;
pub use lookup::ObjectLocation;
pub use write::{ObjectRow, TransactionState};
