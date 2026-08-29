use layerfs_sdk::{
    AddResult, AdmissionSetReceipt, BranchCommit, BranchId, BranchRecord, BranchSource,
    BranchStore, CommitId, CreatedStack, DatabaseReceipt, InitializedLayer, LayerId,
    LayerInitialization, LayerSource, LayerStore, LocalAdmissionReceipt, LocalObjectReceipt,
    MergeOutcome, ObjectTransferReceipt, OperationReceipt, PulledBranch, RefOutcome, StackId,
    StackStore, StorageReceipt, TransferReceipt, TransferSetReceipt, TransportReceipt,
    WorkspaceSession, WorkspaceState,
};

#[test]
fn semantic_store_manifest_is_exactly_twelve_operations() {
    let _: fn(&LayerStore, LayerInitialization) -> layerfs_sdk::Result<InitializedLayer> =
        LayerStore::initialize;
    let _: fn(&StackStore, LayerId) -> layerfs_sdk::Result<RefOutcome<LayerId>> =
        StackStore::pull_layer;
    let _: fn(&LayerStore, LayerSource) -> layerfs_sdk::Result<AddResult<LayerId>> =
        LayerStore::add_layer;

    let _: fn(&StackStore, LayerId) -> layerfs_sdk::Result<CreatedStack> = StackStore::create_stack;
    let _: fn(&StackStore, StackId) -> layerfs_sdk::Result<RefOutcome<StackId>> =
        StackStore::pull_stack;
    let _: fn(&StackStore, BranchCommit) -> layerfs_sdk::Result<AddResult<StackId>> =
        StackStore::add_stack;
    let _: fn(&StackStore, StackId) -> layerfs_sdk::Result<RefOutcome<StackId>> =
        StackStore::push_stack;

    let _: fn(&BranchStore, BranchSource) -> layerfs_sdk::Result<BranchRecord> =
        BranchStore::create_branch;
    let _: fn(&BranchStore, BranchId, BranchId) -> layerfs_sdk::Result<MergeOutcome> =
        BranchStore::merge;
    let _: fn(&BranchStore, BranchId) -> layerfs_sdk::Result<PulledBranch> =
        BranchStore::pull_branch;
    let _: fn(&BranchStore, BranchId) -> layerfs_sdk::Result<RefOutcome<CommitId>> =
        BranchStore::push_branch;
    let _: fn(&BranchStore, BranchId) -> layerfs_sdk::Result<CommitId> = BranchStore::pull_commits;
}

#[test]
fn operation_storage_receipt_graph_is_public() {
    fn visible<T>() {}
    visible::<AdmissionSetReceipt>();
    visible::<DatabaseReceipt>();
    visible::<LocalObjectReceipt>();
    visible::<LocalAdmissionReceipt>();
    visible::<ObjectTransferReceipt>();
    visible::<TransferSetReceipt>();
    visible::<TransferReceipt>();
    visible::<TransportReceipt>();
    let _: fn(OperationReceipt) -> Vec<StorageReceipt> = |receipt| receipt.storage;
    let _: fn(WorkspaceSession) -> WorkspaceState = |session| session.state;
}
