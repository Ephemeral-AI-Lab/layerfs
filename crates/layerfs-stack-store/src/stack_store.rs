use crate::writer::Writer;
use layerfs_storage::{Result, StoreDb, StoreEndpoint, StoreRole};
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
pub struct StackStore {
    pub(crate) db: StoreDb,
    pub(crate) parent: Arc<dyn StoreEndpoint>,
    pub(crate) writer: Writer,
}

impl StackStore {
    pub fn create(path: impl AsRef<Path>, parent: Arc<dyn StoreEndpoint>) -> Result<Self> {
        let parent_identity = parent.store_identity()?;
        let db = StoreDb::create(path, StoreRole::Stack)?;
        db.bind_parent(parent_identity, true)?;
        let writer = Writer::create(db.path())?;
        Ok(Self { db, parent, writer })
    }

    pub fn connect(path: impl AsRef<Path>, parent: Arc<dyn StoreEndpoint>) -> Result<Self> {
        let parent_identity = parent.store_identity()?;
        let db = StoreDb::connect(path, StoreRole::Stack)?;
        db.bind_parent(parent_identity, false)?;
        let writer = Writer::connect(db.path())?;
        Ok(Self { db, parent, writer })
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

    pub fn stack_history(
        &self,
        id: layerfs_storage::StackHistoryId,
    ) -> Result<Option<layerfs_storage::StackHistoryRecord>> {
        self.db.stack_history(id)
    }

    pub fn stack(
        &self,
        id: layerfs_storage::StackId,
    ) -> Result<Option<layerfs_storage::StackRecord>> {
        self.db.stack(id)
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
}
