//! Public use-case facades over Working storage, workspaces, and explicit synchronization.

#![forbid(unsafe_code)]

mod branch;
mod checkout;
mod durable;
mod error;
mod layer_stack;
mod route;
mod working;

pub use branch::*;
pub use durable::*;
pub use error::{Error, Result};
pub use working::LayerFs;

pub use layerfs_core::logical::{ListPage, Stat};
pub use layerfs_materialization::{NativeRoute, OperationCounters};
pub use layerfs_sync::{
    BranchRollbackOutcome, BranchRollbackPublication, ChildMergeOutcome, ChildMergePublication,
    FetchBranchReceipt, LayerStackMergeOutcome, LayerStackRollbackOutcome, PushBranchReceipt,
    PushLayerStackGenesisReceipt, ResumeToken,
};
pub use layerfs_working_store::{
    BranchHead, BranchId, BranchPushOutcome, BranchRollbackResult, ChildMergeResult, CommitResult,
    EngineCounters, IntegrityMode, LayerCandidate, LayerId, LayerPreparationResult, LayerStackHead,
    LayerStackId, OperationId, OperationRecordRef, OperationVersionId, RecoverableOperation,
    VersionRef,
};
pub use layerfs_workspace::LeaseKind;

pub const COMPONENT: &str = "layerfs-sdk";
