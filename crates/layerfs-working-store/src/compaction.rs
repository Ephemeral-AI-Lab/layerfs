use crate::{Result, WorkingStore};

impl WorkingStore {
    pub fn compact(self) -> Result<Self> {
        let Self { root, storage } = self;
        let generation_root = root.join("working.sqlite.generations");
        let storage = layerfs_storage::generation::compact(
            storage,
            &generation_root,
            &layerfs_storage::generation::NativeGenerationDriver,
        )?;
        Ok(Self { root, storage })
    }
}
