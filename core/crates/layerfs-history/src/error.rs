//! Typed history failures with their exact expected/actual context.
//!
//! A failure is a product outcome, not a message. Nothing here is reconstructed
//! from text, and no caller is expected to parse a string to recover a class:
//! each variant is matched by the service and mapped to one wire code. `Busy` is
//! admission contention and never a semantic conflict; `UnknownOutcome` states
//! that persistence did not answer and therefore proves nothing about rollback.

use crate::identity::{CommitId, LayerId, StageToken};

/// A related record kind, used for a typed absence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Missing {
    /// No LayerStack with that identity.
    LayerStack,
    /// No Branch with that identity or authority-local name.
    Branch,
    /// No Commit with that identity.
    Commit,
    /// No Layer with that identity.
    Layer,
    /// No Workspace stage for that incarnation.
    Stage,
    /// No allocator row for that scope.
    Scope,
    /// No history metadata row: an empty or foreign database.
    Catalog,
}

/// Expected versus observed Branch publication state.
///
/// Both sides are reported, so a caller can distinguish a moved head from a
/// moved base without re-reading the Branch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MovedState {
    /// Head Commit the request expected.
    pub expected_head: Option<CommitId>,
    /// Head Commit the catalog holds.
    pub actual_head: Option<CommitId>,
    /// Base Layer the request expected.
    pub expected_base: LayerId,
    /// Base Layer the catalog holds.
    pub actual_base: LayerId,
}

/// Every failure a history operation can report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HistoryError {
    /// A checked input is structurally unusable.
    InvalidInput(&'static str),
    /// A named record does not exist.
    Missing(Missing),
    /// A required capability is not provided by this build or provider.
    Unsupported(&'static str),
    /// Immediate metadata admission was refused; the caller may retry later.
    Busy,
    /// The content authority this catalog is bound to is not available.
    OwnershipUnavailable,
    /// A declared bound, count or counter is exhausted.
    Capacity(&'static str),
    /// An immutable record or a stored relation contradicts another record.
    Integrity(&'static str),
    /// The Branch moved away from the expected head and base.
    ///
    /// The context is boxed so the error stays small on every `Result` the
    /// catalog returns; the failure is rare and the context is only read when
    /// one is actually reported.
    HeadMoved(Box<MovedState>),
    /// The Branch base contradicts the selected Commit's base.
    BaseMismatch {
        /// Base Layer the selected Commit records.
        commit_base: LayerId,
        /// Base Layer the Branch currently records.
        branch_base: LayerId,
    },
    /// The selected source is not in the expected ancestry or stack.
    NotInHistory(&'static str),
    /// The LayerStack head moved away from the expected Layer.
    StackMoved {
        /// Layer the request expected as the stack head.
        expected: LayerId,
        /// Layer the catalog holds as the stack head.
        actual: LayerId,
    },
    /// The exact stage token does not match the stage that is present.
    StageChanged {
        /// Token the request named.
        expected: StageToken,
        /// Token the catalog holds for that incarnation, when a stage exists.
        actual: Option<StageToken>,
    },
    /// Writable authority was not established by this process.
    ContinuityUnavailable,
    /// Persistence did not answer; the outcome of the attempted statement set
    /// is unknown and nothing may be replayed, discarded or rolled back on a guess.
    UnknownOutcome,
}

impl HistoryError {
    /// True when the attempted mutation's persistence outcome is unknown.
    pub const fn unknown(&self) -> bool {
        matches!(self, Self::UnknownOutcome)
    }
}

impl std::fmt::Display for HistoryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(what) => write!(formatter, "invalid input: {what}"),
            Self::Missing(what) => write!(formatter, "missing {what:?}"),
            Self::Unsupported(what) => write!(formatter, "unsupported: {what}"),
            Self::Busy => formatter.write_str("busy"),
            Self::OwnershipUnavailable => formatter.write_str("ownership unavailable"),
            Self::Capacity(what) => write!(formatter, "capacity: {what}"),
            Self::Integrity(what) => write!(formatter, "integrity: {what}"),
            Self::HeadMoved(state) => write!(formatter, "head moved: {state:?}"),
            Self::BaseMismatch {
                commit_base,
                branch_base,
            } => write!(formatter, "base mismatch: {commit_base} vs {branch_base}"),
            Self::NotInHistory(what) => write!(formatter, "not in history: {what}"),
            Self::StackMoved { expected, actual } => {
                write!(formatter, "stack moved: {expected} -> {actual}")
            }
            Self::StageChanged { expected, actual } => {
                write!(formatter, "stage changed: {expected:?} -> {actual:?}")
            }
            Self::ContinuityUnavailable => formatter.write_str("continuity unavailable"),
            Self::UnknownOutcome => formatter.write_str("unknown outcome"),
        }
    }
}

impl std::error::Error for HistoryError {}

/// Result of one history operation.
pub type HistoryResult<T> = Result<T, HistoryError>;
