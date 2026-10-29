//! The global Store: embedded SQLite (explicit Durable and disk-backed
//! Disposable profiles) or, with the `postgresql` feature, PostgreSQL (#506).
#![deny(unsafe_code)]
#![deny(missing_docs)]
mod backend;
pub mod remote;
mod storage;
mod store;
pub(crate) use store::config;
pub(crate) use store::handles;
mod history;
#[cfg(feature = "postgresql")]
pub use backend::postgresql::status::{
    status as postgresql_status, PostgresStatus, PostgresStatusError,
};
pub use backend::sqlite::acquisition::AcquisitionProvider;
pub use backend::sqlite::heads::read_heads;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use backend::sqlite::lifecycle::headroom::STORE_HEADROOM_BYTES;
pub use backend::sqlite::lifecycle::reclamation::{SpaceReclamation, RECLAMATION_PAGE_LIMIT};
pub use backend::sqlite::session::connection::{ConnectionProfile, SqlWork};
pub use backend::sqlite::session::statement_work::StatementPhaseWork;
pub use backend::sqlite::session::thread_work::thread_work;
pub use backend::sqlite::storage::merge_records::{
    MergeRecords, MergeRecordsWork, MergeScratch, BUFFER_BYTES, MERGE_LEASE, RESIDENT_BYTES,
};
pub use config::{
    BackendSelection, PersistenceConfig, PostgresSelection, SqliteAcquisitionSchema,
    SqlitePackLayout, SqlitePersistenceProfile, SynchronousCommit,
};
pub use handles::Handles;
pub use history::HistoryProvider;
pub(crate) use storage::provider as storage_provider;
pub(crate) use storage::publication;
pub use storage_provider::StorageProvider;
#[cfg(feature = "postgresql")]
pub use store::postgresql::{ImportReceipt, PostgresHandles, PostgresLaneHold, PostgresStorage};
pub use store::seal::SealedStore;

mod metadata;
mod objects;
