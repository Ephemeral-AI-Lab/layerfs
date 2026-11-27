//! Merge requests, outcomes and conflict records.

use crate::compare::{BindingHints, DifferenceWork};
use crate::filesystem::FilesystemRootId;
use crate::policy::{ConstructionCapacities, ConstructionPolicy};

use super::decide::text::Hunk;

/// Kind of one conflicting path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConflictKind {
    /// Both sides changed a file's lines differently.
    Content,
    /// Both sides added different files at one path.
    AddAdd,
    /// Ours changed a file theirs deleted.
    ModifyDelete,
    /// Ours deleted a file theirs changed.
    DeleteModify,
    /// The sides disagree on the entry's kind.
    Type,
    /// Both changed a file that is not bounded text; ours is kept unmarked.
    Binary,
    /// A file still holds an unresolved marker block.
    Markers,
    /// Both changed a file's mode, to different modes; the decided entry's
    /// mode is kept (#352 F-052).
    Mode,
}

impl ConflictKind {
    /// The SDK's name for the kind.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Content => "content",
            Self::AddAdd => "add/add",
            Self::ModifyDelete => "modify/delete",
            Self::DeleteModify => "delete/modify",
            Self::Type => "type",
            Self::Binary => "binary",
            Self::Markers => "markers",
            Self::Mode => "mode",
        }
    }
}

/// One conflicting path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Conflict {
    /// Absolute path.
    pub path: String,
    /// Kind.
    pub kind: ConflictKind,
    /// Marker blocks written into the file, in order.
    pub hunks: Vec<Hunk>,
}

/// One path a `.lww` rule decided.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LwwDecision {
    /// Absolute path.
    pub path: String,
    /// True when ours was newer or as new.
    pub kept_ours: bool,
    /// True when the kept side had deleted the path.
    pub deleted: bool,
    /// The deciding rule as written.
    pub rule: String,
}

/// Which side's `/.lww` decides.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RulesFrom {
    /// Ours (a Branch merge reads the left Branch).
    Ours,
    /// Theirs (a Workspace Commit reads the Branch head).
    Theirs,
}

/// One side of a merge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Side {
    /// Ours.
    Ours,
    /// Theirs.
    Theirs,
}

/// One merge request.
pub struct MergeInput<'a> {
    /// Common ancestor root.
    pub base: FilesystemRootId,
    /// Our root.
    pub ours: FilesystemRootId,
    /// Their root.
    pub theirs: FilesystemRootId,
    /// Marker labels of ours and theirs.
    pub labels: [&'a str; 2],
    /// Side whose `/.lww` applies.
    pub rules_from: RulesFrom,
    /// Construction policy of merged files.
    pub policy: ConstructionPolicy,
    /// Construction capacities of merged files.
    pub capacities: &'a ConstructionCapacities,
    /// Recorded name bindings that name the changed inodes top down; without
    /// them, or when they cannot name every one, a breadth-first walk does.
    pub hints: Option<&'a dyn BindingHints>,
    /// The side whose changes from the base name the paths to decide. Every
    /// other path keeps that side's opposite, on which the result is built.
    /// The decision at each path follows the same rules either way.
    pub decide_from: Side,
    /// When set, a merge with more paths to decide is refused
    /// `BoundedCapacityExceeded { what: "merge.bound" }` before any decision.
    pub bound: Option<usize>,
}

/// One completed merge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MergeOutcome {
    /// The merged root, with marker files where conflicts are.
    pub root: FilesystemRootId,
    /// Conflicting paths, in path order.
    pub conflicts: Vec<Conflict>,
    /// `.lww` decisions, in path order.
    pub lww: Vec<LwwDecision>,
    /// Paths decided.
    pub decided: usize,
    /// Sorted-tree work.
    pub work: DifferenceWork,
    /// True when recorded bindings named the changed inodes.
    pub hinted: bool,
    /// What naming from recorded bindings did, on both sides.
    pub naming: crate::compare::NamingWork,
}

impl MergeOutcome {
    /// True when no path conflicted.
    pub fn clean(&self) -> bool {
        self.conflicts.is_empty()
    }
}
