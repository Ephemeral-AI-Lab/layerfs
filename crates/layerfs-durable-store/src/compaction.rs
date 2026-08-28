use crate::{DurableStore, Result};

impl DurableStore {
    pub fn compact(self) -> Result<Self> {
        let Self { root, storage } = self;
        let generation_root = root.join("durable.sqlite.generations");
        let storage = layerfs_storage::generation::compact_full_durable(
            storage,
            &generation_root,
            &layerfs_storage::generation::NativeGenerationDriver,
        )?;
        Ok(Self { root, storage })
    }
}
