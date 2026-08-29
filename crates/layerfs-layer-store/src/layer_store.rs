use layerfs_storage_core::{Result, SchemaKind, StoreDb};
use std::path::Path;

#[derive(Clone)]
pub struct LayerStore {
    pub(crate) db: StoreDb,
}

impl LayerStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            db: StoreDb::open(path, SchemaKind::Full)?,
        })
    }

    pub fn path(&self) -> &Path {
        self.db.path()
    }

    pub fn layer_history(
        &self,
        id: layerfs_storage_core::LayerHistoryId,
    ) -> Result<Option<layerfs_storage_core::LayerHistoryRecord>> {
        self.db.layer_history(id)
    }

    pub fn layer(
        &self,
        id: layerfs_storage_core::LayerId,
    ) -> Result<Option<layerfs_storage_core::LayerRecord>> {
        self.db.layer(id)
    }
}
