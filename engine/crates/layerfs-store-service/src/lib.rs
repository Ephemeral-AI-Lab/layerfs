//! The Store engine: typed reads, file changes and Branch history over the
//! global Store, in the process that calls them.
//!
//! The SDK's fs, branch and project methods and the operator Store calls run
//! on this engine in the SDK's own process, against the Store it opened
//! (`StoreService`); this is their only route (#352 F-020). The engine is
//! organised by mechanism: `read`, `write` and `history` know no SDK method
//! names; `methods/` is the SDK facade, one file per public Store method and
//! their method table, [`methods::call`]. The daemon uses the same `store`
//! and `overlay` for its Workspace Commits. This crate has no FUSE, socket or
//! Sandbox. Each Workspace owns one overlay connection behind one lock; a
//! caller runs its own short typed call under it and keeps exact failure
//! custody.
#![forbid(unsafe_code)]

pub mod families;
pub mod filesystem;
pub mod history;
pub mod lookup;
pub(crate) mod service;
pub mod store;
pub mod telemetry;
pub mod workspace;

pub use owner::{OriginalInput, Owner, OwnerClient, OwnerConfig, OwnerError, WorkspacePanic};
pub use service::{OpenRefusal, Retained, StoreService, CALLS};
pub use workspace::owner::class::ServiceClass;
pub use workspace::owner::jobs as owner;
pub use workspace::owner::operations::hold::{MaintenanceHold, MutationHold};
pub use workspace::owner::roster::startup::OwnerStart;
pub use workspace::owner::work::JobWork;
pub use workspace::owner::work::OwnerWork;

// #828 compat: old public module paths, removed in wave 3.
#[doc(hidden)]
pub use crate::families as methods;
#[doc(hidden)]
pub use crate::store::install; // #828 compat
#[doc(hidden)]
pub use crate::store::kernel_guard; // #828 compat
#[doc(hidden)]
pub use crate::store::open as bootstrap; // #828 compat // #828 compat
#[doc(hidden)]
pub mod operator {
    // #828 compat
    #[doc(hidden)]
    pub use crate::families::args; // #828 compat
    #[doc(hidden)]
    pub use crate::families::operator::list_workspaces as inspect; // #828 compat
    #[doc(hidden)]
    pub use crate::families::operator::storage as store;
    #[allow(unused_imports)]
    pub use crate::families::operator::*; // #828 compat // #828 compat
}
#[doc(hidden)]
pub mod overlay {
    // #828 compat
    #[doc(hidden)]
    pub use crate::workspace::owner::jobs as owner; // #828 compat
    #[doc(hidden)]
    pub use crate::workspace::owner::operations::hold; // #828 compat
    #[doc(hidden)]
    pub use crate::workspace::owner::operations::kernel as kernel_operations; // #828 compat
    #[doc(hidden)]
    pub use crate::workspace::owner::operations::lifetime as lifetime_operations; // #828 compat
    #[doc(hidden)]
    pub use crate::workspace::owner::operations::read as read_operations; // #828 compat
    #[doc(hidden)]
    pub use crate::workspace::owner::ports::captured_namespace as captured_namespace_port; // #828 compat
    #[doc(hidden)]
    pub use crate::workspace::owner::ports::captured_runs as captured_run_port; // #828 compat
    #[doc(hidden)]
    pub use crate::workspace::owner::ports::files as file_port; // #828 compat
    #[doc(hidden)]
    pub use crate::workspace::owner::ports::operation_record as operation_record_port; // #828 compat
    #[doc(hidden)]
    pub use crate::workspace::owner::ports::overlay as read_port; // #828 compat
    #[doc(hidden)]
    pub use crate::workspace::owner::roster::backing; // #828 compat
    #[doc(hidden)]
    pub use crate::workspace::owner::roster::scopes; // #828 compat
    #[doc(hidden)]
    pub use crate::workspace::owner::roster::workspaces;
    #[allow(unused_imports)]
    pub use crate::workspace::owner::*; // #828 compat // #828 compat
}
#[doc(hidden)]
pub mod read {
    // #828 compat
    #[doc(hidden)]
    pub use crate::filesystem::cache as cached; // #828 compat
    #[doc(hidden)]
    pub use crate::filesystem::compare::boundaries as comparison_pages; // #828 compat
    #[doc(hidden)]
    pub use crate::filesystem::compare::diff as compare; // #828 compat
    #[doc(hidden)]
    pub use crate::filesystem::compare::file_rows as file_compare; // #828 compat
    #[doc(hidden)]
    pub use crate::filesystem::compare::plans; // #828 compat
    #[doc(hidden)]
    pub use crate::filesystem::compare::tree_pages as branch_compare; // #828 compat
    #[doc(hidden)]
    pub use crate::filesystem::facts as file_facts; // #828 compat
    #[doc(hidden)]
    pub use crate::filesystem::paged::query; // #828 compat
    #[doc(hidden)]
    pub use crate::filesystem::paged::results as paged_reads; // #828 compat
    #[doc(hidden)]
    pub use crate::filesystem::paged::revision; // #828 compat
    #[doc(hidden)]
    pub use crate::filesystem::root as snapshot; // #828 compat
    #[doc(hidden)]
    pub use crate::filesystem::text::attribution; // #828 compat
    #[doc(hidden)]
    pub use crate::filesystem::text::blame; // #828 compat
    #[doc(hidden)]
    pub use crate::filesystem::text::search; // #828 compat
    #[doc(hidden)]
    pub use crate::filesystem::text::window;
    #[doc(hidden)]
    pub use crate::filesystem::tree::positional as reads; // #828 compat
    #[doc(hidden)]
    pub use crate::filesystem::tree::stats as tree_stats; // #828 compat
    #[doc(hidden)]
    pub use crate::filesystem::tree::visibility; // #828 compat
    #[doc(hidden)]
    pub use crate::filesystem::tree::walk; // #828 compat
    #[allow(unused_imports)]
    pub use crate::filesystem::*; // #828 compat // #828 compat
}
#[doc(hidden)]
pub use crate::history::whiteout as removal; // #828 compat
#[doc(hidden)]
pub use crate::workspace::owner as service; // #828 compat
#[doc(hidden)]
pub mod write {
    // #828 compat
    #[doc(hidden)]
    pub use crate::filesystem::change::run as files;
    #[allow(unused_imports)]
    pub use crate::filesystem::change::*; // #828 compat // #828 compat
}
