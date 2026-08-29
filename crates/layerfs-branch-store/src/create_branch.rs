use crate::BranchStore;
use layerfs_storage::{
    BaseId, BranchId, BranchRecord, BranchSource, CommitId, CommitRecord, Result, StorageError,
};

impl BranchStore {
    pub fn create_branch(&self, source: BranchSource) -> Result<BranchRecord> {
        match source {
            BranchSource::Layer(id) => self.create_from_base(BaseId::Layer(id)),
            BranchSource::Stack(id) => self.create_from_base(BaseId::Stack(id)),
            BranchSource::Commit(source) => {
                self.create_from_commit(source.branch_id, source.commit_id)
            }
        }
    }

    fn create_from_commit(
        &self,
        source_branch_id: layerfs_storage::BranchId,
        source_commit_id: CommitId,
    ) -> Result<BranchRecord> {
        let _operation = self.db.enter_operation()?;
        let source = self
            .db
            .branch(source_branch_id)?
            .ok_or(StorageError::NotFound("source Branch"))?;
        if !self
            .db
            .is_commit_ancestor(source_commit_id, source.head_commit_id)?
        {
            return Err(StorageError::NotFound("reachable Commit"));
        }
        let branch = BranchRecord {
            id: BranchId::new(),
            head_commit_id: source_commit_id,
            base_id: source.base_id,
        };
        self.db.create_subbranch(branch)?;
        Ok(branch)
    }

    fn create_from_base(&self, base_id: BaseId) -> Result<BranchRecord> {
        let _operation = self.db.enter_operation()?;
        let base = self.parent.base_snapshot(base_id)?;
        layerfs_content::filesystem::namespace(&layerfs_storage::CoreReader(self), base.root_id)?;
        let anchor = CommitRecord {
            id: CommitId::derive(base.root_id, None, None),
            root_id: base.root_id,
            parent_id: None,
            merge_parent_id: None,
        };
        let branch = BranchRecord {
            id: BranchId::new(),
            head_commit_id: anchor.id,
            base_id,
        };
        self.db.create_branch(branch, anchor)?;
        Ok(branch)
    }
}
