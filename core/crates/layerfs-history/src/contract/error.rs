//! Typed history failures with their exact expected/actual context.
//!
//! A failure is a product outcome, not a message. Nothing here is reconstructed
//! from text, and no caller is expected to parse a string to recover a class:
//! each variant is matched by the service and mapped to one wire code. `Busy` is
//! admission contention and never a semantic conflict; `UnknownOutcome` states
//! that persistence did not answer and therefore proves nothing about rollback.

use crate::identity::{CommitId, LayerId};
use crate::records::GcIntentKind;

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
    /// No allocator row for that scope.
    Scope,
    /// No history metadata row: an empty or foreign database.
    Catalog,
}

/// Which kind of record holds a name.
///
/// A later release may add a holder kind, so the enum is non-exhaustive: a
/// caller matches it with a wildcard arm and never constructs it.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HolderKind {
    /// A Branch.
    Branch,
    /// A project (LayerStack).
    Project,
}

/// State of the record that holds a name.
///
/// A later release may add a state, so the enum is non-exhaustive: a
/// caller matches it with a wildcard arm and never constructs it.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HolderState {
    /// The record is live.
    Live,
    /// The record is retired by `branch.remove`.
    Retired,
    /// The project is deleted.
    Deleted,
}

/// The record that holds a name.
///
/// A later release may add a field, so the struct is non-exhaustive:
/// a caller reads its fields, and construction belongs to this crate.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NameHolder {
    /// Record kind.
    pub kind: HolderKind,
    /// Name held.
    pub name: String,
    /// Holder state.
    pub state: HolderState,
}

/// Where a name is held.
///
/// A later release may add a source, so the enum is non-exhaustive: a
/// caller matches it with a wildcard arm and never constructs it.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NameSource {
    /// The Store's own records.
    Store,
    /// A home project record.
    Home,
}

/// Context for a `NameTaken` refusal.
///
/// A later release may add a field, so the struct is non-exhaustive:
/// a caller reads its fields, and construction belongs to this crate.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NameTakenState {
    /// The holder.
    pub holder: NameHolder,
    /// Where it is held.
    pub source: NameSource,
}

/// One hidden path.
///
/// A later release may add a field, so the struct is non-exhaustive:
/// a caller reads its fields, and construction belongs to this crate.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemovedPath {
    /// The request path.
    pub path: String,
    /// The governing frontier serial.
    pub frontier_serial: u64,
}

/// Context for a `Removed` refusal.
///
/// A later release may add a field, so the struct is non-exhaustive:
/// a caller reads its fields, and construction belongs to this crate.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemovedState {
    /// The hidden paths.
    pub paths: Vec<RemovedPath>,
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
    /// A name is held by a live, retired or deleted record.
    NameTaken(Box<NameTakenState>),
    /// A path is hidden by `remove_file`.
    Removed(Box<RemovedState>),
    /// A staged name or artifact already exists and no record names it.
    Exists(&'static str),
    /// A required marker or fence row is missing or contradicts the request.
    Conflict(&'static str),
    /// The Store was replaced or moved under this handle.
    StaleStore,
    /// Immediate metadata admission was refused; the caller may retry later.
    Busy,
    /// A mount or Sandbox holds the scope; the holders are the ones observed.
    MountsActive(Box<MountHolders>),
    /// A live operation holds the scope; its entity, phase and op are reported.
    BusyOp(Box<OpConflict>),
    /// A dead or uncertain owner's operation holds the scope.
    Unresolved(Box<OpConflict>),
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
    /// Writable authority was not established by this process.
    ContinuityUnavailable,
    /// Persistence did not answer; the outcome of the attempted statement set
    /// is unknown and nothing may be replayed, discarded or rolled back on a guess.
    UnknownOutcome,
}

impl HistoryError {
    /// True when the attempted mutation's persistence outcome is unknown.
    #[must_use]
    pub fn unknown(&self) -> bool {
        matches!(self.cause(), Self::UnknownOutcome)
    }

    /// The typed failure itself; kept for callers that match through it.
    #[must_use]
    pub fn cause(&self) -> &Self {
        self
    }
}

impl std::fmt::Display for HistoryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(what) => write!(formatter, "invalid input: {what}"),
            Self::Missing(what) => write!(formatter, "missing {what:?}"),
            Self::Unsupported(what) => write!(formatter, "unsupported: {what}"),
            Self::NameTaken(state) => write!(formatter, "name taken: {:?}", state.holder),
            Self::Removed(state) => write!(formatter, "removed: {} paths", state.paths.len()),
            Self::Exists(what) => write!(formatter, "exists: {what}"),
            Self::Conflict(what) => write!(formatter, "conflict: {what}"),
            Self::StaleStore => formatter.write_str("stale Store"),
            Self::Busy => formatter.write_str("busy"),
            Self::MountsActive(state) => {
                write!(
                    formatter,
                    "mounts active: {} holder(s)",
                    state.holders.len()
                )
            }
            Self::BusyOp(state) => write!(
                formatter,
                "busy: {} at {}",
                state.kind.as_str(),
                state.entity
            ),
            Self::Unresolved(state) => write!(
                formatter,
                "unresolved: {} at {}",
                state.kind.as_str(),
                state.entity
            ),
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
            Self::ContinuityUnavailable => formatter.write_str("continuity unavailable"),
            Self::UnknownOutcome => formatter.write_str("unknown outcome"),
        }
    }
}

impl std::error::Error for HistoryError {}

/// Result of one history operation.
pub type HistoryResult<T> = Result<T, HistoryError>;

impl NameHolder {
    /// Builds a holder for a typed `NameTaken` context.
    pub fn new(kind: HolderKind, name: impl Into<String>, state: HolderState) -> Self {
        Self {
            kind,
            name: name.into(),
            state,
        }
    }
}

impl NameTakenState {
    /// Builds the context of a `NameTaken` refusal.
    pub fn new(holder: NameHolder, source: NameSource) -> Self {
        Self { holder, source }
    }
}

impl RemovedPath {
    /// Builds one hidden path for a `Removed` refusal.
    pub fn new(path: impl Into<String>, frontier_serial: u64) -> Self {
        Self {
            path: path.into(),
            frontier_serial,
        }
    }
}

impl RemovedState {
    /// Builds the context of a `Removed` refusal.
    pub fn new(paths: Vec<RemovedPath>) -> Self {
        Self { paths }
    }
}

/// One admission holder: a live mount or operation in the scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GcHolder {
    /// What kind of holder it is (`mount`, `init`, `merge`, ...).
    pub kind: GcIntentKind,
    /// The project, Branch or Store the holder is on.
    pub entity: String,
    /// The holder's Workspace, when it has one.
    pub workspace: Option<String>,
    /// The holder's Sandbox, when it has one.
    pub sandbox: Option<String>,
    /// The holder's machine, when it has one.
    pub machine: Option<String>,
    /// The holder operation identity, hex encoded.
    pub op: String,
}

impl GcHolder {
    /// Builds one holder report.
    #[must_use]
    pub fn new(kind: GcIntentKind, entity: impl Into<String>, op: impl Into<String>) -> Self {
        Self {
            kind,
            entity: entity.into(),
            workspace: None,
            sandbox: None,
            machine: None,
            op: op.into(),
        }
    }
}

/// The holders that refuse a removal or compaction with `MountsActive`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MountHolders {
    /// Every live or uncertain holder observed in the scope.
    pub holders: Vec<GcHolder>,
}

impl MountHolders {
    /// Builds the refusal context.
    #[must_use]
    pub fn new(holders: Vec<GcHolder>) -> Self {
        Self { holders }
    }
}

/// The live or uncertain operation that refuses a call with `Busy` or
/// `Unresolved`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpConflict {
    /// The conflicting operation kind.
    pub kind: GcIntentKind,
    /// The project, Branch or Store it is on.
    pub entity: String,
    /// Its phase, when recorded.
    pub phase: Option<String>,
    /// Its operation identity, hex encoded.
    pub op: String,
    /// Whether its owner is uncertain.
    pub uncertain: bool,
}

impl OpConflict {
    /// Builds the refusal context.
    #[must_use]
    pub fn new(
        kind: GcIntentKind,
        entity: impl Into<String>,
        phase: Option<String>,
        op: impl Into<String>,
        uncertain: bool,
    ) -> Self {
        Self {
            kind,
            entity: entity.into(),
            phase,
            op: op.into(),
            uncertain,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_build_the_typed_context() {
        let holder = NameHolder::new(HolderKind::Branch, "main", HolderState::Retired);
        let taken = NameTakenState::new(holder, NameSource::Store);
        assert_eq!(taken.holder.name, "main");
        assert_eq!(taken.holder.kind, HolderKind::Branch);
        assert_eq!(taken.source, NameSource::Store);

        let removed = RemovedState::new(vec![RemovedPath::new("a.txt", 7)]);
        assert_eq!(removed.paths[0].path, "a.txt");
        assert_eq!(removed.paths[0].frontier_serial, 7);
    }
}
