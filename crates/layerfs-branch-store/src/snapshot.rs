use crate::BranchStore;
use layerfs_core::{logical, CanonicalPath, ObjectId};
use layerfs_storage_core::{CoreReader, Result, StorageError};

impl BranchStore {
    #[doc(hidden)]
    pub fn branch_snapshot(
        &self,
        branch_id: layerfs_storage_core::BranchId,
    ) -> Result<(layerfs_storage_core::BranchRecord, ObjectId)> {
        let _operation = self.db.enter_operation()?;
        let branch = self
            .db
            .branch(branch_id)?
            .ok_or(StorageError::NotFound("Branch"))?;
        let root = self
            .db
            .commit(branch.head_commit_id)?
            .ok_or(StorageError::MissingBaseData)?
            .root_id;
        Ok((branch, root))
    }

    pub fn root(&self, branch_id: layerfs_storage_core::BranchId) -> Result<ObjectId> {
        Ok(self.branch_snapshot(branch_id)?.1)
    }

    pub fn read_path(
        &self,
        branch_id: layerfs_storage_core::BranchId,
        path: &str,
    ) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        logical::stream(
            &CoreReader(self),
            self.root(branch_id)?,
            &CanonicalPath::new(path)?,
            &mut bytes,
        )?;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use layerfs_layer_store::LayerStore;
    use layerfs_storage_core::{Change, RefOutcome};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Barrier};

    #[test]
    fn branch_snapshot_never_mixes_a_head_with_another_heads_root() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-branch-snapshot-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let layer = Arc::new(LayerStore::open(root.join("layer.sqlite")).unwrap());
        let (history, genesis) = layer.provision().unwrap();
        let store = BranchStore::open(root.join("branch.sqlite"), layer.clone()).unwrap();
        let branch = store
            .create_branch_from_layer(history.id, genesis.id)
            .unwrap();
        let start = Arc::new(Barrier::new(2));
        let done = Arc::new(AtomicBool::new(false));
        let writer = {
            let store = store.clone();
            let start = start.clone();
            let done = done.clone();
            std::thread::spawn(move || {
                let mut head = branch.head_commit_id;
                start.wait();
                for byte in 0..32_u8 {
                    let RefOutcome::Created(next) = store
                        .commit(
                            branch.id,
                            head,
                            &[Change::Write {
                                path: "value".into(),
                                bytes: vec![byte; 1024],
                                mode: 0o600,
                            }],
                        )
                        .unwrap()
                    else {
                        panic!("expected commit")
                    };
                    head = next;
                }
                done.store(true, Ordering::Release);
            })
        };
        start.wait();
        let mut observations = 0;
        while !done.load(Ordering::Acquire) || observations < 32 {
            let (record, observed_root) = store.branch_snapshot(branch.id).unwrap();
            assert_eq!(
                store
                    .db
                    .commit(record.head_commit_id)
                    .unwrap()
                    .unwrap()
                    .root_id,
                observed_root
            );
            observations += 1;
        }
        writer.join().unwrap();
        drop(store);
        drop(layer);
        std::fs::remove_dir_all(root).unwrap();
    }
}
