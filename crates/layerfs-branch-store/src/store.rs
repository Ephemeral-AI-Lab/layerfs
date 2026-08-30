use layerfs_storage::{BranchId, Result, StorageError, StoreDb, StoreId, StoreRole};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct BranchStore {
    pub(crate) db: StoreDb,
    workspace_leases: Arc<Mutex<BTreeSet<BranchId>>>,
}

impl BranchStore {
    pub fn create(path: impl AsRef<Path>, parent_store_id: StoreId) -> Result<Self> {
        Ok(Self {
            db: StoreDb::create(path, StoreRole::Branch, Some(parent_store_id))?,
            workspace_leases: Arc::new(Mutex::new(BTreeSet::new())),
        })
    }

    pub fn connect(path: impl AsRef<Path>, expected_parent_store_id: StoreId) -> Result<Self> {
        Ok(Self {
            db: StoreDb::connect(path, StoreRole::Branch, Some(expected_parent_store_id))?,
            workspace_leases: Arc::new(Mutex::new(BTreeSet::new())),
        })
    }

    pub fn path(&self) -> &Path {
        self.db.path()
    }

    pub fn store_id(&self) -> StoreId {
        self.db.store_id()
    }

    pub fn parent_store_id(&self) -> StoreId {
        self.db.parent_store_id().expect("BranchStore parent")
    }

    #[doc(hidden)]
    pub fn acquire_workspace_lease(&self, branch_id: BranchId) -> Result<bool> {
        self.workspace_leases
            .lock()
            .map_err(|_| StorageError::Integrity("Workspace lease"))
            .map(|mut leases| leases.insert(branch_id))
    }

    #[doc(hidden)]
    pub fn release_workspace_lease(&self, branch_id: BranchId) {
        if let Ok(mut leases) = self.workspace_leases.lock() {
            leases.remove(&branch_id);
        }
    }
}
