//! The time budget a doctor gives one thing, and the per-check collector
//! today's target checks write into (`Checks`), which the frame turns into
//! that thing's node (`check_report`).
use std::time::{Duration, Instant};

/// A monotonic overall work budget. Probes must bound their real work by it.
pub struct Budget {
    started: Instant,
    deadline: Option<Instant>,
}
impl Budget {
    /// Starts a deadline; an absent timeout does not cap a full audit.
    pub fn new(timeout: Option<Duration>) -> Self {
        let started = Instant::now();
        Self {
            started,
            deadline: timeout.and_then(|d| started.checked_add(d)),
        }
    }
    /// Remaining overall time, or no deadline.
    pub fn remaining(&self) -> Option<Duration> {
        self.deadline
            .map(|d| d.saturating_duration_since(Instant::now()))
    }
    /// Whether the overall deadline has expired.
    pub fn expired(&self) -> bool {
        self.remaining().is_some_and(|d| d.is_zero())
    }
    /// Deadline shared with lower-level synchronous probes.
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }
    /// Elapsed time since diagnostic work began.
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }
    /// A probe timeout never exceeding the remaining budget.
    pub fn limit(&self, maximum: Duration) -> Duration {
        self.remaining().map_or(maximum, |d| d.min(maximum))
    }
}

pub use super::report::Checks;
