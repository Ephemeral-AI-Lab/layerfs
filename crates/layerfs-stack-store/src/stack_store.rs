use crate::writer::Writer;
use layerfs_storage_core::{Result, SchemaKind, StoreDb, StoreEndpoint};
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
pub struct StackStore {
    pub(crate) db: StoreDb,
    pub(crate) parent: Arc<dyn StoreEndpoint>,
    pub(crate) writer: Writer,
}

impl StackStore {
    pub fn open(path: impl AsRef<Path>, parent: Arc<dyn StoreEndpoint>) -> Result<Self> {
        let db = StoreDb::open(path, SchemaKind::Full)?;
        let writer = Writer::open(db.path())?;
        Ok(Self { db, parent, writer })
    }

    pub fn path(&self) -> &Path {
        self.db.path()
    }

    pub fn branch(
        &self,
        id: layerfs_storage_core::BranchId,
    ) -> Result<Option<layerfs_storage_core::BranchRecord>> {
        self.db.branch(id)
    }

    pub fn stack_history(
        &self,
        id: layerfs_storage_core::StackHistoryId,
    ) -> Result<Option<layerfs_storage_core::StackHistoryRecord>> {
        self.db.stack_history(id)
    }

    pub fn stack(
        &self,
        id: layerfs_storage_core::StackId,
    ) -> Result<Option<layerfs_storage_core::StackRecord>> {
        self.db.stack(id)
    }
}
