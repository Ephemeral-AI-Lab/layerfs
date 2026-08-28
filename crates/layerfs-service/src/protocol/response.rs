use layerfs_core::ObjectId;
use layerfs_durable_store::{
    BranchHead, LayerStackHead, LayerStackMergeOutcome, LayerStackRollbackOutcome,
};
use layerfs_sync::{BranchPushBundle, BranchPushOutcome, BranchRollbackOutcome, ChildMergeOutcome};

#[derive(serde::Deserialize, serde::Serialize)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum WireResponse {
    StorageId([u8; 32]),
    BranchHead(Option<BranchHead>),
    LayerStackHead(LayerStackHead),
    Object(Vec<u8>),
    Bool(bool),
    Unit,
    Count(u64),
    BranchPush(BranchPushOutcome),
    BranchBundle(BranchPushBundle),
    ObjectPage(Vec<ObjectId>),
    ChildMerge(ChildMergeOutcome),
    BranchRollback(BranchRollbackOutcome),
    LayerStackMerge(LayerStackMergeOutcome),
    LayerStackRollback(LayerStackRollbackOutcome),
    Error(String),
}
