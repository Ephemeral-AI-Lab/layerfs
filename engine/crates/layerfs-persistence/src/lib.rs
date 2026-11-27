//! The global Store: embedded SQLite (the Normal profile; Durable is
//! disabled by the owner) or PostgreSQL (#506).
#![deny(unsafe_code)]
#![deny(missing_docs)]
mod backend;
pub(crate) mod config;
pub mod diagnostic;
/// Test-only re-exports for unit checks that live outside product source.
#[doc(hidden)]
pub mod internals;
mod storage;
pub(crate) use config::postgresql_options as config;
pub(crate) mod acquisition;
pub mod gc;
mod history;
pub mod upgrade;
pub use backend::error::{outdated_store, upgradable_store};
pub use backend::postgresql::server::access::{access as postgresql_access, PostgresAccess};
pub use backend::postgresql::server::status::{
    compact_status as postgresql_compact_status, status as postgresql_status,
    PostgresCompactOpStatus, PostgresStatus, PostgresStatusError,
};
pub use backend::postgresql::session::connection::HANDSHAKE as POSTGRESQL_HANDSHAKE;
pub use backend::postgresql::session::reopen::one_call;
pub use backend::sqlite::acquisition::AcquisitionProvider;
pub use backend::sqlite::compact::{
    compact_op_status, CompactFailpoint, CompactKeptWaste, CompactOpStatus, CompactPath,
    CompactReceipt, CompactRequest, CompactResidue, CompactZeroReason, OwnerLiveness,
    DELTA_BASE_REFERENCES_CAP,
};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use backend::sqlite::lifecycle::headroom::STORE_HEADROOM_BYTES;
pub use backend::sqlite::lifecycle::heads::{heads_digest, read_heads};
pub use backend::sqlite::lifecycle::reclamation::{SpaceReclamation, RECLAMATION_PAGE_LIMIT};
pub use backend::sqlite::merge_records::{
    MergeRecords, MergeRecordsWork, MergeScratch, BUFFER_BYTES, MERGE_LEASE, RESIDENT_BYTES,
};
pub(crate) use backend::sqlite::objects::provider as storage_provider;
pub use backend::sqlite::session::connection::{ConnectionProfile, SqlWork};
pub use backend::sqlite::session::statement_work::StatementPhaseWork;
pub use backend::sqlite::session::thread_work::{
    profile_statements, take_statement_profile, thread_work, StatementCost,
};
pub use config::postgresql::{
    PostgresAcquisition, PostgresHandles, PostgresLaneHold, PostgresStorage,
};
pub use config::selection::{Handles, StoreFile, StoreSelection};
pub use config::sqlite::SealedStore;
pub use config::sqlite::SqliteHandles;
pub use config::sqlite::{DeferredSqliteWriter, SqliteWriter};
pub use config::{
    PersistenceConfig, PostgresCheck, PostgresSelection, SqliteAcquisitionSchema, SqlitePackLayout,
    SqlitePersistenceProfile, SynchronousCommit, READER_CHECK_TIMEOUT, READER_IDLE_CHECK_MS,
};
pub use gc::admission::intents::{has_place_view, with_place_view};
pub use gc::compact::{CompactStatus, POSTGRES_COMPACT_PHASES, SQLITE_COMPACT_PHASES};
pub use history::{
    Committer, Fused, HistoryProvider, KeptHistory, KeptNode, KeptNodeId, KeptObjects, KeptWaste,
    MarkResult, Sequence, Spans, FIXED_ROWS as FUSED_FIXED_ROWS, HINT_BATCH_ROWS,
};
/// The PostgreSQL client's own reading of a connection URL (its `FromStr`
/// is tokio-postgres's `UrlParser`), the one every Store session dials
/// with: control reads a Store's URL with it, never with a parser of its own.
pub use postgres::config::{Config as PostgresConfig, Host as PostgresHost};
pub(crate) use storage::publication;
pub use storage_provider::StorageProvider;

// #828 compat: old public module paths, removed in wave 3.
#[doc(hidden)]
pub use crate::gc::metrics as compact_metrics; // #828 compat
