//! Bounded receipts for an original owner operation, retained with its result.
/// SQL includes readiness and attempted publication/rollback work, attributed
/// to its actual statement families. Allocation request bytes include repeated
/// precise Linux range calls and are not newly consumed disk. A parked
/// readiness check has no attempted mutation to replay.
#[derive(Debug, Default)]
pub struct JobWork {
    pub sql: crate::JobSql,
    pub payload: layerfs_overlay::PayloadWork,
    pub allocation: layerfs_overlay::AllocationWork,
    pub parked_turns: u64,
    /// Admission to final runnable turn; includes readiness/parking wait.
    pub queue_wait_ns: u64,
    /// Final runnable command span; overlaps inclusive SQL spans.
    pub service_ns: u64,
}
