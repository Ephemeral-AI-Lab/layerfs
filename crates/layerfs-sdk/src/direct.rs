use crate::endpoint::{LayerEndpoint, RemoteEndpoint};
use layerfs_branch_store::BranchStore;
use layerfs_layer_store::LayerStore;
use layerfs_storage_core::{
    AddLayerSource, AddResult, BranchId, BranchRecord, Change, CommitId, LayerHistoryId,
    LayerHistoryRecord, LayerId, LayerRecord, MergeOutcome, RefOutcome, Result,
};
use layerfs_workspace::Workspace;
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
pub struct Direct {
    branch: BranchStore,
    layer: LayerEndpoint,
}

impl Direct {
    pub fn open(branch_db: impl AsRef<Path>, layer_db: impl AsRef<Path>) -> Result<Self> {
        let layer = Arc::new(LayerStore::open(layer_db)?);
        let branch = BranchStore::open(branch_db, layer.clone())?;
        Ok(Self {
            branch,
            layer: LayerEndpoint::Embedded(layer),
        })
    }

    pub fn bootstrap(
        branch_db: impl AsRef<Path>,
        layer_db: impl AsRef<Path>,
    ) -> Result<(Self, LayerHistoryRecord, LayerRecord, BranchRecord)> {
        let layer = Arc::new(LayerStore::open(layer_db)?);
        let (history, genesis) = layer.provision()?;
        let branch = BranchStore::open(branch_db, layer.clone())?;
        let created = branch.create_branch_from_layer(history.id, genesis.id)?;
        Ok((
            Self {
                branch,
                layer: LayerEndpoint::Embedded(layer),
            },
            history,
            genesis,
            created,
        ))
    }

    pub fn from_parts(branch: BranchStore, layer: RemoteEndpoint) -> Self {
        Self {
            branch,
            layer: LayerEndpoint::Remote(layer),
        }
    }

    pub fn workspace(&self, branch_id: BranchId, spool: impl AsRef<Path>) -> Result<Workspace> {
        crate::binding::workspace(&self.branch, branch_id, spool)
    }

    pub fn materialize(&self, branch_id: BranchId, destination: &Path) -> Result<()> {
        crate::binding::materialize(&self.branch, branch_id, destination)
    }

    pub fn create_branch_from_layer(
        &self,
        layer_history_id: LayerHistoryId,
        layer_id: LayerId,
    ) -> Result<BranchRecord> {
        self.branch
            .create_branch_from_layer(layer_history_id, layer_id)
    }

    pub fn create_branch_from_commit(
        &self,
        source_branch_id: BranchId,
        source_commit_id: CommitId,
    ) -> Result<BranchRecord> {
        self.branch
            .create_branch_from_commit(source_branch_id, source_commit_id)
    }

    pub fn commit(
        &self,
        branch_id: BranchId,
        expected_head: CommitId,
        changes: &[Change],
    ) -> Result<RefOutcome<CommitId>> {
        self.branch.commit(branch_id, expected_head, changes)
    }

    pub fn merge(
        &self,
        source_branch_id: BranchId,
        target_branch_id: BranchId,
        expected_target_head: CommitId,
    ) -> Result<MergeOutcome> {
        self.branch
            .merge(source_branch_id, target_branch_id, expected_target_head)
    }

    pub fn pull_branch(
        &self,
        source_branch_id: BranchId,
        local_branch_id: BranchId,
    ) -> Result<RefOutcome<CommitId>> {
        self.branch.pull_branch(source_branch_id, local_branch_id)
    }

    pub fn push_branch(&self, branch_id: BranchId) -> Result<RefOutcome<CommitId>> {
        self.branch.push_branch(branch_id)
    }

    pub fn add_layer(
        &self,
        layer_history_id: LayerHistoryId,
        source: AddLayerSource,
    ) -> Result<AddResult<LayerId>> {
        self.layer.add_layer(layer_history_id, source)
    }
}
