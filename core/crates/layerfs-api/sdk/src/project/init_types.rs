//! Explicit Init inputs and original handoff/failure custody.
use layerfs_bridge::provision::StoreManifest;
use layerfs_history::{
    BranchId, BranchSnapshot, HistoryCatalogConfig, HistoryError, HistoryName, LayerStackId,
};
use layerfs_persistence::{PersistenceConfig, SealedStore};
use layerfs_project::{Initialized, ProjectError};
use layerfs_storage::{port::PersistenceError, StorageError, StoragePolicy};
use std::{fmt, path::PathBuf, time::Instant};

/// One native Project Init. The Store is created once with acquisition tables.
#[derive(Debug)]
pub struct InitRequest {
    /// Complete source directory, with no Git-ignore filtering.
    pub source: PathBuf,
    /// New host output, explicit profile and pack layout; must not already exist.
    pub store: PersistenceConfig,
    /// Store placement inside the daemon's filesystem after installation.
    pub locator: String,
    /// Frozen content/storage policy for this Store.
    pub policy: StoragePolicy,
    /// Original catalog authority and cursor key.
    pub catalog: HistoryCatalogConfig,
    /// New LayerStack identity.
    pub stack: LayerStackId,
    /// Checked LayerStack name.
    pub stack_name: HistoryName,
    /// New first Branch identity.
    pub branch: BranchId,
    /// Checked first Branch name.
    pub branch_name: HistoryName,
    /// Filesystem inode allocation scope seed.
    pub scope_seed: [u8; 32],
    /// Explicit Init deadline; it does not apply to future Exec commands.
    pub deadline: Instant,
}
/// Closed host output. It owns no database session, Reader, Save or service.
#[derive(Debug)]
pub struct SealedProject {
    /// One checked closed file ready for bounded streaming.
    pub store: SealedStore,
    /// Installation metadata; the daemon version is absent until installation.
    pub manifest: StoreManifest,
    /// Original acknowledged import and its count diagnostics.
    pub initialized: Initialized,
    /// Original acknowledged first Branch.
    pub branch: BranchSnapshot,
}
/// Original failure, without cleanup, retry or an inferred history result.
#[derive(Debug)]
pub struct InitFailure {
    /// Exact inputs needed to identify retained output and authority.
    pub request: InitRequest,
    /// Deciding failure in its original typed carrier.
    pub error: InitError,
    /// Original acknowledged Init, if it completed before the failure.
    pub initialized: Option<Initialized>,
    /// Original acknowledged fork, if it completed before seal failed.
    pub branch: Option<BranchSnapshot>,
}
/// The original failing boundary of one Init-and-seal operation.
#[derive(Debug)]
pub enum InitError {
    /// Before-effect provisioning metadata refusal.
    Manifest(&'static str),
    /// Store creation or final seal.
    Persistence(PersistenceError),
    /// Storage construction.
    Storage(StorageError),
    /// Complete native import, including original acquisition custody.
    Project(ProjectError),
    /// First Branch publication.
    History(HistoryError),
}
impl fmt::Display for InitFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Project Init: {:?}", self.error)
    }
}
impl std::error::Error for InitFailure {}
