use layerfs_storage::internal::StoreEndpoint;
use layerfs_storage::{Result, StoreDb, StoreRole};
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
pub struct BranchStore {
    pub(crate) db: StoreDb,
    pub(crate) parent: Arc<dyn StoreEndpoint>,
}

impl BranchStore {
    pub fn create(path: impl AsRef<Path>, parent: Arc<dyn StoreEndpoint>) -> Result<Self> {
        let parent_identity = parent.store_identity()?;
        let db = StoreDb::create(path, StoreRole::Branch)?;
        db.bind_parent(parent_identity, true)?;
        Ok(Self { db, parent })
    }

    pub fn connect(path: impl AsRef<Path>, parent: Arc<dyn StoreEndpoint>) -> Result<Self> {
        let parent_identity = parent.store_identity()?;
        let db = StoreDb::connect(path, StoreRole::Branch)?;
        db.bind_parent(parent_identity, false)?;
        Ok(Self { db, parent })
    }

    pub fn path(&self) -> &Path {
        self.db.path()
    }

    pub fn branch(
        &self,
        id: layerfs_storage::BranchId,
    ) -> Result<Option<layerfs_storage::BranchRecord>> {
        self.db.branch(id)
    }

    #[doc(hidden)]
    pub fn commit_record(
        &self,
        id: layerfs_storage::CommitId,
    ) -> Result<Option<layerfs_storage::CommitRecord>> {
        self.db.commit(id)
    }

    #[doc(hidden)]
    pub fn fact_page(
        &self,
        kind: layerfs_storage::FactKind,
        after: Option<&[u8]>,
        limit: u16,
    ) -> Result<Vec<layerfs_storage::Fact>> {
        self.db.fact_page(kind, after, limit)
    }

    #[doc(hidden)]
    pub fn inventory_page(
        &self,
        after: Option<layerfs_content::ObjectId>,
        limit: u16,
    ) -> Result<layerfs_storage::InventoryPage> {
        self.db.inventory_page(after, limit)
    }

    #[doc(hidden)]
    pub fn storage_snapshot(&self) -> Result<layerfs_storage::StoreStorageSnapshot> {
        self.db.storage_snapshot()
    }
}
