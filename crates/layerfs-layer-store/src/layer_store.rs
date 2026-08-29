use layerfs_storage::{Result, StoreDb, StoreRole};
use std::path::Path;

#[derive(Clone)]
pub struct LayerStore {
    pub(crate) db: StoreDb,
}

impl LayerStore {
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            db: StoreDb::create(path, StoreRole::Layer)?,
        })
    }

    pub fn connect(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            db: StoreDb::connect(path, StoreRole::Layer)?,
        })
    }

    pub fn path(&self) -> &Path {
        self.db.path()
    }

    pub fn layer_history(
        &self,
        id: layerfs_storage::LayerHistoryId,
    ) -> Result<Option<layerfs_storage::LayerHistoryRecord>> {
        self.db.layer_history(id)
    }

    pub fn layer(
        &self,
        id: layerfs_storage::LayerId,
    ) -> Result<Option<layerfs_storage::LayerRecord>> {
        self.db.layer(id)
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
