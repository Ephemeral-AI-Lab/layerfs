use layerfs_storage_core::internal::StoreEndpoint;
use layerfs_storage_core::{Result, SchemaKind, StoreDb};
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
pub struct BranchStore {
    pub(crate) db: StoreDb,
    pub(crate) parent: Arc<dyn StoreEndpoint>,
}

impl BranchStore {
    pub fn open(path: impl AsRef<Path>, parent: Arc<dyn StoreEndpoint>) -> Result<Self> {
        Ok(Self {
            db: StoreDb::open(path, SchemaKind::Branch)?,
            parent,
        })
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
}
