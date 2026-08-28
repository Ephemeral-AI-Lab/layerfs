//! Atomic projection telemetry values.

use super::{DurabilityClassCounts, ProjectionTimer};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectionCallFacts {
    pub attempts: u64,
    pub successes: u64,
    pub failures: u64,
    pub wall: ProjectionTimer,
}

impl ProjectionCallFacts {
    pub const fn available() -> Self {
        Self {
            attempts: 0,
            successes: 0,
            failures: 0,
            wall: ProjectionTimer::available(),
        }
    }

    pub(super) fn checked_delta(self, before: Self) -> Option<Self> {
        Some(Self {
            attempts: self.attempts.checked_sub(before.attempts)?,
            successes: self.successes.checked_sub(before.successes)?,
            failures: self.failures.checked_sub(before.failures)?,
            wall: self.wall.checked_delta(before.wall)?,
        })
    }

    pub(super) fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            attempts: self.attempts.checked_add(other.attempts)?,
            successes: self.successes.checked_add(other.successes)?,
            failures: self.failures.checked_add(other.failures)?,
            wall: self.wall.checked_add(other.wall)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectionWriteFacts {
    pub attempts: u64,
    pub successes: u64,
    pub failures: u64,
    pub bytes: u64,
    pub wall: ProjectionTimer,
}

impl ProjectionWriteFacts {
    pub const fn available() -> Self {
        Self {
            attempts: 0,
            successes: 0,
            failures: 0,
            bytes: 0,
            wall: ProjectionTimer::available(),
        }
    }

    pub(super) fn checked_delta(self, before: Self) -> Option<Self> {
        Some(Self {
            attempts: self.attempts.checked_sub(before.attempts)?,
            successes: self.successes.checked_sub(before.successes)?,
            failures: self.failures.checked_sub(before.failures)?,
            bytes: self.bytes.checked_sub(before.bytes)?,
            wall: self.wall.checked_delta(before.wall)?,
        })
    }

    pub(super) fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            attempts: self.attempts.checked_add(other.attempts)?,
            successes: self.successes.checked_add(other.successes)?,
            failures: self.failures.checked_add(other.failures)?,
            bytes: self.bytes.checked_add(other.bytes)?,
            wall: self.wall.checked_add(other.wall)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectionSyncFacts {
    pub attempts: u64,
    pub successes: u64,
    pub failures: u64,
    pub requested: DurabilityClassCounts,
    pub achieved: DurabilityClassCounts,
    pub wall: ProjectionTimer,
}

impl ProjectionSyncFacts {
    pub const fn available() -> Self {
        Self {
            attempts: 0,
            successes: 0,
            failures: 0,
            requested: DurabilityClassCounts {
                process_crash_reconciled: 0,
                host_crash_ordered: 0,
                device_flush_requested: 0,
                power_loss_qualified: 0,
            },
            achieved: DurabilityClassCounts {
                process_crash_reconciled: 0,
                host_crash_ordered: 0,
                device_flush_requested: 0,
                power_loss_qualified: 0,
            },
            wall: ProjectionTimer::available(),
        }
    }

    pub(super) fn checked_delta(self, before: Self) -> Option<Self> {
        Some(Self {
            attempts: self.attempts.checked_sub(before.attempts)?,
            successes: self.successes.checked_sub(before.successes)?,
            failures: self.failures.checked_sub(before.failures)?,
            requested: self.requested.checked_delta(before.requested)?,
            achieved: self.achieved.checked_delta(before.achieved)?,
            wall: self.wall.checked_delta(before.wall)?,
        })
    }

    pub(super) fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            attempts: self.attempts.checked_add(other.attempts)?,
            successes: self.successes.checked_add(other.successes)?,
            failures: self.failures.checked_add(other.failures)?,
            requested: self.requested.checked_add(other.requested)?,
            achieved: self.achieved.checked_add(other.achieved)?,
            wall: self.wall.checked_add(other.wall)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectionReplaceFacts {
    pub attempts: u64,
    pub successes: u64,
    pub failures: u64,
    pub requested_visible: u64,
    pub prior_visible: u64,
    pub visibility_ambiguous: u64,
    pub durability_ambiguous: u64,
    pub wall: ProjectionTimer,
}

impl ProjectionReplaceFacts {
    pub const fn available() -> Self {
        Self {
            attempts: 0,
            successes: 0,
            failures: 0,
            requested_visible: 0,
            prior_visible: 0,
            visibility_ambiguous: 0,
            durability_ambiguous: 0,
            wall: ProjectionTimer::available(),
        }
    }

    pub(super) fn checked_delta(self, before: Self) -> Option<Self> {
        Some(Self {
            attempts: self.attempts.checked_sub(before.attempts)?,
            successes: self.successes.checked_sub(before.successes)?,
            failures: self.failures.checked_sub(before.failures)?,
            requested_visible: self
                .requested_visible
                .checked_sub(before.requested_visible)?,
            prior_visible: self.prior_visible.checked_sub(before.prior_visible)?,
            visibility_ambiguous: self
                .visibility_ambiguous
                .checked_sub(before.visibility_ambiguous)?,
            durability_ambiguous: self
                .durability_ambiguous
                .checked_sub(before.durability_ambiguous)?,
            wall: self.wall.checked_delta(before.wall)?,
        })
    }

    pub(super) fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            attempts: self.attempts.checked_add(other.attempts)?,
            successes: self.successes.checked_add(other.successes)?,
            failures: self.failures.checked_add(other.failures)?,
            requested_visible: self
                .requested_visible
                .checked_add(other.requested_visible)?,
            prior_visible: self.prior_visible.checked_add(other.prior_visible)?,
            visibility_ambiguous: self
                .visibility_ambiguous
                .checked_add(other.visibility_ambiguous)?,
            durability_ambiguous: self
                .durability_ambiguous
                .checked_add(other.durability_ambiguous)?,
            wall: self.wall.checked_add(other.wall)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectionCleanupFacts {
    pub attempts: u64,
    pub successes: u64,
    pub failures: u64,
    pub residue: u64,
    pub wall: ProjectionTimer,
}

impl ProjectionCleanupFacts {
    pub const fn available() -> Self {
        Self {
            attempts: 0,
            successes: 0,
            failures: 0,
            residue: 0,
            wall: ProjectionTimer::available(),
        }
    }

    pub(super) fn checked_delta(self, before: Self) -> Option<Self> {
        Some(Self {
            attempts: self.attempts.checked_sub(before.attempts)?,
            successes: self.successes.checked_sub(before.successes)?,
            failures: self.failures.checked_sub(before.failures)?,
            residue: self.residue.checked_sub(before.residue)?,
            wall: self.wall.checked_delta(before.wall)?,
        })
    }

    pub(super) fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            attempts: self.attempts.checked_add(other.attempts)?,
            successes: self.successes.checked_add(other.successes)?,
            failures: self.failures.checked_add(other.failures)?,
            residue: self.residue.checked_add(other.residue)?,
            wall: self.wall.checked_add(other.wall)?,
        })
    }
}
