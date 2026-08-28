use super::negotiate::{Destination, EndpointDestination, EndpointSource, Source};
use super::receive::{flush, load_resume, record_progress, LoadedResume};
use crate::{
    Direction, DurableEndpoint, RequestId, Result, ResumeToken, SyncError, TransferReceipt,
    TransferResult, MAX_BATCH_BYTES, MAX_BATCH_OBJECTS,
};
use layerfs_core::ObjectId;
use layerfs_working_store::WorkingStore;
use std::time::Instant;

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

pub(crate) fn push_objects_owned(
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
            if crate::types::add(index, 1)? == resume.next_object_index {
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
        receipt.objects_examined = crate::types::add(receipt.objects_examined, 1)?;
        let negotiation = Instant::now();
        let present = destination.contains(id)?;
        receipt.negotiation_ns =
            crate::types::add_ns(receipt.negotiation_ns, negotiation.elapsed().as_nanos())?;
        if present {
            receipt.known_present_objects = crate::types::add(receipt.known_present_objects, 1)?;
            advance_resume(
                &mut receipt,
                &mut resume_hasher,
                id,
                crate::types::add(index, 1)?,
            );
            continue;
        }
        receipt.missing_objects = crate::types::add(receipt.missing_objects, 1)?;
        let source_read = Instant::now();
        let canonical = source.read(id, MAX_BATCH_BYTES)?;
        receipt.source_read_ns =
            crate::types::add_ns(receipt.source_read_ns, source_read.elapsed().as_nanos())?;
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
            batch_retransmitted_bytes =
                crate::types::add(batch_retransmitted_bytes, canonical_bytes)?;
        } else {
            batch_unique_bytes = crate::types::add(batch_unique_bytes, canonical_bytes)?;
        }
        advance_resume(
            &mut receipt,
            &mut resume_hasher,
            id,
            crate::types::add(index, 1)?,
        );
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
