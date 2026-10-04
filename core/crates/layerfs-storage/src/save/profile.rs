//! Selection observations shared by codecs and the bounded save owner.
use std::time::Instant;
/// Codec and candidate selection measurements; nested spans are inclusive.
#[derive(Clone, Copy, Debug, Default)]
pub struct SaveProfile {
    /// FULL encoding wall.
    pub full_ns: u64,
    /// Delta trials wall.
    pub delta_ns: u64,
    /// Group codec/framing wall, including pooled value-group compression.
    pub group_ns: u64,
    /// Non-native records selected.
    pub stored_records: u64,
    /// Dependency selection work.
    pub resolve: ResolveProfile,
    /// Codec probe work.
    pub diag: DiagnosticProfile,
}
/// Dependency selection measurements.
#[derive(Clone, Copy, Debug, Default)]
pub struct ResolveProfile {
    /// Cost reconstruction wall.
    pub cost_ns: u64,
    /// Candidate eligibility wall.
    pub eligible_ns: u64,
    /// Candidate acquisition wall.
    pub acquire_ns: u64,
}
/// Codec probe observations.
#[derive(Clone, Copy, Debug, Default)]
pub struct DiagnosticProfile {
    /// Bounded compression probe wall.
    pub probe_ns: u64,
}
impl SaveProfile {
    pub(crate) fn accumulate(&mut self, other: Self) {
        self.full_ns += other.full_ns;
        self.delta_ns += other.delta_ns;
        self.group_ns += other.group_ns;
        self.stored_records += other.stored_records;
        self.resolve.cost_ns += other.resolve.cost_ns;
        self.resolve.eligible_ns += other.resolve.eligible_ns;
        self.resolve.acquire_ns += other.resolve.acquire_ns;
        self.diag.probe_ns += other.diag.probe_ns;
    }

    /// Charges an inclusive elapsed span.
    pub fn charge(slot: &mut u64, start: Instant) {
        *slot = slot.saturating_add(start.elapsed().as_nanos() as u64);
    }
}
/// What the pooled metadata lane actually did for one save operation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PoolCounters {
    /// Canonical inode leaves admitted.
    pub leaves: u64,
    /// Values that reused an existing ordinal.
    pub reused_values: u64,
    /// Values that received a new ordinal.
    pub new_values: u64,
    /// Value groups written.
    pub groups: u64,
    /// Pooled leaves stored against a direct base.
    pub delta_leaves: u64,
    /// Pooled leaves stored in full.
    pub full_leaves: u64,
    /// Delta trials attempted.
    pub trials: u64,
    /// Candidates refused because the resulting chain would exceed its budget.
    pub work_exceeded: u64,
}
