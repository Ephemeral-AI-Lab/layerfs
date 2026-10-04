//! Shared persistence adapter; durable embedded SQLite is the available engine.
#![forbid(unsafe_code)]
#![deny(missing_docs)]
mod backend;
mod config;
mod handles;
mod history;
mod open;
mod publication;
mod storage_provider;
pub use backend::sqlite::connection::{Checkpoint, ConnectionProfile, SqlWork};
pub use backend::sqlite::statement_work::StatementPhaseWork;
pub use config::{BackendSelection, PersistenceConfig};
pub use handles::Handles;
pub use history::HistoryProvider;
pub use storage_provider::StorageProvider;

mod metadata;
mod objects;
