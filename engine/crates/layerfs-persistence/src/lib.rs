//! The global Store: embedded SQLite (explicit Durable and disk-backed
//! Disposable profiles) or PostgreSQL (#506).
#![deny(unsafe_code)]
#![deny(missing_docs)]
mod backend;
pub mod compact_metrics;
pub mod diagnostic;
/// Test-only re-exports for unit checks that live outside product source.
#[doc(hidden)]
pub mod internals;
mod storage;
mod store;
pub(crate) use store::config;
mod history;
mod report;
pub use backend::postgresql::access::{access as postgresql_access, PostgresAccess};
pub use backend::postgresql::status::{
    compact_status as postgresql_compact_status, status as postgresql_status,
    PostgresCompactOpStatus, PostgresStatus, PostgresStatusError,
};
pub use backend::sqlite::acquisition::AcquisitionProvider;
pub use backend::sqlite::compact::{
    compact_op_status, CompactFailpoint, CompactKeptWaste, CompactOpStatus, CompactPath,
    CompactReceipt, CompactRequest, CompactResidue, CompactZeroReason, OwnerLiveness,
    DELTA_BASE_REFERENCES_CAP,
};
pub use backend::sqlite::heads::{heads_digest, read_heads};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use backend::sqlite::lifecycle::headroom::STORE_HEADROOM_BYTES;
pub use backend::sqlite::lifecycle::reclamation::{SpaceReclamation, RECLAMATION_PAGE_LIMIT};
pub use backend::sqlite::session::connection::{ConnectionProfile, SqlWork};
pub use backend::sqlite::session::statement_work::StatementPhaseWork;
pub use backend::sqlite::session::thread_work::{
    profile_statements, take_statement_profile, thread_work, StatementCost,
};
pub use backend::sqlite::storage::merge_records::{
    MergeRecords, MergeRecordsWork, MergeScratch, BUFFER_BYTES, MERGE_LEASE, RESIDENT_BYTES,
};
pub use config::{
    PersistenceConfig, PostgresCheck, PostgresSelection, SqliteAcquisitionSchema, SqlitePackLayout,
    SqlitePersistenceProfile, SynchronousCommit,
};
pub use history::{
    HistoryProvider, KeptHistory, KeptNode, KeptNodeId, KeptObjects, KeptWaste, MarkResult,
    Sequence, Spans,
};
pub use report::{CompactStatus, POSTGRES_COMPACT_PHASES, SQLITE_COMPACT_PHASES};
pub(crate) use storage::provider as storage_provider;
pub(crate) use storage::publication;
pub use storage_provider::StorageProvider;
pub use store::handles::SqliteHandles;
pub use store::postgresql::{
    PostgresAcquisition, PostgresHandles, PostgresLaneHold, PostgresStorage,
};
pub use store::seal::SealedStore;
pub use store::selection::{Handles, StoreFile, StoreSelection};

mod blocks;
mod metadata;
mod objects;
