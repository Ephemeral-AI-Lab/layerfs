//! Exact storage deltas and elapsed lock work of one short operation.
#[derive(Debug, Default)]
pub struct JobWork {
    /// Actual connection jobs represented by this receipt (compositions sum them).
    pub jobs: u64,
    pub sql: layerfs_overlay::DatabaseWork,
    pub payload: layerfs_overlay::PayloadWork,
    pub allocation: layerfs_overlay::AllocationWork,
    pub lock_wait_ns: u64,
    pub hold_ns: u64,
}
