use layerfs_storage::{Result, StoreDb, StoreId, StoreRole};
use std::path::Path;

#[derive(Clone)]
pub struct LayerStackStore {
    pub(crate) db: StoreDb,
}

impl LayerStackStore {
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            db: StoreDb::create(path, StoreRole::LayerStack, None)?,
        })
    }

    pub fn connect(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            db: StoreDb::connect(path, StoreRole::LayerStack, None)?,
        })
    }

    pub fn path(&self) -> &Path {
        self.db.path()
    }

    pub fn store_id(&self) -> StoreId {
        self.db.store_id()
    }
}
