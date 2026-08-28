//! Central Verified authority over one physically distinct Durable StorageId.

#![forbid(unsafe_code)]

mod backup;
mod branch;
mod compaction;
mod layer_stack;
mod recovery;
mod store;

pub use layerfs_storage::{
    derive_id, BranchHead, BranchId, BranchPushBundle, BranchPushOutcome, BranchPushRequest,
    BranchRollbackOutcome, BranchRollbackPublication, ChildMergeOutcome, ChildMergePublication,
    LayerCandidate, LayerId, LayerStackHead, LayerStackId, LayerStackMergeOutcome,
    LayerStackRollbackOutcome, OperationVersionId, RequestId,
};
pub use store::{DurableError, DurableStore, Result};

pub const COMPONENT: &str = "layerfs-durable-store";
