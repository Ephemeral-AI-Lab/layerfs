//! Shared SQLite adapter with explicit Durable and disk-backed Disposable profiles.
#![deny(unsafe_code)]
#![deny(missing_docs)]
mod backend;
mod storage;
mod store;
pub(crate) use store::config;
pub(crate) use store::handles;
mod history;
pub use backend::sqlite::connection::{ConnectionProfile, SqlWork};
pub use backend::sqlite::reclamation::{SpaceReclamation, RECLAMATION_PAGE_LIMIT};
pub use backend::sqlite::statement_work::StatementPhaseWork;
pub use backend::sqlite::thread_work::thread_work;
#[cfg(target_os = "windows")]
#[cfg(target_os = "windows")]
pub use backend::sqlite::windows_volume_filesystem;
pub use config::{
    BackendSelection, PersistenceConfig, SqliteAcquisitionSchema, SqlitePackLayout,
    SqlitePersistenceProfile,
};
pub use handles::Handles;

/// The Store directory's writer lane, held for a coordinated close: the
/// connection teardown's close-time checkpoint runs inside it, never beside
/// another opener's first attach. Best-effort - a failed acquisition closes
/// bare, as an uncoordinated drop always has.
pub struct StoreLane(Option<crate::backend::sqlite::writer_admission::Writer>);
impl Drop for StoreLane {
    fn drop(&mut self) {
        if let Some(writer) = self.0.take() {
            let _ = writer.release();
        }
    }
}
/// Holds the writer lane of the Store directory holding `store`.
pub fn store_lane(store: &std::path::Path) -> StoreLane {
    StoreLane(crate::backend::sqlite::writer_admission::Writer::acquire(store).ok())
}
pub use history::HistoryProvider;
pub use storage::acquisition::AcquisitionProvider;
pub use storage::merge_records::{
    MergeRecords, MergeRecordsWork, MergeScratch, BUFFER_BYTES, MERGE_LEASE, RESIDENT_BYTES,
};
pub(crate) use storage::provider as storage_provider;
pub(crate) use storage::publication;
pub use storage_provider::StorageProvider;
pub use store::seal::SealedStore;

mod metadata;
mod objects;
