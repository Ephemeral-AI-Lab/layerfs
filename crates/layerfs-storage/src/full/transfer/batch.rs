//! Bounded history-transfer records and progress.

use crate::full::branch::read::{BranchAncestry, BranchHead, VersionRef};
use crate::full::layer_stack::read::LayerStackHead;
use crate::full::record_id::{
    derive_id, BranchId, LayerId, OperationId, OperationVersionId, RequestId,
};
use crate::{EngineError, EngineResult};
use layerfs_core::ObjectId;
use serde::{Deserialize, Serialize};

pub const MAX_PUSH_OPERATION_RECORDS: usize = 1024;
pub const MAX_HISTORY_PAGE_RECORDS: usize = 64;
pub const MAX_TRANSITION_PAYLOAD_BYTES: usize = 256;
pub const BRANCH_PUSH_IDENTITY_VERSION: u64 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PushedRelease {
    pub generation: u64,
    pub request_id: RequestId,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PushedOperation {
    pub operation_id: OperationId,
    pub operation_sequence: u64,
    pub expected_branch_generation: u64,
    pub base: VersionRef,
    pub operation_version_id: OperationVersionId,
    pub version_sequence: u64,
    pub parent_operation_version_id: Option<OperationVersionId>,
    pub root: ObjectId,
    pub release: Option<PushedRelease>,
    pub operation_delta_id: [u8; 32],
    pub transition_delta_id: [u8; 32],
    pub transition_payload: Vec<u8>,
    pub request_id: RequestId,
    pub before_generation: u64,
    pub after_generation: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PushedChildMerge {
    pub operation_version_id: OperationVersionId,
    pub version_sequence: u64,
    pub parent_operation_version_id: Option<OperationVersionId>,
    pub root: ObjectId,
    pub release: Option<PushedRelease>,
    pub source_branch_id: BranchId,
    pub source_branch_generation: u64,
    pub source_operation_version_id: OperationVersionId,
    pub branch_delta_id: [u8; 32],
    pub base_root: ObjectId,
    pub source_root: ObjectId,
    pub destination_root: ObjectId,
    pub source_delta_id: [u8; 32],
    pub source_transition_payload: Vec<u8>,
    pub applied_delta_id: [u8; 32],
    pub applied_transition_payload: Vec<u8>,
    pub request_id: RequestId,
    pub before_generation: u64,
    pub after_generation: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PushedBranchRollback {
    pub before_operation_version_id: OperationVersionId,
    pub target_operation_version_id: OperationVersionId,
    pub target_root: ObjectId,
    pub request_id: RequestId,
    pub before_generation: u64,
    pub after_generation: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PushedLayerMerge {
    pub source_branch_id: BranchId,
    pub source_branch_depth: u64,
    pub source_branch_generation: u64,
    pub source_operation_version_id: OperationVersionId,
    pub request_id: RequestId,
    pub branch_delta_id: [u8; 32],
    pub base_root: ObjectId,
    pub source_root: ObjectId,
    pub destination_root: ObjectId,
    pub source_delta_id: [u8; 32],
    pub source_transition_payload: Vec<u8>,
    pub applied_delta_id: [u8; 32],
    pub applied_transition_payload: Vec<u8>,
    pub layer_delta_id: [u8; 32],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PushedLayer {
    pub layer_id: LayerId,
    pub parent_layer_id: Option<LayerId>,
    pub root: ObjectId,
    pub release: Option<PushedRelease>,
    pub accepted_generation: u64,
    pub merge: Option<PushedLayerMerge>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PushedLayerStackAction {
    Merge,
    Rollback,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PushedLayerStackTransition {
    pub before_generation: u64,
    pub after_generation: u64,
    pub before_layer_id: LayerId,
    pub after_layer_id: LayerId,
    pub action: PushedLayerStackAction,
    pub source_record_id: [u8; 32],
    pub request_id: RequestId,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PushedLayerStack {
    pub name: String,
    pub base: Option<LayerStackHead>,
    pub head: LayerStackHead,
    pub complete: bool,
    pub layers: Vec<PushedLayer>,
    pub transitions: Vec<PushedLayerStackTransition>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BranchPushBundle {
    pub name: Option<String>,
    pub ancestry: BranchAncestry,
    pub base: Option<BranchHead>,
    pub head: BranchHead,
    pub complete: bool,
    pub operations: Vec<PushedOperation>,
    pub child_merges: Vec<PushedChildMerge>,
    pub rollbacks: Vec<PushedBranchRollback>,
    pub origin_stack: PushedLayerStack,
    pub dependencies: Vec<BranchPushBundle>,
}

pub(crate) fn validate_staged_push_page(bundle: &BranchPushBundle) -> EngineResult<()> {
    let history_len = bundle
        .operations
        .len()
        .checked_add(bundle.child_merges.len())
        .and_then(|count| count.checked_add(bundle.rollbacks.len()))
        .ok_or(EngineError::CounterOverflow)?;
    let base_generation = bundle.base.map_or(0, |head| head.generation);
    if history_len > MAX_HISTORY_PAGE_RECORDS
        || !bundle.dependencies.is_empty()
        || bundle.head.branch_id
            != bundle
                .base
                .map_or(bundle.head.branch_id, |head| head.branch_id)
        || bundle.head.generation < base_generation
        || usize::try_from(bundle.head.generation - base_generation)
            .map_err(|_| EngineError::CounterOverflow)?
            != history_len
        || bundle.origin_stack.head.layer_stack_id != bundle.ancestry.origin_layer_stack_id
        || bundle.origin_stack.base != Some(bundle.origin_stack.head)
        || !bundle.origin_stack.complete
        || !bundle.origin_stack.layers.is_empty()
        || !bundle.origin_stack.transitions.is_empty()
        || bundle.origin_stack.name.is_empty()
        || bundle.origin_stack.name.len() > 255
        || bundle
            .name
            .as_ref()
            .is_some_and(|name| name.is_empty() || name.len() > 255)
        || bundle
            .operations
            .iter()
            .any(|operation| operation.transition_payload.len() > MAX_TRANSITION_PAYLOAD_BYTES)
        || bundle.child_merges.iter().any(|merge| {
            merge.source_transition_payload.len() > MAX_TRANSITION_PAYLOAD_BYTES
                || merge.applied_transition_payload.len() > MAX_TRANSITION_PAYLOAD_BYTES
        })
    {
        return Err(EngineError::InvalidRecord("Push page"));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BranchPushOutcome {
    DurablyAccepted { head: BranchHead, reconciled: bool },
    Conflict { actual: Option<BranchHead> },
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SyncTransferCounters {
    pub unique_bytes: u64,
    pub resumed_bytes: u64,
    pub retransmitted_bytes: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BranchPushRequest {
    pub request_id: RequestId,
    pub transfer_id: RequestId,
    pub candidate_digest: [u8; 32],
    pub expected: Option<BranchHead>,
    pub counters: SyncTransferCounters,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BranchPushIdentityBuilder {
    transfer_id: RequestId,
    next_sequence: u64,
    chain_digest: [u8; 32],
}

impl BranchPushIdentityBuilder {
    pub fn new(transfer_id: RequestId) -> Self {
        Self {
            transfer_id,
            next_sequence: 0,
            chain_digest: derive_id(b"layerfs.branch-push-chain.v1", &[transfer_id.as_bytes()]),
        }
    }

    pub fn absorb_page(&mut self, page_sequence: u64, page_digest: [u8; 32]) -> EngineResult<()> {
        if page_sequence != self.next_sequence {
            return Err(EngineError::InvalidRecord("Push page sequence"));
        }
        self.chain_digest = derive_id(
            b"layerfs.branch-push-chain-page.v1",
            &[
                self.transfer_id.as_bytes(),
                &page_sequence.to_be_bytes(),
                &self.chain_digest,
                &page_digest,
            ],
        );
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(EngineError::CounterOverflow)?;
        Ok(())
    }

    pub fn finish(self, head: BranchHead) -> [u8; 32] {
        let (version_present, version) = head
            .operation_version_id
            .map_or(([0_u8], [0_u8; 32]), |id| ([1_u8], *id.as_bytes()));
        derive_id(
            b"layerfs.branch-push-candidate.v1",
            &[
                &BRANCH_PUSH_IDENTITY_VERSION.to_be_bytes(),
                self.transfer_id.as_bytes(),
                &self.next_sequence.to_be_bytes(),
                &self.chain_digest,
                head.branch_id.as_bytes(),
                &version_present,
                &version,
                &head.generation.to_be_bytes(),
                head.root.as_bytes(),
            ],
        )
    }
}

pub fn branch_push_page_digest(
    transfer_id: RequestId,
    page_sequence: u64,
    data_request_id: RequestId,
    branch_id: BranchId,
    encoded_bundle: &[u8],
    counters: SyncTransferCounters,
) -> [u8; 32] {
    derive_id(
        b"layerfs.branch-push-page.v1",
        &[
            &BRANCH_PUSH_IDENTITY_VERSION.to_be_bytes(),
            transfer_id.as_bytes(),
            &page_sequence.to_be_bytes(),
            data_request_id.as_bytes(),
            branch_id.as_bytes(),
            encoded_bundle,
            &counters.unique_bytes.to_be_bytes(),
            &counters.resumed_bytes.to_be_bytes(),
            &counters.retransmitted_bytes.to_be_bytes(),
        ],
    )
}

pub fn branch_push_bundle_page_digest(
    transfer_id: RequestId,
    page_sequence: u64,
    data_request_id: RequestId,
    bundle: &BranchPushBundle,
    counters: SyncTransferCounters,
) -> EngineResult<[u8; 32]> {
    let encoded =
        serde_json::to_vec(bundle).map_err(|_| EngineError::InvalidRecord("Push page encoding"))?;
    Ok(branch_push_page_digest(
        transfer_id,
        page_sequence,
        data_request_id,
        bundle.head.branch_id,
        &encoded,
        counters,
    ))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedFetchRequest {
    pub request_id: RequestId,
    pub durable_storage_id: [u8; 32],
    pub counters: SyncTransferCounters,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredTransferState {
    pub batch_sequence: u64,
    pub cursor: Vec<u8>,
    pub complete: bool,
    pub counters: SyncTransferCounters,
}
