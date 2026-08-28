use super::{LayerStackHead, TransferReceipt};

pub use layerfs_storage::{
    BranchPushBundle, BranchPushOutcome, BranchPushRequest, BranchRollbackOutcome,
    BranchRollbackPublication, ChildMergeOutcome, ChildMergePublication, LayerCandidate,
    LayerStackMergeOutcome, LayerStackRollbackOutcome, SyncTransferCounters,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PushBranchReceipt {
    pub outcome: BranchPushOutcome,
    pub transfer: TransferReceipt,
    pub history_export_ns: u128,
    pub closure_traversal_ns: u128,
    pub staging_ns: u128,
    pub head_transaction_ns: u128,
    pub complete_wall_ns: u128,
    pub terminal_queued_batches: u64,
    pub pages: u64,
    pub complete: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PushLayerStackGenesisReceipt {
    pub head: LayerStackHead,
    pub transfer: TransferReceipt,
    pub closure_traversal_ns: u128,
    pub head_transaction_ns: u128,
    pub complete_wall_ns: u128,
}
