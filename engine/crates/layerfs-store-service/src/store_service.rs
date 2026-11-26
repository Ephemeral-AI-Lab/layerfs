//! One opened global Store and the overlay owner its file-changing calls
//! construct through: what a Store call needs, and nothing of a Sandbox.
//! The owner and its template file start at the first file-changing call,
//! so a process that only reads never creates them (#819).
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
use std::path::{Path, PathBuf};
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
/// Each private overlay's page cache, as a daemon's: 4 MiB, four 4 KiB pages
/// per byte of one write window's payload, so a window's transaction does not
/// spill half way through it.
const PAGER_KIB: u32 = 4096;

/// Why a Store could not be opened directly.
#[derive(Debug)]
pub enum OpenRefusal {
    /// The kernel guard refused this setup, before any file was opened.
    Unsupported(Unsupported),
    /// The Store file is absent: no Sandbox installed it into the volume yet.
    Missing(String),
    /// Another writer held the Store at open, before any effect. The call is
    /// a definite refusal the caller may repeat; nothing was changed.
    Busy(String),
    /// The Store is of an older schema version that this release neither
    /// reads nor migrates (#776): the persistence refusal's own status,
    /// which names the version. Nothing was opened or changed.
    Outdated(String),
    /// The Store or the overlay template failed to open.
    Failed(String),
}

/// The refusal for a Store open that failed: a Busy open is a definite
/// pre-effect refusal, a Store of an older schema version is [`Outdated`]
/// with its status, every other open failure is not.
///
/// [`Outdated`]: OpenRefusal::Outdated
fn open_refusal(error: layerfs_storage::StorageError) -> OpenRefusal {
    if let Some(status) = outdated(&error) {
        return OpenRefusal::Outdated(status.to_owned());
    }
    let reason = format!("Store open: {error}");
    match error {
        layerfs_storage::StorageError::Busy => OpenRefusal::Busy(reason),
        _ => OpenRefusal::Failed(reason),
    }
}

/// The status of a persistence refusal of an older-format Store, carried
/// as the storage error's I/O source (`From<PersistenceError>`).
fn outdated(error: &layerfs_storage::StorageError) -> Option<&'static str> {
    let layerfs_storage::StorageError::Io(io) = error else {
        return None;
    };
    match io.get_ref()?.downcast_ref()? {
        layerfs_storage::port::PersistenceError::Refused { status } => {
            layerfs_persistence::outdated_store(status).filter(|known| known == status)
        }
        _ => None,
    }
}

/// The Store and overlay owner of this process's Store calls.
pub struct StoreService {
    pub opened: OpenedStore,
    pub store: Arc<Store>,
    /// The overlay owner, or where the first `bind` starts it.
    overlay: Mutex<Overlay>,
    capacity: usize,
    bound: AtomicUsize,
    retained: Mutex<Vec<Retained>>,
}

/// A service's overlay owner: not started yet (the template it starts
/// over), or started, by the service (`owned`) or by its caller.
enum Overlay {
    Later(PathBuf),
    Started {
        owned: Option<Owner>,
        client: OwnerClient,
    },
}

impl StoreService {
    /// Serves Store calls over `opened`, constructing through `owner`.
    pub fn new(opened: OpenedStore, owner: &Owner) -> Self {
        let overlay = Overlay::Started {
            owned: None,
            client: owner.client(),
        };
        Self::serving(opened, overlay, owner.configuration().namespaces)
    }

    /// Serves Store calls over `opened`; the overlay owner starts over
    /// `template` when the first file-changing call needs it.
    fn later(opened: OpenedStore, template: &Path) -> Self {
        Self::serving(opened, Overlay::Later(template.to_path_buf()), CALLS)
    }

    fn serving(opened: OpenedStore, overlay: Overlay, capacity: usize) -> Self {
        Self {
            store: opened.store.clone(),
            opened,
            overlay: Mutex::new(overlay),
            capacity,
            bound: AtomicUsize::new(0),
            retained: Mutex::new(Vec::new()),
        }
    }

    /// Opens the installed Store at `store` for this process's Store calls.
    /// The kernel guard runs first, before any file is opened; then the
    /// Store, selected Normal explicitly. The overlay template at `template`
    /// (a local file) for the calls' private Workspaces is created by the
    /// first file-changing call, not here. No busy handler, retry or fsync:
    /// one more opener, as a daemon is.
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
            PersistenceConfig::sqlite(store).with_sqlite_profile(SqlitePersistenceProfile::Normal),
            binding,
            cursor_key,
            CALLS,
            CACHE_BYTES,
            ReservationBlocks::default(),
        )
        .map_err(open_refusal)?;
        Ok(Self::later(opened, template))
    }

    /// Opens a PostgreSQL Store (#506) for this process's Store calls, with
    /// the overlay template at `template`. No kernel guard runs: the Store
    /// is no file shared through one kernel but a server every opener
    /// reaches over its own connection.
    pub fn open_postgres(
        selection: &layerfs_persistence::PostgresSelection,
        binding: &[u8],
        cursor_key: [u8; 32],
        template: &Path,
    ) -> Result<Self, OpenRefusal> {
        let opened = open_store_observed(
            selection.clone(),
            binding,
            cursor_key,
            CALLS,
            CACHE_BYTES,
            ReservationBlocks::default(),
        )
        .map_err(open_refusal)?;
        Ok(Self::later(opened, template))
    }

    /// The overlay owner, started over the template by the first call that
    /// needs it. A start that fails fails that call alone; nothing is retried
    /// for it.
    fn owner(&self) -> Result<OwnerClient, String> {
        let mut overlay = self.overlay.lock().unwrap_or_else(|e| e.into_inner());
        let template = match &*overlay {
            Overlay::Started { client, .. } => return Ok(client.clone()),
            Overlay::Later(template) => template,
        };
        let profile = ProfileConfig {
            pager_kib: PAGER_KIB,
            max_pages: None,
        };
        let owned = Owner::start(template, profile, OwnerConfig { namespaces: CALLS })
            .map_err(|error| format!("overlay template: {error}"))?;
        let client = owned.client();
        *overlay = Overlay::Started {
            owned: Some(owned),
            client: client.clone(),
        };
        Ok(client)
    }

    /// Returns the Store's idle producer handle's kept ordinals
    /// ([`Store::release_producer`]), then stops the overlay owner this
    /// service started. Private Workspaces kept with an uncertain Commit stay
    /// with it until then.
    pub fn close(self) -> Result<(), String> {
        self.store.release_producer();
        let overlay = self.overlay.into_inner().unwrap_or_else(|e| e.into_inner());
        match overlay {
            Overlay::Started {
                owned: Some(owner), ..
            } => owner.stop().map_err(|error| error.to_string()),
            _ => Ok(()),
        }
    }

    /// One private Workspace on `branch` for one file-changing call.
    pub fn bind(
        &self,
        workspace: WorkspaceId,
        branch: BranchId,
        timing: &TimingScope<'_, Active>,
    ) -> Result<Arc<BoundWorkspace>, String> {
        let owner = self.owner()?;
        if let Some(stopped) = owner.maintenance_failure().map_err(|e| e.to_string())? {
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
                .bind_private(owner, BindRequest { workspace, branch })
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
        self.owner()?
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
