//! Explicit bounded Fetch/Push data plane. Transfer alone changes no head.

#![forbid(unsafe_code)]

use layerfs_core::ObjectId;
pub use layerfs_storage::product::{
    BranchHead, BranchId, BranchPushBundle, BranchPushOutcome, BranchPushRequest,
    BranchRollbackOutcome, BranchRollbackPublication, ChildMergeOutcome, ChildMergePublication,
    LayerCandidate, LayerId, LayerStackHead, LayerStackId, LayerStackMergeOutcome,
    LayerStackRollbackOutcome, RequestId, SyncTransferCounters,
};
use layerfs_working_store::WorkingStore;
use std::fmt;
use std::time::Instant;

pub const COMPONENT: &str = "layerfs-sync";
pub const MAX_BATCH_BYTES: usize = 1024 * 1024;
pub const MAX_BATCH_OBJECTS: usize = 1024;
pub const MAX_QUEUED_BATCHES: usize = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum Direction {
    Fetch,
    Push,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferResult {
    TransferredNoVisibility,
    ReconciledNoTransfer,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResumeToken {
    next_object_index: u64,
    binding: [u8; 32],
}

impl ResumeToken {
    pub const fn next_object_index(self) -> u64 {
        self.next_object_index
    }

    fn encode(self) -> [u8; 40] {
        let mut encoded = [0; 40];
        encoded[..8].copy_from_slice(&self.next_object_index.to_be_bytes());
        encoded[8..].copy_from_slice(&self.binding);
        encoded
    }

    fn decode(encoded: &[u8]) -> Result<Self> {
        if encoded.len() < 40 {
            return Err(SyncError::InvalidResume);
        }
        let mut index = [0; 8];
        index.copy_from_slice(&encoded[..8]);
        let mut binding = [0; 32];
        binding.copy_from_slice(&encoded[8..40]);
        Ok(Self {
            next_object_index: u64::from_be_bytes(index),
            binding,
        })
    }
}

#[derive(Clone, Copy)]
struct PendingObject {
    id: ObjectId,
    bytes: u64,
}

struct LoadedResume {
    token: ResumeToken,
    previous: Option<layerfs_storage::product::StoredTransferState>,
    pending: Vec<PendingObject>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransferReceipt {
    pub request_id: [u8; 32],
    pub source_storage_id: [u8; 32],
    pub destination_storage_id: [u8; 32],
    pub direction: Direction,
    pub result: TransferResult,
    pub objects_examined: u64,
    pub known_present_objects: u64,
    pub missing_objects: u64,
    pub transferred_objects: u64,
    pub unique_bytes: u64,
    pub resumed_bytes: u64,
    pub retransmitted_bytes: u64,
    pub batches: u64,
    pub largest_batch_bytes: u64,
    pub largest_batch_objects: u64,
    pub negotiation_ns: u128,
    pub source_read_ns: u128,
    pub receiver_admission_ns: u128,
    pub complete_wall_ns: u128,
    pub terminal_buffer_bytes: u64,
    pub terminal_queued_batches: u64,
    pub resume: ResumeToken,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FetchBranchReceipt {
    pub head: BranchHead,
    pub origin_stack_head: LayerStackHead,
    pub transfer: TransferReceipt,
    pub dependency_transfer: Option<TransferReceipt>,
    pub history_export_ns: u128,
    pub closure_traversal_ns: u128,
    pub head_transaction_ns: u128,
    pub complete_wall_ns: u128,
    pub terminal_object_page_entries: u64,
    pub pages: u64,
    pub dependency_pages: u64,
    pub complete: bool,
}

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

#[derive(Debug)]
pub enum SyncError {
    Source(String),
    Destination(String),
    ResourceExhausted,
    CounterOverflow,
    SameStorage,
    InvalidResume,
    Progress(String),
}

impl fmt::Display for SyncError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for SyncError {}

pub type Result<T> = std::result::Result<T, SyncError>;

pub trait DurableEndpoint {
    fn durable_storage_id(&self) -> [u8; 32];
    fn read_object(&self, id: ObjectId, maximum: usize) -> Result<Vec<u8>>;
    fn contains_object(&self, id: ObjectId) -> Result<bool>;
    fn accept_objects(
        &self,
        owner_request_id: RequestId,
        request_id: RequestId,
        direction: Direction,
        objects: &[(ObjectId, Vec<u8>)],
    ) -> Result<()>;
    fn abort_transfer(&self, owner_request_id: RequestId, direction: Direction) -> Result<u64>;
}

pub trait DurableControlEndpoint: DurableEndpoint {
    fn branch_head(&self, branch_id: BranchId) -> Result<Option<BranchHead>>;
    fn bootstrap_layer_stack(
        &self,
        stack: LayerStackId,
        layer: LayerId,
        name: &str,
        root: ObjectId,
    ) -> Result<LayerStackHead>;
    fn stage_branch_push_page(
        &self,
        transfer_id: RequestId,
        page_sequence: u64,
        data_request_id: RequestId,
        bundle: &BranchPushBundle,
        counters: SyncTransferCounters,
    ) -> Result<()>;
    fn commit_staged_branch_push(
        &self,
        request: BranchPushRequest,
        branch_id: BranchId,
    ) -> Result<BranchPushOutcome>;
    fn reconcile_branch_push(
        &self,
        request_id: RequestId,
        expected: Option<BranchHead>,
        accepted: BranchHead,
    ) -> Result<BranchPushOutcome>;
    fn export_branch_fetch(
        &self,
        branch_id: BranchId,
        base: Option<BranchHead>,
        origin_stack_base: Option<LayerStackHead>,
    ) -> Result<BranchPushBundle>;
    #[allow(clippy::too_many_arguments)]
    fn branch_fetch_object_page(
        &self,
        branch_id: BranchId,
        base: Option<BranchHead>,
        origin_stack_base: Option<LayerStackHead>,
        expected_head: BranchHead,
        expected_stack_head: LayerStackHead,
        after: Option<ObjectId>,
        limit: usize,
    ) -> Result<Vec<ObjectId>>;
    fn accept_child_branch_merge(
        &self,
        publication: ChildMergePublication,
    ) -> Result<ChildMergeOutcome>;
    fn accept_branch_rollback(
        &self,
        publication: BranchRollbackPublication,
    ) -> Result<BranchRollbackOutcome>;
    fn accept_layer_stack_merge(
        &self,
        candidate: LayerCandidate,
        expected: LayerStackHead,
    ) -> Result<LayerStackMergeOutcome>;
    fn layer_stack_rollback(
        &self,
        expected: LayerStackHead,
        target: LayerId,
    ) -> Result<LayerStackRollbackOutcome>;
}

trait Source {
    fn storage_id(&self) -> [u8; 32];
    fn read(&self, id: ObjectId, maximum: usize) -> Result<Vec<u8>>;
}

trait Destination {
    fn storage_id(&self) -> [u8; 32];
    fn contains(&self, id: ObjectId) -> Result<bool>;
    fn accept(
        &self,
        owner_request_id: RequestId,
        request_id: RequestId,
        direction: Direction,
        objects: &[(ObjectId, Vec<u8>)],
    ) -> Result<()>;
}

impl Source for WorkingStore {
    fn storage_id(&self) -> [u8; 32] {
        self.storage_id()
    }

    fn read(&self, id: ObjectId, maximum: usize) -> Result<Vec<u8>> {
        self.sync_read_object(id, maximum)
            .map_err(|error| SyncError::Source(error.to_string()))
    }
}

impl Destination for WorkingStore {
    fn storage_id(&self) -> [u8; 32] {
        self.storage_id()
    }

    fn contains(&self, id: ObjectId) -> Result<bool> {
        self.sync_has_object(id)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn accept(
        &self,
        owner_request_id: RequestId,
        request_id: RequestId,
        direction: Direction,
        objects: &[(ObjectId, Vec<u8>)],
    ) -> Result<()> {
        self.sync_accept_objects(
            owner_request_id,
            request_id,
            match direction {
                Direction::Fetch => "fetch",
                Direction::Push => "push",
            },
            objects,
        )
        .map_err(|error| SyncError::Destination(error.to_string()))
    }
}

struct EndpointSource<'a, T>(&'a T);

impl<T: DurableEndpoint> Source for EndpointSource<'_, T> {
    fn storage_id(&self) -> [u8; 32] {
        self.0.durable_storage_id()
    }

    fn read(&self, id: ObjectId, maximum: usize) -> Result<Vec<u8>> {
        self.0.read_object(id, maximum)
    }
}

struct EndpointDestination<'a, T>(&'a T);

impl<T: DurableEndpoint> Destination for EndpointDestination<'_, T> {
    fn storage_id(&self) -> [u8; 32] {
        self.0.durable_storage_id()
    }

    fn contains(&self, id: ObjectId) -> Result<bool> {
        self.0.contains_object(id)
    }

    fn accept(
        &self,
        owner_request_id: RequestId,
        request_id: RequestId,
        direction: Direction,
        objects: &[(ObjectId, Vec<u8>)],
    ) -> Result<()> {
        self.0
            .accept_objects(owner_request_id, request_id, direction, objects)
    }
}

pub mod server {
    use super::*;
    use layerfs_durable_store::DurableStore;

    pub struct LocalDurable<'a>(&'a DurableStore);

    impl<'a> LocalDurable<'a> {
        pub const fn new(store: &'a DurableStore) -> Self {
            Self(store)
        }
    }

    impl DurableEndpoint for LocalDurable<'_> {
        fn durable_storage_id(&self) -> [u8; 32] {
            self.0.storage_id()
        }

        fn read_object(&self, id: ObjectId, maximum: usize) -> Result<Vec<u8>> {
            self.0
                .sync_read_object(id, maximum)
                .map_err(|error| SyncError::Source(error.to_string()))
        }

        fn contains_object(&self, id: ObjectId) -> Result<bool> {
            self.0
                .sync_has_object(id)
                .map_err(|error| SyncError::Destination(error.to_string()))
        }

        fn accept_objects(
            &self,
            owner_request_id: RequestId,
            request_id: RequestId,
            direction: Direction,
            objects: &[(ObjectId, Vec<u8>)],
        ) -> Result<()> {
            self.0
                .sync_accept_objects(
                    owner_request_id,
                    request_id,
                    match direction {
                        Direction::Fetch => "fetch",
                        Direction::Push => "push",
                    },
                    objects,
                )
                .map_err(|error| SyncError::Destination(error.to_string()))
        }

        fn abort_transfer(&self, owner_request_id: RequestId, direction: Direction) -> Result<u64> {
            self.0
                .abort_sync_transfer(
                    owner_request_id,
                    match direction {
                        Direction::Fetch => "fetch",
                        Direction::Push => "push",
                    },
                )
                .map_err(|error| SyncError::Destination(error.to_string()))
        }
    }

    impl DurableControlEndpoint for LocalDurable<'_> {
        fn branch_head(&self, branch_id: BranchId) -> Result<Option<BranchHead>> {
            self.0
                .branch_head(branch_id)
                .map_err(|error| SyncError::Source(error.to_string()))
        }
        fn bootstrap_layer_stack(
            &self,
            stack: LayerStackId,
            layer: LayerId,
            name: &str,
            root: ObjectId,
        ) -> Result<LayerStackHead> {
            self.0
                .bootstrap_layer_stack(stack, layer, name, root)
                .map_err(|error| SyncError::Destination(error.to_string()))
        }
        fn stage_branch_push_page(
            &self,
            transfer_id: RequestId,
            page_sequence: u64,
            data_request_id: RequestId,
            bundle: &BranchPushBundle,
            counters: SyncTransferCounters,
        ) -> Result<()> {
            self.0
                .stage_branch_push_page(
                    transfer_id,
                    page_sequence,
                    data_request_id,
                    bundle,
                    counters,
                )
                .map_err(|error| SyncError::Destination(error.to_string()))
        }

        fn commit_staged_branch_push(
            &self,
            request: BranchPushRequest,
            branch_id: BranchId,
        ) -> Result<BranchPushOutcome> {
            self.0
                .commit_staged_branch_push(request, branch_id)
                .map_err(|error| SyncError::Destination(error.to_string()))
        }

        fn reconcile_branch_push(
            &self,
            request_id: RequestId,
            expected: Option<BranchHead>,
            accepted: BranchHead,
        ) -> Result<BranchPushOutcome> {
            self.0
                .reconcile_branch_push(request_id, expected, accepted)
                .map_err(|error| SyncError::Destination(error.to_string()))
        }

        fn export_branch_fetch(
            &self,
            branch_id: BranchId,
            base: Option<BranchHead>,
            origin_stack_base: Option<LayerStackHead>,
        ) -> Result<BranchPushBundle> {
            self.0
                .export_branch_fetch(branch_id, base, origin_stack_base)
                .map_err(|error| SyncError::Source(error.to_string()))
        }

        fn branch_fetch_object_page(
            &self,
            branch_id: BranchId,
            base: Option<BranchHead>,
            origin_stack_base: Option<LayerStackHead>,
            expected_head: BranchHead,
            expected_stack_head: LayerStackHead,
            after: Option<ObjectId>,
            limit: usize,
        ) -> Result<Vec<ObjectId>> {
            self.0
                .branch_fetch_object_page(
                    branch_id,
                    base,
                    origin_stack_base,
                    expected_head,
                    expected_stack_head,
                    after,
                    limit,
                )
                .map_err(|error| SyncError::Source(error.to_string()))
        }

        fn accept_child_branch_merge(
            &self,
            publication: ChildMergePublication,
        ) -> Result<ChildMergeOutcome> {
            self.0
                .accept_child_branch_merge(publication)
                .map_err(|error| SyncError::Destination(error.to_string()))
        }

        fn accept_branch_rollback(
            &self,
            publication: BranchRollbackPublication,
        ) -> Result<BranchRollbackOutcome> {
            self.0
                .accept_branch_rollback(publication)
                .map_err(|error| SyncError::Destination(error.to_string()))
        }

        fn accept_layer_stack_merge(
            &self,
            candidate: LayerCandidate,
            expected: LayerStackHead,
        ) -> Result<LayerStackMergeOutcome> {
            self.0
                .accept_layer_stack_merge(candidate, expected)
                .map_err(|error| SyncError::Destination(error.to_string()))
        }

        fn layer_stack_rollback(
            &self,
            expected: LayerStackHead,
            target: LayerId,
        ) -> Result<LayerStackRollbackOutcome> {
            self.0
                .layer_stack_rollback(expected, target)
                .map_err(|error| SyncError::Destination(error.to_string()))
        }
    }
}

pub mod client {
    use super::*;

    pub fn abort_push_transfer(
        destination: &impl DurableEndpoint,
        owner_request_id: [u8; 32],
    ) -> Result<u64> {
        destination.abort_transfer(RequestId::from_bytes(owner_request_id), Direction::Push)
    }

    pub fn abort_fetch_transfer(
        destination: &WorkingStore,
        owner_request_id: [u8; 32],
    ) -> Result<u64> {
        destination
            .abort_sync_transfer(RequestId::from_bytes(owner_request_id), "fetch")
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    pub fn push_objects(
        source: &WorkingStore,
        destination: &impl DurableEndpoint,
        request_id: [u8; 32],
        object_ids: impl IntoIterator<Item = ObjectId>,
        resume: ResumeToken,
    ) -> Result<TransferReceipt> {
        push_objects_owned(
            source,
            destination,
            RequestId::from_bytes(request_id),
            request_id,
            object_ids,
            resume,
        )
    }

    fn push_objects_owned(
        source: &WorkingStore,
        destination: &impl DurableEndpoint,
        owner_request_id: RequestId,
        request_id: [u8; 32],
        object_ids: impl IntoIterator<Item = ObjectId>,
        resume: ResumeToken,
    ) -> Result<TransferReceipt> {
        let loaded = load_resume(source, request_id, Direction::Push, resume)?;
        transfer(
            source,
            &EndpointDestination(destination),
            source,
            owner_request_id,
            request_id,
            object_ids,
            loaded,
            Direction::Push,
        )
    }

    pub fn fetch_objects(
        source: &impl DurableEndpoint,
        destination: &WorkingStore,
        request_id: [u8; 32],
        object_ids: impl IntoIterator<Item = ObjectId>,
        resume: ResumeToken,
    ) -> Result<TransferReceipt> {
        let loaded = load_resume(destination, request_id, Direction::Fetch, resume)?;
        transfer(
            &EndpointSource(source),
            destination,
            destination,
            RequestId::from_bytes(request_id),
            request_id,
            object_ids,
            loaded,
            Direction::Fetch,
        )
    }

    pub fn push_branch(
        source: &WorkingStore,
        destination: &impl DurableControlEndpoint,
        request_id: [u8; 32],
        branch_id: BranchId,
        expected: Option<BranchHead>,
        mut resume: ResumeToken,
    ) -> Result<PushBranchReceipt> {
        let complete = Instant::now();
        if let Some((accepted, state)) = source
            .push_outbox_head(RequestId::from_bytes(request_id))
            .map_err(|error| SyncError::Progress(error.to_string()))?
        {
            if matches!(state.as_str(), "accepted" | "indeterminate") {
                let head_transaction = Instant::now();
                let outcome = destination.reconcile_branch_push(
                    RequestId::from_bytes(request_id),
                    expected,
                    accepted,
                )?;
                source
                    .record_push_outbox(
                        RequestId::from_bytes(request_id),
                        destination.durable_storage_id(),
                        accepted,
                        expected.map(|head| head.generation),
                        match outcome {
                            BranchPushOutcome::DurablyAccepted { .. } => "accepted",
                            BranchPushOutcome::Conflict { .. } => "conflict",
                        },
                    )
                    .map_err(|error| SyncError::Progress(error.to_string()))?;
                source
                    .clear_transfer_state_owner(RequestId::from_bytes(request_id), "push")
                    .map_err(|error| SyncError::Progress(error.to_string()))?;
                let mut transfer = TransferReceipt::default_for(Direction::Push, request_id);
                transfer.source_storage_id = source.storage_id();
                transfer.destination_storage_id = destination.durable_storage_id();
                transfer.result = TransferResult::ReconciledNoTransfer;
                return Ok(PushBranchReceipt {
                    outcome,
                    transfer,
                    history_export_ns: 0,
                    closure_traversal_ns: 0,
                    staging_ns: 0,
                    head_transaction_ns: head_transaction.elapsed().as_nanos(),
                    complete_wall_ns: complete.elapsed().as_nanos(),
                    terminal_queued_batches: 0,
                    pages: 0,
                    complete: true,
                });
            }
        }
        let mut page_base = expected;
        let mut transfer = None;
        let mut page = 0_u64;
        let mut history_export_ns = 0_u128;
        let mut closure_traversal_ns = 0_u128;
        let mut staging_ns = 0_u128;
        let final_head;
        loop {
            let page_request = page_request_id(
                request_id,
                b"push",
                page_base.map_or(0, |head| head.generation),
            );
            let staged = match stage_branch_push_page(
                source,
                destination,
                RequestId::from_bytes(request_id),
                page,
                page_request,
                branch_id,
                page_base,
                resume,
            ) {
                Ok(receipt) => receipt,
                Err(error) => {
                    let head = source
                        .branch_head(branch_id)
                        .map_err(|source| SyncError::Source(source.to_string()))?
                        .ok_or_else(|| SyncError::Source("Push Branch disappeared".into()))?;
                    source
                        .record_push_outbox(
                            RequestId::from_bytes(request_id),
                            destination.durable_storage_id(),
                            head,
                            expected.map(|head| head.generation),
                            "transferring",
                        )
                        .map_err(|progress| SyncError::Progress(progress.to_string()))?;
                    return Err(error);
                }
            };
            page = add(page, 1)?;
            match transfer.as_mut() {
                Some(total) => merge_transfer_receipt(total, staged.transfer)?,
                None => transfer = Some(staged.transfer),
            }
            history_export_ns = add_ns(history_export_ns, staged.history_export_ns)?;
            closure_traversal_ns = add_ns(closure_traversal_ns, staged.closure_traversal_ns)?;
            staging_ns = add_ns(staging_ns, staged.staging_ns)?;
            page_base = Some(staged.head);
            if staged.complete {
                final_head = staged.head;
                break;
            }
            resume = ResumeToken::default();
        }
        let mut transfer = transfer.ok_or(SyncError::CounterOverflow)?;
        transfer.request_id = request_id;
        source
            .record_push_outbox(
                RequestId::from_bytes(request_id),
                destination.durable_storage_id(),
                final_head,
                expected.map(|head| head.generation),
                "transferred",
            )
            .map_err(|error| SyncError::Progress(error.to_string()))?;
        let head_transaction = Instant::now();
        let outcome = destination.commit_staged_branch_push(
            BranchPushRequest {
                request_id: RequestId::from_bytes(request_id),
                expected,
                counters: SyncTransferCounters {
                    unique_bytes: transfer.unique_bytes,
                    resumed_bytes: transfer.resumed_bytes,
                    retransmitted_bytes: transfer.retransmitted_bytes,
                },
            },
            branch_id,
        );
        let head_transaction_ns = head_transaction.elapsed().as_nanos();
        let state = match outcome {
            Ok(BranchPushOutcome::DurablyAccepted { .. }) => "accepted",
            Ok(BranchPushOutcome::Conflict { .. }) => "conflict",
            Err(_) => "indeterminate",
        };
        source
            .record_push_outbox(
                RequestId::from_bytes(request_id),
                destination.durable_storage_id(),
                final_head,
                expected.map(|head| head.generation),
                state,
            )
            .map_err(|error| SyncError::Progress(error.to_string()))?;
        if outcome.is_ok() {
            source
                .clear_transfer_state_owner(RequestId::from_bytes(request_id), "push")
                .map_err(|error| SyncError::Progress(error.to_string()))?;
        }
        Ok(PushBranchReceipt {
            outcome: outcome?,
            transfer,
            history_export_ns,
            closure_traversal_ns,
            staging_ns,
            head_transaction_ns,
            complete_wall_ns: complete.elapsed().as_nanos(),
            terminal_queued_batches: 0,
            pages: page,
            complete: true,
        })
    }

    struct StagedPushPageReceipt {
        head: BranchHead,
        transfer: TransferReceipt,
        history_export_ns: u128,
        closure_traversal_ns: u128,
        staging_ns: u128,
        complete: bool,
    }

    struct PreparedPushPage {
        bundle: BranchPushBundle,
        transfer: TransferReceipt,
        history_export_ns: u128,
        closure_traversal_ns: u128,
    }

    #[allow(clippy::too_many_arguments)]
    fn stage_branch_push_page(
        source: &WorkingStore,
        destination: &impl DurableControlEndpoint,
        transfer_id: RequestId,
        page_sequence: u64,
        request_id: [u8; 32],
        branch_id: BranchId,
        expected: Option<layerfs_storage::product::BranchHead>,
        resume: ResumeToken,
    ) -> Result<StagedPushPageReceipt> {
        let prepared = prepare_branch_push_page(
            source,
            destination,
            transfer_id,
            request_id,
            branch_id,
            expected,
            resume,
        )?;
        let PreparedPushPage {
            bundle,
            transfer,
            history_export_ns,
            closure_traversal_ns,
        } = prepared;
        for merge in &bundle.child_merges {
            let source_head = BranchHead {
                branch_id: merge.source_branch_id,
                generation: merge.source_branch_generation,
                operation_version_id: Some(merge.source_operation_version_id),
                root: merge.source_root,
            };
            let durable_head = destination.branch_head(merge.source_branch_id)?;
            if durable_head != Some(source_head) {
                let dependency = push_branch(
                    source,
                    destination,
                    dependency_request_id(*transfer_id.as_bytes(), merge.source_branch_id),
                    merge.source_branch_id,
                    durable_head,
                    ResumeToken::default(),
                )?;
                if !matches!(
                    dependency.outcome,
                    BranchPushOutcome::DurablyAccepted { head, .. } if head == source_head
                ) {
                    return Err(SyncError::Destination(
                        "Child merge source Branch Push conflict".into(),
                    ));
                }
            }
        }
        let staging = Instant::now();
        destination.stage_branch_push_page(
            transfer_id,
            page_sequence,
            RequestId::from_bytes(request_id),
            &bundle,
            SyncTransferCounters {
                unique_bytes: transfer.unique_bytes,
                resumed_bytes: transfer.resumed_bytes,
                retransmitted_bytes: transfer.retransmitted_bytes,
            },
        )?;
        Ok(StagedPushPageReceipt {
            head: bundle.head,
            transfer,
            history_export_ns,
            closure_traversal_ns,
            staging_ns: staging.elapsed().as_nanos(),
            complete: bundle.complete,
        })
    }

    fn prepare_branch_push_page(
        source: &WorkingStore,
        destination: &impl DurableControlEndpoint,
        transfer_id: RequestId,
        request_id: [u8; 32],
        branch_id: BranchId,
        expected: Option<layerfs_storage::product::BranchHead>,
        resume: ResumeToken,
    ) -> Result<PreparedPushPage> {
        let mut object_ids = WorkingObjectPages::new(source, branch_id, expected);
        let transfer = push_objects_owned(
            source,
            destination,
            transfer_id,
            request_id,
            &mut object_ids,
            resume,
        )?;
        if let Some(error) = object_ids.error.take() {
            return Err(error);
        }
        let closure_traversal_ns = object_ids.traversal_ns;
        let history_export = Instant::now();
        let bundle = source
            .export_branch_push(branch_id, expected)
            .map_err(|error| SyncError::Source(error.to_string()))?;
        let history_export_ns = history_export.elapsed().as_nanos();
        Ok(PreparedPushPage {
            bundle,
            transfer,
            history_export_ns,
            closure_traversal_ns,
        })
    }

    fn merge_transfer_receipt(total: &mut TransferReceipt, page: TransferReceipt) -> Result<()> {
        for (target, value) in [
            (&mut total.objects_examined, page.objects_examined),
            (&mut total.known_present_objects, page.known_present_objects),
            (&mut total.missing_objects, page.missing_objects),
            (&mut total.transferred_objects, page.transferred_objects),
            (&mut total.unique_bytes, page.unique_bytes),
            (&mut total.resumed_bytes, page.resumed_bytes),
            (&mut total.retransmitted_bytes, page.retransmitted_bytes),
            (&mut total.batches, page.batches),
        ] {
            *target = add(*target, value)?;
        }
        total.largest_batch_bytes = total.largest_batch_bytes.max(page.largest_batch_bytes);
        total.largest_batch_objects = total.largest_batch_objects.max(page.largest_batch_objects);
        total.negotiation_ns = add_ns(total.negotiation_ns, page.negotiation_ns)?;
        total.source_read_ns = add_ns(total.source_read_ns, page.source_read_ns)?;
        total.receiver_admission_ns =
            add_ns(total.receiver_admission_ns, page.receiver_admission_ns)?;
        total.complete_wall_ns = add_ns(total.complete_wall_ns, page.complete_wall_ns)?;
        total.terminal_buffer_bytes = page.terminal_buffer_bytes;
        total.terminal_queued_batches = page.terminal_queued_batches;
        total.resume = page.resume;
        Ok(())
    }

    fn page_request_id(request: [u8; 32], direction: &[u8], generation: u64) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"layerfs.sync.history-page.v1\0");
        hasher.update(&request);
        hasher.update(direction);
        hasher.update(&generation.to_be_bytes());
        *hasher.finalize().as_bytes()
    }

    pub fn push_layer_stack_genesis(
        source: &WorkingStore,
        destination: &impl DurableControlEndpoint,
        request_id: [u8; 32],
        branch_id: BranchId,
        stack: LayerStackHead,
        name: &str,
        resume: ResumeToken,
    ) -> Result<PushLayerStackGenesisReceipt> {
        let complete = Instant::now();
        let mut object_ids = WorkingObjectPages::new(source, branch_id, None);
        let transfer = push_objects(source, destination, request_id, &mut object_ids, resume)?;
        if let Some(error) = object_ids.error.take() {
            return Err(error);
        }
        let closure_traversal_ns = object_ids.traversal_ns;
        let head_transaction = Instant::now();
        let head = destination.bootstrap_layer_stack(
            stack.layer_stack_id,
            stack.layer_id,
            name,
            stack.root,
        )?;
        destination.abort_transfer(RequestId::from_bytes(request_id), Direction::Push)?;
        source
            .clear_transfer_state(RequestId::from_bytes(request_id), "push")
            .map_err(|error| SyncError::Progress(error.to_string()))?;
        let head_transaction_ns = head_transaction.elapsed().as_nanos();
        if head != stack {
            return Err(SyncError::Destination(
                "LayerStack bootstrap head".to_owned(),
            ));
        }
        Ok(PushLayerStackGenesisReceipt {
            head,
            transfer,
            closure_traversal_ns,
            head_transaction_ns,
            complete_wall_ns: complete.elapsed().as_nanos(),
        })
    }

    struct WorkingObjectPages<'a> {
        source: &'a WorkingStore,
        branch_id: BranchId,
        base: Option<BranchHead>,
        after: Option<ObjectId>,
        page: std::vec::IntoIter<ObjectId>,
        done: bool,
        error: Option<SyncError>,
        traversal_ns: u128,
    }

    impl<'a> WorkingObjectPages<'a> {
        fn new(source: &'a WorkingStore, branch_id: BranchId, base: Option<BranchHead>) -> Self {
            Self {
                source,
                branch_id,
                base,
                after: None,
                page: Vec::new().into_iter(),
                done: false,
                error: None,
                traversal_ns: 0,
            }
        }
    }

    impl Iterator for WorkingObjectPages<'_> {
        type Item = ObjectId;

        fn next(&mut self) -> Option<Self::Item> {
            loop {
                if let Some(id) = self.page.next() {
                    return Some(id);
                }
                if self.done || self.error.is_some() {
                    return None;
                }
                let traversal = Instant::now();
                let page = self.source.branch_push_object_page(
                    self.branch_id,
                    self.base,
                    self.after,
                    MAX_BATCH_OBJECTS,
                );
                self.traversal_ns = match add_ns(self.traversal_ns, traversal.elapsed().as_nanos())
                {
                    Ok(total) => total,
                    Err(error) => {
                        self.error = Some(error);
                        return None;
                    }
                };
                let page = match page {
                    Ok(page) => page,
                    Err(error) => {
                        self.error = Some(SyncError::Source(error.to_string()));
                        return None;
                    }
                };
                if page.is_empty() {
                    self.done = true;
                    return None;
                }
                if page.len() > MAX_BATCH_OBJECTS
                    || page
                        .iter()
                        .scan(self.after, |previous, id| {
                            let ordered = previous.is_none_or(|previous| *id > previous);
                            *previous = Some(*id);
                            Some(ordered)
                        })
                        .any(|ordered| !ordered)
                {
                    self.error = Some(SyncError::Source("invalid Working closure page".to_owned()));
                    return None;
                }
                self.after = page.last().copied();
                self.page = page.into_iter();
            }
        }
    }

    pub fn fetch_branch(
        source: &impl DurableControlEndpoint,
        destination: &WorkingStore,
        request_id: [u8; 32],
        branch_id: BranchId,
        resume: ResumeToken,
    ) -> Result<FetchBranchReceipt> {
        fetch_branch_inner(
            source,
            destination,
            request_id,
            branch_id,
            resume,
            None,
            &mut std::collections::BTreeSet::new(),
        )
    }

    #[derive(Clone, Copy)]
    enum FetchStop {
        Version(layerfs_storage::product::VersionRef),
        Root(BranchId, ObjectId),
    }

    fn fetch_stop_satisfied(destination: &WorkingStore, stop: FetchStop) -> Result<bool> {
        match stop {
            FetchStop::Version(version) => Ok(destination.validate_version_ref(version).is_ok()),
            FetchStop::Root(branch, root) => destination
                .branch_contains_root(branch, root)
                .map_err(|error| SyncError::Destination(error.to_string())),
        }
    }

    fn fetch_branch_inner(
        source: &impl DurableControlEndpoint,
        destination: &WorkingStore,
        request_id: [u8; 32],
        branch_id: BranchId,
        mut resume: ResumeToken,
        stop: Option<FetchStop>,
        active: &mut std::collections::BTreeSet<BranchId>,
    ) -> Result<FetchBranchReceipt> {
        if !active.insert(branch_id) {
            return Err(SyncError::Source("Fetch Branch dependency cycle".into()));
        }
        let result = (|| {
            let complete = Instant::now();
            let mut base = destination
                .fetch_resume_branch_head(branch_id)
                .map_err(|error| SyncError::Destination(error.to_string()))?;
            let mut aggregate = None;
            let mut page = 0_u64;
            let mut origin_stack_base = None;
            loop {
                let page_request =
                    page_request_id(request_id, b"fetch", base.map_or(0, |head| head.generation));
                let receipt = fetch_branch_page(
                    source,
                    destination,
                    page_request,
                    branch_id,
                    base,
                    origin_stack_base,
                    resume,
                    active,
                )?;
                page = add(page, 1)?;
                base = Some(receipt.head);
                origin_stack_base = Some(receipt.origin_stack_head);
                let done = receipt.complete;
                aggregate_fetch_receipt(&mut aggregate, receipt)?;
                let stopped = stop
                    .map(|stop| fetch_stop_satisfied(destination, stop))
                    .transpose()?
                    .unwrap_or(false);
                if done || stopped {
                    let mut receipt = aggregate.ok_or(SyncError::CounterOverflow)?;
                    receipt.pages = page;
                    receipt.complete_wall_ns = complete.elapsed().as_nanos();
                    receipt.transfer.request_id = request_id;
                    if done {
                        if let Some(parent) = destination
                            .branch_parent(branch_id)
                            .map_err(|error| SyncError::Destination(error.to_string()))?
                            .filter(|parent| !active.contains(parent))
                        {
                            let dependency = fetch_branch_inner(
                                source,
                                destination,
                                dependency_request_id(request_id, parent),
                                parent,
                                ResumeToken::default(),
                                None,
                                active,
                            )?;
                            merge_dependency_fetch(&mut receipt, dependency)?;
                        }
                    }
                    return Ok(receipt);
                }
                resume = ResumeToken::default();
            }
        })();
        active.remove(&branch_id);
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn fetch_branch_page(
        source: &impl DurableControlEndpoint,
        destination: &WorkingStore,
        request_id: [u8; 32],
        branch_id: BranchId,
        base: Option<BranchHead>,
        origin_stack_base: Option<LayerStackHead>,
        resume: ResumeToken,
        active: &mut std::collections::BTreeSet<BranchId>,
    ) -> Result<FetchBranchReceipt> {
        let complete = Instant::now();
        let export = Instant::now();
        let mut bundle = source.export_branch_fetch(branch_id, base, origin_stack_base)?;
        let mut history_export_ns = export.elapsed().as_nanos();
        if origin_stack_base.is_none() {
            let local_stack = destination
                .fetch_resume_layer_stack_head(bundle.origin_stack.head.layer_stack_id)
                .map_err(|error| SyncError::Destination(error.to_string()))?;
            if local_stack.is_some() {
                let export = Instant::now();
                bundle = source.export_branch_fetch(branch_id, base, local_stack)?;
                history_export_ns = add_ns(history_export_ns, export.elapsed().as_nanos())?;
            }
        }
        let dependencies =
            ensure_fetch_dependencies(source, destination, request_id, &bundle, active)?;
        let local_stack = destination
            .fetch_resume_layer_stack_head(bundle.origin_stack.head.layer_stack_id)
            .map_err(|error| SyncError::Destination(error.to_string()))?;
        if local_stack != bundle.origin_stack.base {
            let export = Instant::now();
            bundle = source.export_branch_fetch(branch_id, base, local_stack)?;
            history_export_ns = add_ns(history_export_ns, export.elapsed().as_nanos())?;
        }
        let mut object_ids = BranchObjectPages::new(
            source,
            branch_id,
            base,
            bundle.origin_stack.base,
            bundle.head,
            bundle.origin_stack.head,
        );
        let transfer = fetch_objects(source, destination, request_id, &mut object_ids, resume)?;
        if let Some(error) = object_ids.error.take() {
            return Err(error);
        }
        let closure_traversal_ns = object_ids.traversal_ns;
        let terminal_object_page_entries = object_ids.page.len() as u64;
        let head_transaction = Instant::now();
        let head = destination
            .accept_verified_fetch(
                source.durable_storage_id(),
                RequestId::from_bytes(request_id),
                &bundle,
                SyncTransferCounters {
                    unique_bytes: transfer.unique_bytes,
                    resumed_bytes: transfer.resumed_bytes,
                    retransmitted_bytes: transfer.retransmitted_bytes,
                },
            )
            .map_err(|error| SyncError::Destination(error.to_string()))?;
        let head_transaction_ns = head_transaction.elapsed().as_nanos();
        let mut receipt = FetchBranchReceipt {
            head,
            origin_stack_head: bundle.origin_stack.head,
            transfer,
            dependency_transfer: None,
            history_export_ns,
            closure_traversal_ns,
            head_transaction_ns,
            complete_wall_ns: complete.elapsed().as_nanos(),
            terminal_object_page_entries,
            pages: 1,
            dependency_pages: 0,
            complete: bundle.complete,
        };
        for dependency in dependencies {
            merge_dependency_fetch(&mut receipt, dependency)?;
        }
        Ok(receipt)
    }

    fn ensure_fetch_dependencies(
        source: &impl DurableControlEndpoint,
        destination: &WorkingStore,
        request_id: [u8; 32],
        bundle: &BranchPushBundle,
        active: &mut std::collections::BTreeSet<BranchId>,
    ) -> Result<Vec<FetchBranchReceipt>> {
        let mut receipts = Vec::new();
        if let (Some(parent), Some(version)) = (
            bundle.ancestry.immediate_parent_branch_id,
            bundle.ancestry.fork_operation_version_id,
        ) {
            let retained = destination
                .validate_version_ref(layerfs_storage::product::VersionRef::OperationVersion {
                    branch_id: parent,
                    operation_version_id: version,
                    root: bundle.ancestry.fork_root,
                })
                .is_ok();
            if !retained {
                receipts.push(fetch_branch_inner(
                    source,
                    destination,
                    dependency_request_id(request_id, parent),
                    parent,
                    ResumeToken::default(),
                    Some(FetchStop::Version(
                        layerfs_storage::product::VersionRef::OperationVersion {
                            branch_id: parent,
                            operation_version_id: version,
                            root: bundle.ancestry.fork_root,
                        },
                    )),
                    active,
                )?);
            }
        }
        for (branch, root) in bundle
            .child_merges
            .iter()
            .map(|merge| (merge.source_branch_id, merge.source_root))
            .chain(bundle.origin_stack.layers.iter().filter_map(|layer| {
                layer
                    .merge
                    .as_ref()
                    .map(|merge| (merge.source_branch_id, merge.source_root))
            }))
        {
            if branch == bundle.head.branch_id {
                continue;
            }
            if !destination
                .branch_contains_root(branch, root)
                .map_err(|error| SyncError::Destination(error.to_string()))?
            {
                receipts.push(fetch_branch_inner(
                    source,
                    destination,
                    dependency_request_id(request_id, branch),
                    branch,
                    ResumeToken::default(),
                    Some(FetchStop::Root(branch, root)),
                    active,
                )?);
            }
        }
        Ok(receipts)
    }

    fn dependency_request_id(request: [u8; 32], branch: BranchId) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"layerfs.sync.fetch-dependency.v1\0");
        hasher.update(&request);
        hasher.update(branch.as_bytes());
        *hasher.finalize().as_bytes()
    }

    fn aggregate_fetch_receipt(
        aggregate: &mut Option<FetchBranchReceipt>,
        page: FetchBranchReceipt,
    ) -> Result<()> {
        let Some(total) = aggregate.as_mut() else {
            *aggregate = Some(page);
            return Ok(());
        };
        total.head = page.head;
        total.origin_stack_head = page.origin_stack_head;
        merge_transfer_receipt(&mut total.transfer, page.transfer)?;
        if let Some(dependency) = page.dependency_transfer {
            merge_dependency_transfer(&mut total.dependency_transfer, dependency)?;
        }
        total.dependency_pages = add(total.dependency_pages, page.dependency_pages)?;
        total.history_export_ns = add_ns(total.history_export_ns, page.history_export_ns)?;
        total.closure_traversal_ns = add_ns(total.closure_traversal_ns, page.closure_traversal_ns)?;
        total.head_transaction_ns = add_ns(total.head_transaction_ns, page.head_transaction_ns)?;
        total.terminal_object_page_entries = page.terminal_object_page_entries;
        total.complete = page.complete;
        Ok(())
    }

    fn merge_dependency_fetch(
        target: &mut FetchBranchReceipt,
        dependency: FetchBranchReceipt,
    ) -> Result<()> {
        target.dependency_pages = add(
            target.dependency_pages,
            add(dependency.pages, dependency.dependency_pages)?,
        )?;
        merge_dependency_transfer(&mut target.dependency_transfer, dependency.transfer)?;
        if let Some(nested) = dependency.dependency_transfer {
            merge_dependency_transfer(&mut target.dependency_transfer, nested)?;
        }
        Ok(())
    }

    fn merge_dependency_transfer(
        total: &mut Option<TransferReceipt>,
        dependency: TransferReceipt,
    ) -> Result<()> {
        match total {
            Some(total) => merge_transfer_receipt(total, dependency),
            None => {
                *total = Some(dependency);
                Ok(())
            }
        }
    }

    struct BranchObjectPages<'a, T> {
        source: &'a T,
        branch_id: BranchId,
        base: Option<BranchHead>,
        origin_stack_base: Option<LayerStackHead>,
        expected_head: BranchHead,
        expected_stack_head: LayerStackHead,
        after: Option<ObjectId>,
        page: std::vec::IntoIter<ObjectId>,
        done: bool,
        error: Option<SyncError>,
        traversal_ns: u128,
    }

    impl<'a, T: DurableControlEndpoint> BranchObjectPages<'a, T> {
        fn new(
            source: &'a T,
            branch_id: BranchId,
            base: Option<BranchHead>,
            origin_stack_base: Option<LayerStackHead>,
            expected_head: BranchHead,
            expected_stack_head: LayerStackHead,
        ) -> Self {
            Self {
                source,
                branch_id,
                base,
                origin_stack_base,
                expected_head,
                expected_stack_head,
                after: None,
                page: Vec::new().into_iter(),
                done: false,
                error: None,
                traversal_ns: 0,
            }
        }
    }

    impl<T: DurableControlEndpoint> Iterator for BranchObjectPages<'_, T> {
        type Item = ObjectId;

        fn next(&mut self) -> Option<Self::Item> {
            loop {
                if let Some(id) = self.page.next() {
                    return Some(id);
                }
                if self.done || self.error.is_some() {
                    return None;
                }
                let traversal = Instant::now();
                let page = self.source.branch_fetch_object_page(
                    self.branch_id,
                    self.base,
                    self.origin_stack_base,
                    self.expected_head,
                    self.expected_stack_head,
                    self.after,
                    MAX_BATCH_OBJECTS,
                );
                self.traversal_ns = match add_ns(self.traversal_ns, traversal.elapsed().as_nanos())
                {
                    Ok(total) => total,
                    Err(error) => {
                        self.error = Some(error);
                        return None;
                    }
                };
                let page = match page {
                    Ok(page) => page,
                    Err(error) => {
                        self.error = Some(error);
                        return None;
                    }
                };
                if page.is_empty() {
                    self.done = true;
                    return None;
                }
                if page.len() > MAX_BATCH_OBJECTS
                    || page
                        .iter()
                        .scan(self.after, |previous, id| {
                            let ordered = previous.is_none_or(|previous| *id > previous);
                            *previous = Some(*id);
                            Some(ordered)
                        })
                        .any(|ordered| !ordered)
                {
                    self.error = Some(SyncError::Source("invalid Durable closure page".to_owned()));
                    return None;
                }
                self.after = page.last().copied();
                self.page = page.into_iter();
            }
        }
    }

    pub fn push_child_branch_merge(
        destination: &impl DurableControlEndpoint,
        publication: ChildMergePublication,
    ) -> Result<ChildMergeOutcome> {
        destination.accept_child_branch_merge(publication)
    }

    pub fn push_branch_rollback(
        destination: &impl DurableControlEndpoint,
        publication: BranchRollbackPublication,
    ) -> Result<BranchRollbackOutcome> {
        destination.accept_branch_rollback(publication)
    }

    pub fn push_layer_stack_merge(
        destination: &impl DurableControlEndpoint,
        candidate: LayerCandidate,
        expected: LayerStackHead,
    ) -> Result<LayerStackMergeOutcome> {
        destination.accept_layer_stack_merge(candidate, expected)
    }

    pub fn push_layer_stack_rollback(
        destination: &impl DurableControlEndpoint,
        expected: LayerStackHead,
        target: LayerId,
    ) -> Result<LayerStackRollbackOutcome> {
        destination.layer_stack_rollback(expected, target)
    }
}

#[allow(clippy::too_many_arguments)]
fn transfer(
    source: &impl Source,
    destination: &impl Destination,
    progress: &WorkingStore,
    pin_owner: RequestId,
    request_id: [u8; 32],
    object_ids: impl IntoIterator<Item = ObjectId>,
    loaded: LoadedResume,
    direction: Direction,
) -> Result<TransferReceipt> {
    let complete = Instant::now();
    if source.storage_id() == destination.storage_id() {
        return Err(SyncError::SameStorage);
    }
    let resume = loaded.token;
    let mut receipt = TransferReceipt {
        request_id,
        source_storage_id: source.storage_id(),
        destination_storage_id: destination.storage_id(),
        direction,
        result: TransferResult::TransferredNoVisibility,
        resume,
        ..TransferReceipt::default_for(direction, request_id)
    };
    if let Some(previous) = &loaded.previous {
        receipt.unique_bytes = previous.counters.unique_bytes;
        receipt.resumed_bytes = previous.counters.resumed_bytes;
        receipt.retransmitted_bytes = previous.counters.retransmitted_bytes;
        receipt.batches = previous.batch_sequence;
    }
    if resume.next_object_index == 0 && resume.binding != [0; 32] {
        return Err(SyncError::InvalidResume);
    }
    let mut resume_hasher = blake3::Hasher::new();
    resume_hasher.update(b"layerfs.sync.resume.v1\0");
    resume_hasher.update(&request_id);
    resume_hasher.update(&source.storage_id());
    resume_hasher.update(&destination.storage_id());
    resume_hasher.update(&[match direction {
        Direction::Fetch => 0,
        Direction::Push => 1,
    }]);
    let mut resume_validated = resume.next_object_index == 0;
    let mut batch = Vec::new();
    let mut batch_bytes = 0_usize;
    let mut batch_unique_bytes = 0_u64;
    let mut batch_retransmitted_bytes = 0_u64;
    let mut batch_start = resume;
    let mut pending_seen = 0_usize;
    for (index, id) in object_ids.into_iter().enumerate() {
        let index = u64::try_from(index).map_err(|_| SyncError::CounterOverflow)?;
        if index < resume.next_object_index {
            resume_hasher.update(id.as_bytes());
            if add(index, 1)? == resume.next_object_index {
                if resume_hasher.clone().finalize().as_bytes() != &resume.binding {
                    return Err(SyncError::InvalidResume);
                }
                resume_validated = true;
            }
            continue;
        }
        let pending = usize::try_from(index - resume.next_object_index)
            .ok()
            .and_then(|index| loaded.pending.get(index).copied());
        if let Some(expected) = pending {
            if expected.id != id {
                return Err(SyncError::InvalidResume);
            }
            pending_seen = pending_seen
                .checked_add(1)
                .ok_or(SyncError::CounterOverflow)?;
        }
        receipt.objects_examined = add(receipt.objects_examined, 1)?;
        let negotiation = Instant::now();
        let present = destination.contains(id)?;
        receipt.negotiation_ns = add_ns(receipt.negotiation_ns, negotiation.elapsed().as_nanos())?;
        if present {
            receipt.known_present_objects = add(receipt.known_present_objects, 1)?;
            advance_resume(&mut receipt, &mut resume_hasher, id, add(index, 1)?);
            continue;
        }
        receipt.missing_objects = add(receipt.missing_objects, 1)?;
        let source_read = Instant::now();
        let canonical = source.read(id, MAX_BATCH_BYTES)?;
        receipt.source_read_ns = add_ns(receipt.source_read_ns, source_read.elapsed().as_nanos())?;
        let canonical_bytes =
            u64::try_from(canonical.len()).map_err(|_| SyncError::CounterOverflow)?;
        if pending.is_some_and(|expected| expected.bytes != canonical_bytes) {
            return Err(SyncError::InvalidResume);
        }
        if canonical.len() > MAX_BATCH_BYTES {
            return Err(SyncError::ResourceExhausted);
        }
        if !batch.is_empty()
            && (batch.len() == MAX_BATCH_OBJECTS
                || batch_bytes
                    .checked_add(canonical.len())
                    .is_none_or(|bytes| bytes > MAX_BATCH_BYTES))
        {
            flush(
                destination,
                progress,
                pin_owner,
                &mut receipt,
                &mut batch,
                &mut batch_bytes,
                &mut batch_unique_bytes,
                &mut batch_retransmitted_bytes,
                batch_start,
                loaded.previous.is_some() || resume.next_object_index != 0,
            )?;
        }
        if batch.is_empty() {
            batch_start = receipt.resume;
        }
        batch_bytes = batch_bytes
            .checked_add(canonical.len())
            .ok_or(SyncError::CounterOverflow)?;
        batch.push((id, canonical));
        if pending.is_some() {
            batch_retransmitted_bytes = add(batch_retransmitted_bytes, canonical_bytes)?;
        } else {
            batch_unique_bytes = add(batch_unique_bytes, canonical_bytes)?;
        }
        advance_resume(&mut receipt, &mut resume_hasher, id, add(index, 1)?);
    }
    if !resume_validated || pending_seen != loaded.pending.len() {
        return Err(SyncError::InvalidResume);
    }
    flush(
        destination,
        progress,
        pin_owner,
        &mut receipt,
        &mut batch,
        &mut batch_bytes,
        &mut batch_unique_bytes,
        &mut batch_retransmitted_bytes,
        batch_start,
        loaded.previous.is_some() || resume.next_object_index != 0,
    )?;
    record_progress(progress, pin_owner, &receipt, true, receipt.resume, &[])?;
    receipt.complete_wall_ns = complete.elapsed().as_nanos();
    receipt.terminal_buffer_bytes = batch_bytes as u64;
    receipt.terminal_queued_batches = u64::from(!batch.is_empty());
    Ok(receipt)
}

fn advance_resume(
    receipt: &mut TransferReceipt,
    hasher: &mut blake3::Hasher,
    id: ObjectId,
    next_object_index: u64,
) {
    hasher.update(id.as_bytes());
    receipt.resume.next_object_index = next_object_index;
    receipt.resume.binding = *hasher.clone().finalize().as_bytes();
}

#[allow(clippy::too_many_arguments)]
fn flush(
    destination: &impl Destination,
    progress: &WorkingStore,
    pin_owner: RequestId,
    receipt: &mut TransferReceipt,
    batch: &mut Vec<(ObjectId, Vec<u8>)>,
    batch_bytes: &mut usize,
    batch_unique_bytes: &mut u64,
    batch_retransmitted_bytes: &mut u64,
    batch_start: ResumeToken,
    resumed_attempt: bool,
) -> Result<()> {
    if batch.is_empty() {
        return Ok(());
    }
    let bytes = u64::try_from(*batch_bytes).map_err(|_| SyncError::CounterOverflow)?;
    let objects = u64::try_from(batch.len()).map_err(|_| SyncError::CounterOverflow)?;
    receipt.unique_bytes = add(receipt.unique_bytes, *batch_unique_bytes)?;
    receipt.retransmitted_bytes = add(receipt.retransmitted_bytes, *batch_retransmitted_bytes)?;
    if resumed_attempt {
        receipt.resumed_bytes = add(receipt.resumed_bytes, bytes)?;
    }
    receipt.transferred_objects = add(receipt.transferred_objects, objects)?;
    receipt.batches = add(receipt.batches, 1)?;
    receipt.largest_batch_bytes = receipt.largest_batch_bytes.max(bytes);
    receipt.largest_batch_objects = receipt.largest_batch_objects.max(objects);
    let admission = Instant::now();
    let accepted = destination.accept(
        pin_owner,
        RequestId::from_bytes(receipt.request_id),
        receipt.direction,
        batch,
    );
    receipt.receiver_admission_ns = add_ns(
        receipt.receiver_admission_ns,
        admission.elapsed().as_nanos(),
    )?;
    if let Err(error) = accepted {
        let pending = batch
            .iter()
            .map(|(id, canonical)| PendingObject {
                id: *id,
                bytes: canonical.len() as u64,
            })
            .collect::<Vec<_>>();
        record_progress(progress, pin_owner, receipt, false, batch_start, &pending)?;
        return Err(error);
    }
    record_progress(progress, pin_owner, receipt, false, receipt.resume, &[])?;
    batch.clear();
    *batch_bytes = 0;
    *batch_unique_bytes = 0;
    *batch_retransmitted_bytes = 0;
    Ok(())
}

fn load_resume(
    progress: &WorkingStore,
    request_id: [u8; 32],
    direction: Direction,
    requested: ResumeToken,
) -> Result<LoadedResume> {
    let previous = progress
        .latest_transfer_state(RequestId::from_bytes(request_id), direction_name(direction))
        .map_err(|error| SyncError::Progress(error.to_string()))?;
    let Some(previous) = previous else {
        return Ok(LoadedResume {
            token: requested,
            previous: None,
            pending: Vec::new(),
        });
    };
    let persisted = ResumeToken::decode(&previous.cursor)?;
    if requested != ResumeToken::default() && requested != persisted {
        return Err(SyncError::InvalidResume);
    }
    let pending = decode_pending(&previous.cursor)?;
    Ok(LoadedResume {
        token: persisted,
        previous: Some(previous),
        pending,
    })
}

fn record_progress(
    progress: &WorkingStore,
    owner_request_id: RequestId,
    receipt: &TransferReceipt,
    complete: bool,
    cursor: ResumeToken,
    pending: &[PendingObject],
) -> Result<()> {
    let encoded = encode_progress_cursor(cursor, pending)?;
    progress
        .record_transfer_state(
            owner_request_id,
            RequestId::from_bytes(receipt.request_id),
            receipt.batches,
            direction_name(receipt.direction),
            &encoded,
            complete,
            SyncTransferCounters {
                unique_bytes: receipt.unique_bytes,
                resumed_bytes: receipt.resumed_bytes,
                retransmitted_bytes: receipt.retransmitted_bytes,
            },
        )
        .map_err(|error| SyncError::Progress(error.to_string()))?;
    Ok(())
}

fn encode_progress_cursor(token: ResumeToken, pending: &[PendingObject]) -> Result<Vec<u8>> {
    if pending.len() > MAX_BATCH_OBJECTS {
        return Err(SyncError::ResourceExhausted);
    }
    let mut encoded = Vec::with_capacity(48 + pending.len() * 40);
    encoded.extend_from_slice(&token.encode());
    if !pending.is_empty() {
        encoded.extend_from_slice(b"PND1");
        encoded.extend_from_slice(
            &u32::try_from(pending.len())
                .map_err(|_| SyncError::CounterOverflow)?
                .to_be_bytes(),
        );
        for object in pending {
            encoded.extend_from_slice(object.id.as_bytes());
            encoded.extend_from_slice(&object.bytes.to_be_bytes());
        }
    }
    Ok(encoded)
}

fn decode_pending(encoded: &[u8]) -> Result<Vec<PendingObject>> {
    if encoded.len() == 40 {
        return Ok(Vec::new());
    }
    if encoded.len() < 48 || &encoded[40..44] != b"PND1" {
        return Err(SyncError::InvalidResume);
    }
    let mut count = [0; 4];
    count.copy_from_slice(&encoded[44..48]);
    let count =
        usize::try_from(u32::from_be_bytes(count)).map_err(|_| SyncError::CounterOverflow)?;
    if count > MAX_BATCH_OBJECTS || encoded.len() != 48 + count * 40 {
        return Err(SyncError::InvalidResume);
    }
    let mut pending = Vec::with_capacity(count);
    for chunk in encoded[48..].chunks_exact(40) {
        let mut id = [0; 32];
        id.copy_from_slice(&chunk[..32]);
        let mut bytes = [0; 8];
        bytes.copy_from_slice(&chunk[32..]);
        pending.push(PendingObject {
            id: ObjectId::from_bytes(&id).map_err(|_| SyncError::InvalidResume)?,
            bytes: u64::from_be_bytes(bytes),
        });
    }
    Ok(pending)
}

const fn direction_name(direction: Direction) -> &'static str {
    match direction {
        Direction::Fetch => "fetch",
        Direction::Push => "push",
    }
}

impl TransferReceipt {
    fn default_for(direction: Direction, request_id: [u8; 32]) -> Self {
        Self {
            request_id,
            source_storage_id: [0; 32],
            destination_storage_id: [0; 32],
            direction,
            result: TransferResult::TransferredNoVisibility,
            objects_examined: 0,
            known_present_objects: 0,
            missing_objects: 0,
            transferred_objects: 0,
            unique_bytes: 0,
            resumed_bytes: 0,
            retransmitted_bytes: 0,
            batches: 0,
            largest_batch_bytes: 0,
            largest_batch_objects: 0,
            negotiation_ns: 0,
            source_read_ns: 0,
            receiver_admission_ns: 0,
            complete_wall_ns: 0,
            terminal_buffer_bytes: 0,
            terminal_queued_batches: 0,
            resume: ResumeToken::default(),
        }
    }
}

fn add(left: u64, right: u64) -> Result<u64> {
    left.checked_add(right).ok_or(SyncError::CounterOverflow)
}

fn add_ns(left: u128, right: u128) -> Result<u128> {
    left.checked_add(right).ok_or(SyncError::CounterOverflow)
}
