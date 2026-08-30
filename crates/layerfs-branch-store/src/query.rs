use crate::BranchStore;
use layerfs_content::{filesystem as logical, ObjectId};
use layerfs_storage::{
    BranchFact, BranchId, BranchRecord, BranchScope, BranchScopePage, CommitId, CommitRecord, Fact,
    FactKind, InventoryPage, LayerId, LayerRecord, LayerStackEndpoint, LayerStackId,
    LayerStackScopePage, Result, StorageError, StoreStorageSnapshot,
};
use std::sync::Arc;

impl BranchStore {
    #[doc(hidden)]
    pub fn require_local_branch(&self, branch_id: BranchId) -> Result<BranchRecord> {
        let branch = self
            .db
            .branch(branch_id)?
            .ok_or(StorageError::NotFound("Branch"))?;
        match self.db.branch_scope(branch_id)?.map(|record| record.scope) {
            Some(BranchScope::Local) => Ok(branch),
            Some(BranchScope::Remote { .. }) => Err(StorageError::ReadOnlyBranch(branch_id)),
            None => Err(StorageError::Integrity("visible Branch scope")),
        }
    }

    pub fn branch(&self, id: BranchId) -> Result<Option<BranchRecord>> {
        self.db.branch(id)
    }

    #[doc(hidden)]
    pub fn branch_fact(&self, id: BranchId) -> Result<Option<BranchFact>> {
        self.db.branch_fact(id)
    }

    #[doc(hidden)]
    pub fn layer_stack_fact(
        &self,
        id: LayerStackId,
    ) -> Result<Option<layerfs_storage::LayerStackFact>> {
        self.db.layer_stack_fact(id)
    }

    pub fn commit(&self, id: CommitId) -> Result<Option<CommitRecord>> {
        self.db.commit(id)
    }

    pub fn layer(&self, id: LayerId) -> Result<Option<LayerRecord>> {
        match self.visible_layer_scope(id) {
            Ok((layer, _)) => Ok(Some(layer)),
            Err(StorageError::NotFound(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn root_complete(&self, root: ObjectId) -> Result<bool> {
        self.db.complete_root(root)
    }

    pub fn fact_page(
        &self,
        kind: FactKind,
        after: Option<&[u8]>,
        limit: u16,
    ) -> Result<(Vec<Fact>, Option<Vec<u8>>)> {
        self.db.fact_page(kind, after, limit)
    }

    pub fn inventory_page(&self, after: Option<ObjectId>, limit: u16) -> Result<InventoryPage> {
        self.db.inventory_page(after, limit)
    }

    pub fn storage_snapshot(&self) -> Result<StoreStorageSnapshot> {
        self.db.storage_snapshot()
    }

    pub fn layer_stack_scope_page(
        &self,
        after: Option<LayerStackId>,
        limit: u16,
    ) -> Result<LayerStackScopePage> {
        self.db.layer_stack_scope_page(after, limit)
    }

    pub fn branch_scope_page(
        &self,
        layer_stack_id: Option<LayerStackId>,
        after: Option<BranchId>,
        limit: u16,
    ) -> Result<BranchScopePage> {
        self.db.branch_scope_page(layer_stack_id, after, limit)
    }

    pub fn branch_root(&self, id: BranchId) -> Result<ObjectId> {
        let branch = self
            .db
            .branch(id)?
            .ok_or(StorageError::NotFound("Branch"))?;
        self.db.branch_effective_root(&branch)
    }

    pub fn branch_contains_commit(&self, branch_id: BranchId, commit_id: CommitId) -> Result<bool> {
        let branch = self
            .db
            .branch(branch_id)?
            .ok_or(StorageError::NotFound("Branch"))?;
        self.full_history_contains(&branch, commit_id)
    }

    pub(crate) fn full_history_contains(
        &self,
        branch: &BranchRecord,
        commit_id: CommitId,
    ) -> Result<bool> {
        let mut cursor = branch.head_commit_id;
        let mut cycle = branch.head_commit_id;
        while let Some(current) = cursor {
            if current == commit_id {
                return Ok(true);
            }
            cursor = self.checked_commit_parent(branch.layer_stack_id, current)?;
            cycle = cycle
                .map(|id| self.checked_commit_parent(branch.layer_stack_id, id))
                .transpose()?
                .flatten()
                .map(|id| self.checked_commit_parent(branch.layer_stack_id, id))
                .transpose()?
                .flatten();
            if cursor.is_some() && cursor == cycle {
                return Err(StorageError::Integrity("Commit ancestry cycle"));
            }
        }
        Ok(false)
    }

    fn checked_commit_parent(
        &self,
        layer_stack_id: layerfs_storage::LayerStackId,
        commit_id: CommitId,
    ) -> Result<Option<CommitId>> {
        let commit = self
            .db
            .commit(commit_id)?
            .ok_or(StorageError::Integrity("Commit ancestry"))?;
        let base = self
            .db
            .layer(commit.base_layer_id)?
            .ok_or(StorageError::Integrity("Commit base Layer"))?;
        if base.layer_stack_id != layer_stack_id {
            return Err(StorageError::Integrity("Branch LayerStack ownership"));
        }
        Ok(commit.parent_commit_id)
    }

    pub fn visit_branch_commit_diff(
        &self,
        parent: Arc<dyn LayerStackEndpoint>,
        branch_id: BranchId,
        from: CommitId,
        to: CommitId,
        visitor: impl FnMut(logical::DiffEntry) -> Result<()>,
    ) -> Result<()> {
        let pinned = self
            .db
            .pin_branch(branch_id)?
            .ok_or(StorageError::NotFound("Branch"))?;
        let scope = pinned.scope.scope;
        let branch = pinned.branch;
        if !self.full_history_contains(&branch, from)?
            || !self.full_history_contains(&branch, to)?
        {
            return Err(StorageError::NotFound("Commit in Branch history"));
        }
        let left = self
            .db
            .commit(from)?
            .ok_or(StorageError::Integrity("Commit"))?;
        let right = self
            .db
            .commit(to)?
            .ok_or(StorageError::Integrity("Commit"))?;
        let left_local = self.scope_requires_local(scope, left.root_id)?;
        let right_local = self.scope_requires_local(scope, right.root_id)?;
        self.visit_root_diff(
            parent,
            left.root_id,
            left_local,
            right.root_id,
            right_local,
            visitor,
        )
    }

    pub fn visit_branch_layer_diff(
        &self,
        parent: Arc<dyn LayerStackEndpoint>,
        branch_id: BranchId,
        layer_id: LayerId,
        visitor: impl FnMut(logical::DiffEntry) -> Result<()>,
    ) -> Result<()> {
        let _operation = self.db.enter_operation()?;
        let pinned = self
            .db
            .pin_branch(branch_id)?
            .ok_or(StorageError::NotFound("Branch"))?;
        let scope = pinned.scope.scope;
        let root = pinned.root_id;
        let branch = pinned.branch;
        let (layer, _) = self.visible_layer_scope(layer_id)?;
        if layer.layer_stack_id != branch.layer_stack_id {
            return Err(StorageError::InvalidInput("LayerStack mismatch"));
        }
        let left_local = self.layer_requires_local(layer.id, layer.root_id)?;
        let right_local = self.scope_requires_local(scope, root)?;
        self.visit_root_diff(
            parent,
            layer.root_id,
            left_local,
            root,
            right_local,
            visitor,
        )
    }

    pub fn visit_layer_diff(
        &self,
        parent: Arc<dyn LayerStackEndpoint>,
        from_layer_id: LayerId,
        to_layer_id: LayerId,
        visitor: impl FnMut(logical::DiffEntry) -> Result<()>,
    ) -> Result<()> {
        let _operation = self.db.enter_operation()?;
        let (from, _) = self.visible_layer_scope(from_layer_id)?;
        let (to, _) = self.visible_layer_scope(to_layer_id)?;
        if from.layer_stack_id != to.layer_stack_id {
            return Err(StorageError::InvalidInput("LayerStack mismatch"));
        }
        let from_local = self.layer_requires_local(from.id, from.root_id)?;
        let to_local = self.layer_requires_local(to.id, to.root_id)?;
        self.visit_root_diff(
            parent,
            from.root_id,
            from_local,
            to.root_id,
            to_local,
            visitor,
        )
    }

    fn visit_root_diff(
        &self,
        parent: Arc<dyn LayerStackEndpoint>,
        left: ObjectId,
        left_local: bool,
        right: ObjectId,
        right_local: bool,
        mut visitor: impl FnMut(logical::DiffEntry) -> Result<()>,
    ) -> Result<()> {
        let reader = self.pair_reader_with_policy(parent, left, left_local, right, right_local)?;
        logical::diff_roots(
            &layerfs_storage::CoreReader(&reader),
            left,
            right,
            |entry| visitor(entry).map_err(|_| layerfs_content::CoreError::Io),
        )?;
        Ok(())
    }
}
