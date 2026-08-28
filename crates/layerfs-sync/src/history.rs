use crate::object_transfer::{push_objects_owned, WorkingObjectPages};
use crate::{
    BranchHead, BranchId, BranchPushBundle, DurableControlEndpoint, FetchBranchReceipt, RequestId,
    Result, ResumeToken, SyncError, SyncTransferCounters, TransferReceipt,
};
use layerfs_core::ObjectId;
use layerfs_working_store::WorkingStore;
use std::collections::BTreeSet;
use std::time::Instant;

pub(crate) struct StagedPushPageReceipt {
    pub(crate) head: BranchHead,
    pub(crate) page_digest: [u8; 32],
    pub(crate) transfer: TransferReceipt,
    pub(crate) history_export_ns: u128,
    pub(crate) closure_traversal_ns: u128,
    pub(crate) staging_ns: u128,
    pub(crate) complete: bool,
}

pub(crate) struct PreparedPushPage {
    pub(crate) bundle: BranchPushBundle,
    pub(crate) transfer: TransferReceipt,
    pub(crate) history_export_ns: u128,
    pub(crate) closure_traversal_ns: u128,
}

#[derive(Clone, Copy)]
pub(crate) enum FetchStop {
    Version(layerfs_storage::VersionRef),
    Root(BranchId, ObjectId),
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn stage_branch_push_page(
    source: &WorkingStore,
    destination: &impl DurableControlEndpoint,
    transfer_id: RequestId,
    page_sequence: u64,
    request_id: [u8; 32],
    branch_id: BranchId,
    expected: Option<BranchHead>,
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
            let dependency = crate::publication::push_branch(
                source,
                destination,
                dependency_request_id(*transfer_id.as_bytes(), merge.source_branch_id),
                merge.source_branch_id,
                durable_head,
                ResumeToken::default(),
            )?;
            if !matches!(
                dependency.outcome,
                crate::BranchPushOutcome::DurablyAccepted { head, .. } if head == source_head
            ) {
                return Err(SyncError::Destination(
                    "Child merge source Branch Push conflict".into(),
                ));
            }
        }
    }
    let staging = Instant::now();
    let counters = SyncTransferCounters {
        unique_bytes: transfer.unique_bytes,
        resumed_bytes: transfer.resumed_bytes,
        retransmitted_bytes: transfer.retransmitted_bytes,
    };
    let page_digest = layerfs_storage::branch_push_bundle_page_digest(
        transfer_id,
        page_sequence,
        RequestId::from_bytes(request_id),
        &bundle,
        counters,
    )
    .map_err(|error| SyncError::Source(error.to_string()))?;
    destination.stage_branch_push_page(
        transfer_id,
        page_sequence,
        RequestId::from_bytes(request_id),
        &bundle,
        counters,
    )?;
    Ok(StagedPushPageReceipt {
        head: bundle.head,
        page_digest,
        transfer,
        history_export_ns,
        closure_traversal_ns,
        staging_ns: staging.elapsed().as_nanos(),
        complete: bundle.complete,
    })
}

pub(crate) fn prepare_branch_push_page(
    source: &WorkingStore,
    destination: &impl DurableControlEndpoint,
    transfer_id: RequestId,
    request_id: [u8; 32],
    branch_id: BranchId,
    expected: Option<BranchHead>,
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
    Ok(PreparedPushPage {
        bundle,
        transfer,
        history_export_ns: history_export.elapsed().as_nanos(),
        closure_traversal_ns,
    })
}

pub(crate) fn merge_transfer_receipt(
    total: &mut TransferReceipt,
    page: TransferReceipt,
) -> Result<()> {
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
        *target = crate::types::add(*target, value)?;
    }
    total.largest_batch_bytes = total.largest_batch_bytes.max(page.largest_batch_bytes);
    total.largest_batch_objects = total.largest_batch_objects.max(page.largest_batch_objects);
    total.negotiation_ns = crate::types::add_ns(total.negotiation_ns, page.negotiation_ns)?;
    total.source_read_ns = crate::types::add_ns(total.source_read_ns, page.source_read_ns)?;
    total.receiver_admission_ns =
        crate::types::add_ns(total.receiver_admission_ns, page.receiver_admission_ns)?;
    total.complete_wall_ns = crate::types::add_ns(total.complete_wall_ns, page.complete_wall_ns)?;
    total.terminal_buffer_bytes = page.terminal_buffer_bytes;
    total.terminal_queued_batches = page.terminal_queued_batches;
    total.resume = page.resume;
    Ok(())
}

pub(crate) fn page_request_id(request: [u8; 32], direction: &[u8], generation: u64) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs.sync.history-page.v1\0");
    hasher.update(&request);
    hasher.update(direction);
    hasher.update(&generation.to_be_bytes());
    *hasher.finalize().as_bytes()
}

pub(crate) fn dependency_request_id(request: [u8; 32], branch: BranchId) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs.sync.fetch-dependency.v1\0");
    hasher.update(&request);
    hasher.update(branch.as_bytes());
    *hasher.finalize().as_bytes()
}

pub(crate) fn fetch_stop_satisfied(destination: &WorkingStore, stop: FetchStop) -> Result<bool> {
    match stop {
        FetchStop::Version(version) => Ok(destination.validate_version_ref(version).is_ok()),
        FetchStop::Root(branch, root) => destination
            .branch_contains_root(branch, root)
            .map_err(|error| SyncError::Destination(error.to_string())),
    }
}

pub(crate) fn ensure_fetch_dependencies(
    source: &impl DurableControlEndpoint,
    destination: &WorkingStore,
    request_id: [u8; 32],
    bundle: &BranchPushBundle,
    active: &mut BTreeSet<BranchId>,
) -> Result<Vec<FetchBranchReceipt>> {
    let mut receipts = Vec::new();
    if let (Some(parent), Some(version)) = (
        bundle.ancestry.immediate_parent_branch_id,
        bundle.ancestry.fork_operation_version_id,
    ) {
        let version = layerfs_storage::VersionRef::OperationVersion {
            branch_id: parent,
            operation_version_id: version,
            root: bundle.ancestry.fork_root,
        };
        if destination.validate_version_ref(version).is_err() {
            receipts.push(crate::checkout::fetch_branch_inner(
                source,
                destination,
                dependency_request_id(request_id, parent),
                parent,
                ResumeToken::default(),
                Some(FetchStop::Version(version)),
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
        if branch != bundle.head.branch_id
            && !destination
                .branch_contains_root(branch, root)
                .map_err(|error| SyncError::Destination(error.to_string()))?
        {
            receipts.push(crate::checkout::fetch_branch_inner(
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

pub(crate) fn aggregate_fetch_receipt(
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
    total.dependency_pages = crate::types::add(total.dependency_pages, page.dependency_pages)?;
    total.history_export_ns =
        crate::types::add_ns(total.history_export_ns, page.history_export_ns)?;
    total.closure_traversal_ns =
        crate::types::add_ns(total.closure_traversal_ns, page.closure_traversal_ns)?;
    total.head_transaction_ns =
        crate::types::add_ns(total.head_transaction_ns, page.head_transaction_ns)?;
    total.terminal_object_page_entries = page.terminal_object_page_entries;
    total.complete = page.complete;
    Ok(())
}

pub(crate) fn merge_dependency_fetch(
    target: &mut FetchBranchReceipt,
    dependency: FetchBranchReceipt,
) -> Result<()> {
    target.dependency_pages = crate::types::add(
        target.dependency_pages,
        crate::types::add(dependency.pages, dependency.dependency_pages)?,
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
