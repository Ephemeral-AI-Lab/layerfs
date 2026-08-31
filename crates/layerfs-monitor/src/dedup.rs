use crate::{CandidateStats, MonitorError, OperationReceipt};
use layerfs_layerstack_store::LayerStackStore;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CandidateTotals {
    pub candidate_objects: u64,
    pub candidate_bytes: u64,
    pub inserted_objects: u64,
    pub inserted_bytes: u64,
    pub reused_objects: u64,
    pub reused_bytes: u64,
    pub batch_inserted_objects: u64,
    pub batch_inserted_bytes: u64,
    pub final_inserted_objects: u64,
    pub final_inserted_bytes: u64,
    pub preexisting_reused_objects: u64,
    pub preexisting_reused_bytes: u64,
    pub admission_transactions: u64,
    pub max_transaction_objects: u64,
    pub max_transaction_bytes: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DedupAnalysis {
    pub physical_objects: u64,
    pub physical_bytes: u64,
    pub reachable_objects: u64,
    pub reachable_bytes: u64,
    pub candidates: CandidateTotals,
    pub saved_fraction: Option<f64>,
    pub logical_to_physical_ratio: Option<f64>,
    pub unreachable_objects: u64,
    pub unreachable_bytes: u64,
}

pub(crate) fn analyze(
    store: &LayerStackStore,
    receipts: &[OperationReceipt],
) -> Result<DedupAnalysis, MonitorError> {
    let physical = store.canonical_storage()?;
    let reachable = store.reachable_storage()?;
    if reachable.objects > physical.objects || reachable.encoded_bytes > physical.encoded_bytes {
        return Err(MonitorError::Integrity("reachable canonical storage"));
    }
    let candidates = receipts
        .iter()
        .filter_map(|receipt| receipt.candidate)
        .try_fold(CandidateTotals::default(), |mut total, candidate| {
            if !candidate.validate() {
                return Err(MonitorError::Integrity("candidate equation"));
            }
            add_candidate(&mut total, candidate);
            Ok(total)
        })?;
    Ok(DedupAnalysis {
        physical_objects: physical.objects,
        physical_bytes: physical.encoded_bytes,
        reachable_objects: reachable.objects,
        reachable_bytes: reachable.encoded_bytes,
        candidates,
        saved_fraction: (candidates.candidate_bytes != 0)
            .then(|| candidates.reused_bytes as f64 / candidates.candidate_bytes as f64),
        logical_to_physical_ratio: None,
        unreachable_objects: physical.objects - reachable.objects,
        unreachable_bytes: physical.encoded_bytes - reachable.encoded_bytes,
    })
}

fn add_candidate(total: &mut CandidateTotals, candidate: CandidateStats) {
    total.candidate_objects = total
        .candidate_objects
        .saturating_add(candidate.candidate_objects);
    total.candidate_bytes = total
        .candidate_bytes
        .saturating_add(candidate.candidate_bytes);
    total.inserted_objects = total
        .inserted_objects
        .saturating_add(candidate.inserted_objects);
    total.inserted_bytes = total
        .inserted_bytes
        .saturating_add(candidate.inserted_bytes);
    total.reused_objects = total
        .reused_objects
        .saturating_add(candidate.reused_objects);
    total.reused_bytes = total.reused_bytes.saturating_add(candidate.reused_bytes);
    total.batch_inserted_objects = total
        .batch_inserted_objects
        .saturating_add(candidate.batch_inserted_objects);
    total.batch_inserted_bytes = total
        .batch_inserted_bytes
        .saturating_add(candidate.batch_inserted_bytes);
    total.final_inserted_objects = total
        .final_inserted_objects
        .saturating_add(candidate.final_inserted_objects);
    total.final_inserted_bytes = total
        .final_inserted_bytes
        .saturating_add(candidate.final_inserted_bytes);
    total.preexisting_reused_objects = total
        .preexisting_reused_objects
        .saturating_add(candidate.preexisting_reused_objects);
    total.preexisting_reused_bytes = total
        .preexisting_reused_bytes
        .saturating_add(candidate.preexisting_reused_bytes);
    total.admission_transactions = total
        .admission_transactions
        .saturating_add(candidate.admission_transactions);
    total.max_transaction_objects = total
        .max_transaction_objects
        .max(candidate.max_transaction_objects);
    total.max_transaction_bytes = total
        .max_transaction_bytes
        .max(candidate.max_transaction_bytes);
}
