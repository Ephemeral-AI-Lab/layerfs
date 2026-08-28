use crate::history::{
    aggregate_fetch_receipt, ensure_fetch_dependencies, fetch_stop_satisfied,
    merge_dependency_fetch, page_request_id, FetchStop,
};
use crate::object_transfer::{fetch_objects, BranchObjectPages};
use crate::{
    BranchId, DurableControlEndpoint, FetchBranchReceipt, RequestId, Result, ResumeToken,
    SyncError, SyncTransferCounters,
};
use layerfs_working_store::WorkingStore;
use std::collections::BTreeSet;
use std::time::Instant;

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
        &mut BTreeSet::new(),
    )
}

pub(crate) fn fetch_branch_inner(
    source: &impl DurableControlEndpoint,
    destination: &WorkingStore,
    request_id: [u8; 32],
    branch_id: BranchId,
    mut resume: ResumeToken,
    stop: Option<FetchStop>,
    active: &mut BTreeSet<BranchId>,
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
            page = crate::types::add(page, 1)?;
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
                            crate::history::dependency_request_id(request_id, parent),
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
    base: Option<crate::BranchHead>,
    origin_stack_base: Option<crate::LayerStackHead>,
    resume: ResumeToken,
    active: &mut BTreeSet<BranchId>,
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
            history_export_ns =
                crate::types::add_ns(history_export_ns, export.elapsed().as_nanos())?;
        }
    }
    let dependencies = ensure_fetch_dependencies(source, destination, request_id, &bundle, active)?;
    let local_stack = destination
        .fetch_resume_layer_stack_head(bundle.origin_stack.head.layer_stack_id)
        .map_err(|error| SyncError::Destination(error.to_string()))?;
    if local_stack != bundle.origin_stack.base {
        let export = Instant::now();
        bundle = source.export_branch_fetch(branch_id, base, local_stack)?;
        history_export_ns = crate::types::add_ns(history_export_ns, export.elapsed().as_nanos())?;
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
    let mut receipt = FetchBranchReceipt {
        head,
        origin_stack_head: bundle.origin_stack.head,
        transfer,
        dependency_transfer: None,
        history_export_ns,
        closure_traversal_ns,
        head_transaction_ns: head_transaction.elapsed().as_nanos(),
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
