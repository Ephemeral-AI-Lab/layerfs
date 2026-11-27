//! Three-way merge of immutable roots, conflict markers and `.lww` rules.
//!
//! Entry module: declarations and re-exports only.

pub mod decide;
pub(crate) mod outcome;
pub(crate) mod plan;
pub(crate) mod roots;
pub mod rule;
pub(crate) mod serial;

pub use decide::markers::paths_with_markers;
pub use decide::text::{has_markers, merge3, Hunk, TextMerge};
pub use outcome::{Conflict, ConflictKind, LwwDecision, MergeInput, MergeOutcome, RulesFrom, Side};
pub use roots::merge_roots;
pub use rule::lww::{Rule, Rules};
pub use rule::revert::{revert_units, RevertInput, Reverted};
pub use rule::units::{decide_units, unit_changes, MergeUnit, UnitChange, UnitReport, Units};

// #828 compat: old public module paths, removed in wave 3.
#[doc(hidden)]
pub use crate::merge::decide::markers; // #828 compat
#[doc(hidden)]
pub use crate::merge::decide::text; // #828 compat
#[doc(hidden)]
pub use crate::merge::rule::lww; // #828 compat
