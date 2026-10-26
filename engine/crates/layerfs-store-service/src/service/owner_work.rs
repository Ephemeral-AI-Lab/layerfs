//! Actual short jobs and storage work; no scheduling or memory-budget gauges.
#[derive(Clone, Copy, Debug, Default)]
pub struct OwnerWork {
    pub sql_foreground: layerfs_overlay::DatabaseWork,
    pub sql_maintenance: layerfs_overlay::DatabaseWork,
    pub payload_foreground: layerfs_overlay::PayloadWork,
    pub payload_maintenance: layerfs_overlay::PayloadWork,
    pub allocation_foreground: layerfs_overlay::AllocationWork,
    pub allocation_maintenance: layerfs_overlay::AllocationWork,
    pub jobs: [u64; 3],
    pub lock_wait_ns: [u64; 3],
    pub hold_ns: [u64; 3],
    /// Holds of a Workspace lock that only read these counters: no SQL and
    /// no job of any class, so an observation never changes what it observes.
    pub observations: u64,
    /// Reply attempts: each validates its ticket and wakes a waiter under the
    /// Workspace lock, runs no SQL and is no job of any class.
    pub reply_attempts: u64,
    pub reply_hold_ns: u64,
    pub maintenance_jobs: u64,
    pub maintenance_rows: u64,
    pub closed_namespaces: u64,
    pub maintenance_data_bytes: u64,
    pub maintenance_ns: u64,
}
impl OwnerWork {
    pub fn accumulate(&mut self, next: &Self) {
        self.sql_foreground.accumulate(next.sql_foreground);
        self.sql_maintenance.accumulate(next.sql_maintenance);
        self.payload_foreground.accumulate(next.payload_foreground);
        self.payload_maintenance
            .accumulate(next.payload_maintenance);
        self.allocation_foreground
            .accumulate(next.allocation_foreground);
        self.allocation_maintenance
            .accumulate(next.allocation_maintenance);
        for index in 0..3 {
            self.jobs[index] = self.jobs[index].saturating_add(next.jobs[index]);
            self.lock_wait_ns[index] =
                self.lock_wait_ns[index].saturating_add(next.lock_wait_ns[index]);
            self.hold_ns[index] = self.hold_ns[index].saturating_add(next.hold_ns[index]);
        }
        self.observations = self.observations.saturating_add(next.observations);
        self.reply_attempts = self.reply_attempts.saturating_add(next.reply_attempts);
        self.reply_hold_ns = self.reply_hold_ns.saturating_add(next.reply_hold_ns);
        self.maintenance_jobs = self.maintenance_jobs.saturating_add(next.maintenance_jobs);
        self.maintenance_rows = self.maintenance_rows.saturating_add(next.maintenance_rows);
        self.closed_namespaces = self
            .closed_namespaces
            .saturating_add(next.closed_namespaces);
        self.maintenance_data_bytes = self
            .maintenance_data_bytes
            .saturating_add(next.maintenance_data_bytes);
        self.maintenance_ns = self.maintenance_ns.saturating_add(next.maintenance_ns);
    }
}
