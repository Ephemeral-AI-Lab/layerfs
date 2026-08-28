use super::negotiate::Destination;
use crate::{
    Direction, DurableEndpoint, RequestId, Result, ResumeToken, SyncError, SyncTransferCounters,
    TransferReceipt, MAX_BATCH_OBJECTS,
};
use layerfs_core::ObjectId;
use layerfs_working_store::WorkingStore;
use std::time::Instant;

#[derive(Clone, Copy)]
pub(crate) struct PendingObject {
    pub(crate) id: ObjectId,
    pub(crate) bytes: u64,
}

pub(crate) struct LoadedResume {
    pub(crate) token: ResumeToken,
    pub(crate) previous: Option<layerfs_storage::StoredTransferState>,
    pub(crate) pending: Vec<PendingObject>,
}

pub fn abort_push_transfer(
    destination: &impl DurableEndpoint,
    owner_request_id: [u8; 32],
) -> Result<u64> {
    destination.abort_transfer(RequestId::from_bytes(owner_request_id), Direction::Push)
}

pub fn abort_fetch_transfer(destination: &WorkingStore, owner_request_id: [u8; 32]) -> Result<u64> {
    destination
        .abort_sync_transfer(RequestId::from_bytes(owner_request_id), "fetch")
        .map_err(|error| SyncError::Destination(error.to_string()))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn flush(
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
    receipt.unique_bytes = crate::types::add(receipt.unique_bytes, *batch_unique_bytes)?;
    receipt.retransmitted_bytes =
        crate::types::add(receipt.retransmitted_bytes, *batch_retransmitted_bytes)?;
    if resumed_attempt {
        receipt.resumed_bytes = crate::types::add(receipt.resumed_bytes, bytes)?;
    }
    receipt.transferred_objects = crate::types::add(receipt.transferred_objects, objects)?;
    receipt.batches = crate::types::add(receipt.batches, 1)?;
    receipt.largest_batch_bytes = receipt.largest_batch_bytes.max(bytes);
    receipt.largest_batch_objects = receipt.largest_batch_objects.max(objects);
    let admission = Instant::now();
    let accepted = destination.accept(
        pin_owner,
        RequestId::from_bytes(receipt.request_id),
        receipt.direction,
        batch,
    );
    receipt.receiver_admission_ns = crate::types::add_ns(
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

pub(crate) fn load_resume(
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

pub(crate) fn record_progress(
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
