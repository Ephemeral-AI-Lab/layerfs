use crate::endpoint::{LayerEndpoint, RemoteEndpoint, StackPublicationEndpoint};
use layerfs_branch_store::BranchStore;
use layerfs_layer_store::LayerStore;
use layerfs_stack_store::StackStore;
use layerfs_storage_core::{
    AddLayerSource, AddResult, BranchId, BranchRecord, Change, CommitId, LayerHistoryId,
    LayerHistoryRecord, LayerId, LayerRecord, MergeOutcome, RefOutcome, Result, StackHistoryId,
    StackHistoryRecord, StackId, StackRecord,
};
use layerfs_workspace::Workspace;
use std::path::Path;
use std::sync::Arc;

/// Stacked topology intentionally has no `create_branch_from_layer` route.
///
/// ```compile_fail
/// use layerfs_sdk::Stacked;
/// let operation = Stacked::create_branch_from_layer;
/// ```
#[derive(Clone)]
pub struct Stacked {
    branch: BranchStore,
    stack: Arc<StackStore>,
    publication: StackPublicationEndpoint,
    layer: LayerEndpoint,
}

impl Stacked {
    pub fn open(
        branch_db: impl AsRef<Path>,
        stack_db: impl AsRef<Path>,
        layer_db: impl AsRef<Path>,
    ) -> Result<Self> {
        let layer = Arc::new(LayerStore::open(layer_db)?);
        let stack = Arc::new(StackStore::open(stack_db, layer.clone())?);
        let branch = BranchStore::open(branch_db, stack.clone())?;
        Ok(Self {
            branch,
            publication: StackPublicationEndpoint::Embedded(stack.clone()),
            stack,
            layer: LayerEndpoint::Embedded(layer),
        })
    }

    pub fn bootstrap(
        branch_db: impl AsRef<Path>,
        stack_db: impl AsRef<Path>,
        layer_db: impl AsRef<Path>,
    ) -> Result<(
        Self,
        LayerHistoryRecord,
        LayerRecord,
        StackHistoryRecord,
        StackRecord,
        BranchRecord,
    )> {
        let layer = Arc::new(LayerStore::open(layer_db)?);
        let (layer_history, genesis) = layer.provision()?;
        let stack = Arc::new(StackStore::open(stack_db, layer.clone())?);
        stack.pull_layer_history(layer_history.id, genesis.id)?;
        let (stack_history, seed) =
            stack.create_stack_history_from_layer(layer_history.id, genesis.id)?;
        let branch = BranchStore::open(branch_db, stack.clone())?;
        let created = branch.create_branch_from_stack(stack_history.id, seed.id)?;
        Ok((
            Self {
                branch,
                publication: StackPublicationEndpoint::Embedded(stack.clone()),
                stack,
                layer: LayerEndpoint::Embedded(layer),
            },
            layer_history,
            genesis,
            stack_history,
            seed,
            created,
        ))
    }

    pub fn from_parts(
        branch: BranchStore,
        stack: Arc<StackStore>,
        stack_endpoint: RemoteEndpoint,
        layer_endpoint: RemoteEndpoint,
    ) -> Self {
        Self {
            branch,
            stack,
            publication: StackPublicationEndpoint::Remote(stack_endpoint),
            layer: LayerEndpoint::Remote(layer_endpoint),
        }
    }

    pub fn workspace(&self, branch_id: BranchId, spool: impl AsRef<Path>) -> Result<Workspace> {
        crate::binding::workspace(&self.branch, branch_id, spool)
    }

    pub fn materialize(&self, branch_id: BranchId, destination: &Path) -> Result<()> {
        crate::binding::materialize(&self.branch, branch_id, destination)
    }

    pub fn create_branch_from_stack(
        &self,
        stack_history_id: StackHistoryId,
        stack_id: StackId,
    ) -> Result<BranchRecord> {
        self.branch
            .create_branch_from_stack(stack_history_id, stack_id)
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

    pub fn pull_commit_history(&self, branch_id: BranchId) -> Result<CommitId> {
        self.stack.pull_commit_history(branch_id)
    }

    pub fn create_stack_history_from_layer(
        &self,
        layer_history_id: LayerHistoryId,
        layer_id: LayerId,
    ) -> Result<(StackHistoryRecord, StackRecord)> {
        self.stack
            .create_stack_history_from_layer(layer_history_id, layer_id)
    }

    pub fn pull_layer_history(
        &self,
        layer_history_id: LayerHistoryId,
        through_layer_id: LayerId,
    ) -> Result<RefOutcome<LayerId>> {
        self.stack
            .pull_layer_history(layer_history_id, through_layer_id)
    }

    pub fn pull_stack_history(
        &self,
        stack_history_id: StackHistoryId,
        through_stack_id: StackId,
    ) -> Result<RefOutcome<StackId>> {
        self.stack
            .pull_stack_history(stack_history_id, through_stack_id)
    }

    pub fn add_stack(
        &self,
        stack_history_id: StackHistoryId,
        branch_id: BranchId,
        commit_id: CommitId,
    ) -> Result<AddResult<StackId>> {
        self.publication
            .add_stack(stack_history_id, branch_id, commit_id)
    }

    pub fn push_stack(&self, stack_id: StackId) -> Result<RefOutcome<StackId>> {
        self.publication.push_stack(stack_id)
    }

    pub fn add_layer(
        &self,
        layer_history_id: LayerHistoryId,
        source: AddLayerSource,
    ) -> Result<AddResult<LayerId>> {
        self.layer.add_layer(layer_history_id, source)
    }
}
