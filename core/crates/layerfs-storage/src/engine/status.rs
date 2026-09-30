//! Native observations and exact failed-bootstrap custody, without counter resets.

use std::fmt;

use crate::error::StorageError;

/// Actual tracked native ownership. Highwaters are lifetime values, not phases.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeMemory {
    /// Current bytes checked out through SQLite's native allocator.
    pub used_bytes: u64,
    /// Lifetime maximum tracked bytes; no observation resets this counter.
    pub lifetime_highwater_bytes: u64,
    /// Current number of separate native allocations.
    pub allocations: u64,
    /// Lifetime maximum allocation count, never a phase peak.
    pub lifetime_highwater_allocations: u64,
}

/// One read of the exact hard limit and global native counters.
/// These separate API reads are not an atomic snapshot under concurrent work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EngineObservation {
    /// Actual current hard limit read from the established engine.
    pub hard_heap_limit_bytes: u64,
    /// Native allocator counters read with resetFlag0.
    pub memory: NativeMemory,
}

/// Exclusive startup's real native allocation, accounting and release evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EngineBootstrapObservation {
    /// Prior hard limit, accepted only when zero or exactly the selected limit.
    pub previous_hard_heap_limit_bytes: u64,
    /// Tracked initialization/default ownership before the native probe.
    pub before_probe: NativeMemory,
    /// Actual ownership while the one native probe is live.
    pub during_probe: NativeMemory,
    /// Ownership after the known probe has been freed once.
    pub after_probe: NativeMemory,
    /// Actual native `sqlite3_msize`, including provider allocation rounding.
    pub probe_allocated_bytes: u64,
}

/// Exact first failed startup step; no later step is attempted after a failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EngineBootstrapStage {
    /// First native call configuring active memory statistics.
    MemStatus,
    /// Explicit provider initialization after known configuration.
    Initialize,
    /// Inspecting the provider's existing limit after initialization.
    PreviousLimit,
    /// Setting the one supported hard limit once.
    SetLimit,
    /// Reading back that exact limit.
    Readback,
    /// Reading native counters before probe allocation.
    Baseline,
    /// Acquiring the fixed requested native probe.
    ProbeAllocate,
    /// Checking the probe's actual native allocation size.
    ProbeSize,
    /// Observing the actual live allocation and checking its counter deltas.
    DuringProbe,
    /// Checking ownership after one known native free.
    AfterProbe,
    /// Reading actual static linked-provider identity and thread capability.
    ProviderIdentity,
}

/// Recorded bootstrap effects retained alongside its original failure.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EngineBootstrapCustody {
    /// MEMSTATUS1 was acknowledged before any engine initializer/query.
    pub memstatus_acknowledged: bool,
    /// This bootstrap attempted its explicit initializer once.
    pub initialize_attempted: bool,
    /// This bootstrap's initializer returned SQLITE_OK.
    pub initialized_by_bootstrap: bool,
    /// Queried prior limit, absent if configuration/initialization refused.
    pub previous_hard_heap_limit: Option<i64>,
    /// The one selected hard-limit setter was attempted.
    pub hard_limit_set_attempted: bool,
    /// Prior limit returned by the one attempted setter.
    pub setter_previous_hard_heap_limit: Option<i64>,
    /// The exact supported hard limit was read back after the setter.
    pub hard_limit_installed: bool,
    /// Actual limit read back after the setter, including an invalid result.
    pub observed_hard_heap_limit: Option<i64>,
    /// One nonnull native probe was issued.
    pub probe_issued: bool,
    /// The issued known native pointer was freed once, including error paths.
    pub probe_free_attempted: bool,
    /// Actual size when native probe allocation succeeded and size was read.
    pub probe_allocated_bytes: Option<u64>,
}

/// One cached original failure. Bootstrap never resets/shuts down or retries it.
#[derive(Debug)]
pub struct EngineBootstrapFailure {
    stage: EngineBootstrapStage,
    cause: StorageError,
    custody: EngineBootstrapCustody,
}

impl EngineBootstrapFailure {
    pub(super) fn new(
        stage: EngineBootstrapStage,
        cause: StorageError,
        custody: EngineBootstrapCustody,
    ) -> Self {
        Self {
            stage,
            cause,
            custody,
        }
    }

    /// First failed native startup step.
    pub const fn stage(&self) -> EngineBootstrapStage {
        self.stage
    }
    /// Original typed failure, without message parsing or class coercion.
    pub const fn cause(&self) -> &StorageError {
        &self.cause
    }
    /// Exact known effects and remaining global engine custody.
    pub const fn custody(&self) -> &EngineBootstrapCustody {
        &self.custody
    }
    /// True only if this bootstrap's explicit initializer was acknowledged.
    pub const fn initialized_by_bootstrap(&self) -> bool {
        self.custody.initialized_by_bootstrap
    }
    /// True only after exact supported hard-limit readback.
    pub const fn limit_installed(&self) -> bool {
        self.custody.hard_limit_installed
    }
    /// True if the one issued known probe was freed on its success/error path.
    pub const fn probe_free_attempted(&self) -> bool {
        self.custody.probe_free_attempted
    }
}

impl fmt::Display for EngineBootstrapFailure {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            output,
            "SQLite engine bootstrap stage={:?}: {}; custody={:?}; no shutdown/reset/retry",
            self.stage, self.cause, self.custody
        )
    }
}

impl std::error::Error for EngineBootstrapFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}
