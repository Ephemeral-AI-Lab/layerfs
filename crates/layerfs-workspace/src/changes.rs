use crate::model::{Data, FileData, Workspace};
use layerfs_storage_core::internal::StagedChange;
use layerfs_storage_core::{Result, StorageError};

impl Workspace {
    pub(crate) fn build_mutations(&self) -> Result<Vec<StagedChange>> {
        let mut staged = self.mutations.clone();
        for node_id in &self.dirty {
            let Some(node) = self.nodes.get(node_id) else {
                continue;
            };
            let Some(path) = node.paths.first() else {
                continue;
            };
            let Data::File(FileData::Overlay {
                base,
                spool,
                len,
                dirty,
                ..
            }) = &node.data
            else {
                return Err(StorageError::Integrity("dirty file"));
            };
            let base_len = base.map_or(0, |(_, len)| len);
            for (&start, &end) in dirty {
                let end = end.min(*len);
                if start < end {
                    staged.push(StagedChange::SpliceFile {
                        path: path.clone(),
                        start,
                        delete_len: end.min(base_len).saturating_sub(start.min(base_len)),
                        spool: spool.clone(),
                        spool_offset: start,
                        replacement_len: end - start,
                    });
                }
            }
            if *len < base_len {
                staged.push(StagedChange::SpliceFile {
                    path: path.clone(),
                    start: *len,
                    delete_len: base_len - *len,
                    spool: spool.clone(),
                    spool_offset: *len,
                    replacement_len: 0,
                });
            }
        }
        Ok(staged)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = self.clear_spool();
    }
}
