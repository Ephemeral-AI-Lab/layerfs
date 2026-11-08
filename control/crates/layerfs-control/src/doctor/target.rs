//! Shared target options and deadline for observational diagnostics.
use std::time::{Duration, Instant};

/// One explicitly selected diagnostic scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DoctorTarget {
    /// Local installation and home only.
    Host,
    /// One machine and its worker.
    Machine(String),
    /// One sandbox, including its daemon.
    Sandbox(String),
    /// Database and LayerFS integrity for one Store.
    StoreIntegrity(String),
    /// A registered PostgreSQL connection.
    StoreConnection(String),
    /// A registered backup destination.
    BackupConnection(String),
}
impl DoctorTarget {
    /// Stable kind and name for the report.
    pub fn entity(&self) -> (&str, &str) {
        match self {
            Self::Host => ("host", "local"),
            Self::Machine(name) => ("machine", name),
            Self::Sandbox(name) => ("sandbox", name),
            Self::StoreIntegrity(name) | Self::StoreConnection(name) => ("store", name),
            Self::BackupConnection(name) => ("backup", name),
        }
    }
    /// Stable selected operation, separate from entity identity.
    pub fn operation(&self) -> &str {
        match self {
            Self::StoreIntegrity(_) => "integrity",
            Self::StoreConnection(_) | Self::BackupConnection(_) => "connection",
            _ => "diagnose",
        }
    }
}

/// Options shared by all public front ends, outside normal call dispatch.
#[derive(Clone, Debug)]
pub struct DoctorRequest {
    /// Explicit target; absent options use the legacy entry instead.
    pub target: DoctorTarget,
    /// Overall deadline, including admission and external work.
    pub timeout: Option<Duration>,
    /// Reduced Store integrity coverage.
    pub quick: bool,
    /// Explicit temporary backup object round trip.
    pub write_test: bool,
}

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

pub use super::check_report::Checks;
