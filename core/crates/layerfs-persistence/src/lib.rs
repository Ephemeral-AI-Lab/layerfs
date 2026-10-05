//! Shared SQLite adapter with explicit Durable and disk-backed Disposable profiles.
#![forbid(unsafe_code)]
#![deny(missing_docs)]
mod backend;
mod storage;
mod store;
pub(crate) use store::config;
pub(crate) use store::handles;
mod history;
pub use backend::sqlite::connection::{AllocationIdentity, Checkpoint, ConnectionProfile, SqlWork};
pub use backend::sqlite::statement_work::StatementPhaseWork;
pub use config::{BackendSelection, PersistenceConfig, SqlitePackLayout, SqlitePersistenceProfile};
pub use handles::Handles;
pub use history::HistoryProvider;
pub(crate) use storage::provider as storage_provider;
pub(crate) use storage::publication;
pub use storage_provider::StorageProvider;

mod metadata;
mod objects;
