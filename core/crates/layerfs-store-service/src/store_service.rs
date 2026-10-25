//! One opened global Store and the overlay owner its file-changing calls
//! construct through: what a Store call needs, and nothing of a Sandbox.
//!
//! A file-changing call (`fs.write`, `fs.apply`, ...) binds one private
//! Workspace, changes it, commits it once and closes it. The binding is this
//! call's alone: it is never listed, mounted or shared. A Commit whose
//! outcome is uncertain keeps its Workspace here, with the original failure,
//! instead of closing it: the call is answered `Unknown` and nothing is
//! resent or discarded.
use crate::bootstrap::{open_store_observed, OpenedStore};
use crate::kernel_guard::{self, Unsupported};
use crate::store::{BindRequest, BoundWorkspace, CommitFailure, Store};
use crate::{Owner, OwnerClient, OwnerConfig};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_history::{BranchId, WorkspaceId};
use layerfs_overlay::ProfileConfig;
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::ReservationBlocks;
use layerfs_telemetry::timer::{Active, TimingScope};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// A private Workspace whose Commit outcome is uncertain, kept with its cause.
pub struct Retained {
    pub workspace: Arc<BoundWorkspace>,
    pub failure: Box<CommitFailure>,
}

/// Private Workspaces one process's Store calls may hold at once: one per
/// libuv worker thread of the SDK binding.
pub const CALLS: usize = 64;
/// The Store's shared immutable-object cache, as a daemon's.
const CACHE_BYTES: usize = 64 * 1024;
/// Each private overlay's page cache, as a daemon's.
const PAGER_KIB: u32 = 1024;

/// Why a Store could not be opened directly.
#[derive(Debug)]
pub enum OpenRefusal {
    /// The kernel guard refused this setup, before any file was opened.
    Unsupported(Unsupported),
    /// The Store file is absent: no Sandbox installed it into the volume yet.
    Missing(String),
    /// The Store or the overlay template failed to open.
    Failed(String),
}

/// The Store and overlay owner of this process's Store calls.
pub struct StoreService {
    pub opened: OpenedStore,
    pub store: Arc<Store>,
    owned: Option<Owner>,
    owner: OwnerClient,
    capacity: usize,
    bound: AtomicUsize,
    retained: Mutex<Vec<Retained>>,
    /// The Store file whose writer lane guards this service's close.
    store_file: std::path::PathBuf,
}

impl StoreService {
    /// Serves Store calls over `opened`, constructing through `owner`.
    pub fn new(opened: OpenedStore, owner: &Owner) -> Self {
        Self::at(opened, owner, std::path::PathBuf::new())
    }
    /// [`new`](Self::new) with the Store file recorded, for a close that
    /// coordinates its sessions' teardown under the writer lane.
    pub fn at(opened: OpenedStore, owner: &Owner, store_file: std::path::PathBuf) -> Self {
        Self {
            store: opened.store.clone(),
            opened,
            owned: None,
            owner: owner.client(),
            capacity: owner.configuration().namespaces,
            bound: AtomicUsize::new(0),
            retained: Mutex::new(Vec::new()),
            store_file,
        }
    }

    /// Opens the installed Store at `store` for this process's Store calls.
    /// The kernel guard runs first, before any file is opened; then the
    /// Store, selected Disposable explicitly, and the overlay template at
    /// `template` (a local file, created here) for the calls' private
    /// Workspaces. No busy handler, retry or fsync: one more opener, as a
    /// daemon is.
    pub fn open_direct(
        store: &Path,
        binding: &[u8],
        cursor_key: [u8; 32],
        template: &Path,
    ) -> Result<Self, OpenRefusal> {
        kernel_guard::check(store).map_err(OpenRefusal::Unsupported)?;
        if !store.is_file() {
            return Err(OpenRefusal::Missing(format!(
                "no installed Store at {}",
                store.display()
            )));
        }
        let opened = open_store_observed(
            PersistenceConfig::sqlite(store)
                .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
            binding,
            cursor_key,
            CALLS,
            CACHE_BYTES,
            ReservationBlocks::default(),
        )
        .map_err(|error| OpenRefusal::Failed(format!("Store open: {error}")))?;
        let owner = Owner::start(
            template,
            ProfileConfig {
                pager_kib: PAGER_KIB,
                max_pages: None,
            },
            OwnerConfig { namespaces: CALLS },
        )
        .map_err(|error| OpenRefusal::Failed(format!("overlay template: {error}")))?;
        let mut service = Self::at(opened, &owner, store.to_path_buf());
        service.owned = Some(owner);
        Ok(service)
    }

    /// Stops the overlay owner this service started. Private Workspaces kept
    /// with an uncertain Commit stay with it until then.
    pub fn close(mut self) -> Result<(), String> {
        // The overlays stop first: their final Commits take the writer lane
        // themselves, so no outer hold may surround this.
        let stopped = match self.owned.take() {
            Some(owner) => owner.stop().map_err(|error| error.to_string()),
            None => Ok(()),
        };
        // The sessions then close under the Store's writer lane, so their
        // close-time checkpoints never run beside another opener's first
        // attach. A failed lane acquisition closes bare, as a drop does.
        let lane = (!self.store_file.as_os_str().is_empty())
            .then(|| layerfs_persistence::store_lane(&self.store_file));
        drop(self);
        drop(lane);
        stopped
    }

    /// One private Workspace on `branch` for one file-changing call.
    pub fn bind(
        &self,
        workspace: WorkspaceId,
        branch: BranchId,
        timing: &TimingScope<'_, Active>,
    ) -> Result<Arc<BoundWorkspace>, String> {
        if let Some(stopped) = self
            .owner
            .maintenance_failure()
            .map_err(|e| e.to_string())?
        {
            return Err(format!("overlay maintenance stopped: {stopped}"));
        }
        let admitted = self
            .bound
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < self.capacity).then_some(n + 1)
            });
        if admitted.is_err() {
            return Err("Capacity: Store call Workspace capacity".to_owned());
        }
        let bound = crate::telemetry::phase(timing, "store.bind", || {
            self.store
                .bind_private(self.owner.clone(), BindRequest { workspace, branch })
        });
        match bound {
            Ok(success) => Ok(Arc::new(success.workspace)),
            Err(refusal) => {
                self.bound.fetch_sub(1, Ordering::AcqRel);
                Err(refusal.to_string())
            }
        }
    }

    /// Closes a private Workspace and retires its overlay namespace.
    pub fn release(&self, workspace: &BoundWorkspace) -> Result<(), String> {
        workspace.close().map_err(|e| format!("{e:?}"))?;
        self.bound.fetch_sub(1, Ordering::AcqRel);
        self.owner
            .retire_namespace(workspace.route().namespace())
            .map_err(|e| e.to_string())
    }

    /// Keeps a private Workspace whose Commit outcome is uncertain, with its
    /// failure, and answers the call `Unknown`.
    pub fn retain(
        &self,
        workspace: Arc<BoundWorkspace>,
        failure: Box<CommitFailure>,
    ) -> SdkRefusal {
        let refusal = SdkRefusal {
            code: "Unknown".to_owned(),
            message: failure.to_string(),
            detail: SdkValue::map([
                (
                    "workspace",
                    SdkValue::text(crate::history::lookup::hex(
                        &workspace.identity().to_bytes(),
                    )),
                ),
                ("namespace", SdkValue::Int(workspace.route().namespace())),
            ]),
        };
        self.retained
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(Retained { workspace, failure });
        refusal
    }

    /// How many private Workspaces were kept with an uncertain Commit.
    pub fn retained(&self) -> usize {
        self.retained
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }
}
