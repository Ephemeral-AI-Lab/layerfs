use layerfs_sdk::{
    AddLayerSource, AddResult, BranchId, BranchRecord, Change, CommitId, Direct, LayerHistoryId,
    LayerId, MergeOutcome, RefOutcome, Result, StackHistoryId, StackHistoryRecord, StackId,
    StackRecord, Stacked,
};

const OPERATIONS: [&str; 14] = [
    "create_branch_from_layer",
    "create_branch_from_stack",
    "create_branch_from_commit",
    "commit",
    "merge",
    "pull_branch",
    "push_branch",
    "pull_commit_history",
    "create_stack_history_from_layer",
    "pull_layer_history",
    "pull_stack_history",
    "add_stack",
    "push_stack",
    "add_layer",
];

type CommitOperation = fn(&Stacked, BranchId, CommitId, &[Change]) -> Result<RefOutcome<CommitId>>;
type DirectCommitOperation =
    fn(&Direct, BranchId, CommitId, &[Change]) -> Result<RefOutcome<CommitId>>;

#[test]
fn public_domain_manifest_is_exactly_the_fourteen_frozen_operations() {
    let _: fn(&Direct, LayerHistoryId, LayerId) -> Result<BranchRecord> =
        Direct::create_branch_from_layer;
    let _: fn(&Direct, BranchId, CommitId) -> Result<BranchRecord> =
        Direct::create_branch_from_commit;
    let _: DirectCommitOperation = Direct::commit;
    let _: fn(&Direct, BranchId, BranchId, CommitId) -> Result<MergeOutcome> = Direct::merge;
    let _: fn(&Direct, BranchId, BranchId) -> Result<RefOutcome<CommitId>> = Direct::pull_branch;
    let _: fn(&Direct, BranchId) -> Result<RefOutcome<CommitId>> = Direct::push_branch;
    let _: fn(&Direct, LayerHistoryId, AddLayerSource) -> Result<AddResult<LayerId>> =
        Direct::add_layer;
    let _: fn(&Stacked, StackHistoryId, StackId) -> Result<BranchRecord> =
        Stacked::create_branch_from_stack;
    let _: fn(&Stacked, BranchId, CommitId) -> Result<BranchRecord> =
        Stacked::create_branch_from_commit;
    let _: CommitOperation = Stacked::commit;
    let _: fn(&Stacked, BranchId, BranchId, CommitId) -> Result<MergeOutcome> = Stacked::merge;
    let _: fn(&Stacked, BranchId, BranchId) -> Result<RefOutcome<CommitId>> = Stacked::pull_branch;
    let _: fn(&Stacked, BranchId) -> Result<RefOutcome<CommitId>> = Stacked::push_branch;
    let _: fn(&Stacked, BranchId) -> Result<CommitId> = Stacked::pull_commit_history;
    let _: fn(&Stacked, LayerHistoryId, LayerId) -> Result<(StackHistoryRecord, StackRecord)> =
        Stacked::create_stack_history_from_layer;
    let _: fn(&Stacked, LayerHistoryId, LayerId) -> Result<RefOutcome<LayerId>> =
        Stacked::pull_layer_history;
    let _: fn(&Stacked, StackHistoryId, StackId) -> Result<RefOutcome<StackId>> =
        Stacked::pull_stack_history;
    let _: fn(&Stacked, StackHistoryId, BranchId, CommitId) -> Result<AddResult<StackId>> =
        Stacked::add_stack;
    let _: fn(&Stacked, StackId) -> Result<RefOutcome<StackId>> = Stacked::push_stack;
    let _: fn(&Stacked, LayerHistoryId, AddLayerSource) -> Result<AddResult<LayerId>> =
        Stacked::add_layer;
    assert_eq!(OPERATIONS.len(), 14);
}
