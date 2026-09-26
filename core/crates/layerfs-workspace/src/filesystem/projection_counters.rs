//! Bounded per-operation counts of projection callbacks and upstream calls.
//!
//! These are ordinary product telemetry: they let an operator see how much
//! kernel and host work a mounted Workspace is actually doing, so avoidable
//! repeated metadata lookups or upstream requests can be identified from a
//! real route instead of guessed at. Counts are saturating and never gate an
//! operation.

/// The projection callbacks this build distinguishes.
///
/// The set is fixed and small so a counter read is bounded and the reported
/// shape is stable. `Other` covers the remaining callbacks without growing the
/// table per kernel version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionOp {
    Lookup,
    Getattr,
    Read,
    Write,
    Readdir,
    Open,
    Setattr,
    Rename,
    Other,
}

impl ProjectionOp {
    pub const ALL: [ProjectionOp; 9] = [
        ProjectionOp::Lookup,
        ProjectionOp::Getattr,
        ProjectionOp::Read,
        ProjectionOp::Write,
        ProjectionOp::Readdir,
        ProjectionOp::Open,
        ProjectionOp::Setattr,
        ProjectionOp::Rename,
        ProjectionOp::Other,
    ];

    fn index(self) -> usize {
        match self {
            ProjectionOp::Lookup => 0,
            ProjectionOp::Getattr => 1,
            ProjectionOp::Read => 2,
            ProjectionOp::Write => 3,
            ProjectionOp::Readdir => 4,
            ProjectionOp::Open => 5,
            ProjectionOp::Setattr => 6,
            ProjectionOp::Rename => 7,
            ProjectionOp::Other => 8,
        }
    }

    /// The stable name used by status readers.
    pub fn label(self) -> &'static str {
        match self {
            ProjectionOp::Lookup => "lookup",
            ProjectionOp::Getattr => "getattr",
            ProjectionOp::Read => "read",
            ProjectionOp::Write => "write",
            ProjectionOp::Readdir => "readdir",
            ProjectionOp::Open => "open",
            ProjectionOp::Setattr => "setattr",
            ProjectionOp::Rename => "rename",
            ProjectionOp::Other => "other",
        }
    }
}

/// Saturating counts of projection callbacks and upstream host calls.
///
/// A count never wraps and never fails an operation. `upstream` counts calls
/// the Workspace sent to the host Service, which is the quantity a transport
/// change is expected to move.
#[derive(Default)]
pub struct ProjectionCounters {
    callbacks: [u64; ProjectionOp::ALL.len()],
    upstream: u64,
}

impl ProjectionCounters {
    pub fn record(&mut self, op: ProjectionOp) {
        let slot = &mut self.callbacks[op.index()];
        *slot = slot.saturating_add(1);
    }

    pub fn record_upstream(&mut self) {
        self.upstream = self.upstream.saturating_add(1);
    }

    pub fn count(&self, op: ProjectionOp) -> u64 {
        self.callbacks[op.index()]
    }

    pub fn upstream(&self) -> u64 {
        self.upstream
    }
}
