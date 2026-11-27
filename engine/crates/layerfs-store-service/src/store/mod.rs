//! In-process Store ports shared by one daemon's Workspaces.
pub mod handle;
pub mod install;
pub mod kernel_guard;
pub mod open;
pub mod ports;
pub mod recognizer;
pub use super::workspace::bind::run::checked_base;
pub use super::workspace::commit::hints::record_new_bindings;
pub use super::workspace::commit::hints::CatalogHints;
pub use super::workspace::commit::live::LIVE_CONFLICT_PATHS;
pub use super::workspace::commit::merge::{Stale, StaleError, LANE_MERGE_PATHS};

pub use super::workspace::bind::operation::{BoundWorkspace, StoreOperation};
pub use super::workspace::bind::reader::StoreReader;
pub use super::workspace::bind::readers::{ReadAdmissionError, ReadServiceWork, ReaderFactory};
pub use super::workspace::bind::request::{
    BindError, BindIntent, BindPhase, BindRefusal, BindRequest, BindSuccess, CloseError,
    NamedBindRequest,
};
pub use super::workspace::commit::outcome::{
    CapturedConstruction, CommitError, CommitFailure, CommitOptions, CommitPhase, CommitSuccess,
    MergeReport, ReleasedOwner,
};
pub use super::workspace::commit::resolve::{Resolution, Unresolved};
pub use handle::{MergeCounters, ReuseWork, SaveCounters, Store, StoreWork};
pub use ports::{PortError, StorePorts};
pub use recognizer::{Recognizer, RecognizerWork, RECOGNIZER_SLOTS};

pub use super::workspace::commit::outcome::owner_uncertain;
pub use super::workspace::commit::outcome::service_cause;

// #828 compat: old public module paths, removed in wave 3.
#[doc(hidden)]
pub use crate::store::handle as open; // #828 compat
#[doc(hidden)]
pub use crate::workspace::bind::operation; // #828 compat
#[doc(hidden)]
pub use crate::workspace::bind::operation as statistics; // #828 compat
#[doc(hidden)]
pub use crate::workspace::bind::reader as read_handle; // #828 compat
#[doc(hidden)]
pub use crate::workspace::bind::readers as read_service; // #828 compat
#[doc(hidden)]
pub use crate::workspace::bind::request as types;
#[doc(hidden)]
pub use crate::workspace::bind::run as bind; // #828 compat
#[doc(hidden)]
pub use crate::workspace::commit::captured; // #828 compat
#[doc(hidden)]
pub use crate::workspace::commit::digests; // #828 compat
#[doc(hidden)]
pub use crate::workspace::commit::hints; // #828 compat
#[doc(hidden)]
pub use crate::workspace::commit::merge as commit_merge; // #828 compat
#[doc(hidden)]
pub use crate::workspace::commit::outcome as commit_types; // #828 compat
#[doc(hidden)]
pub use crate::workspace::commit::producer; // #828 compat
#[doc(hidden)]
pub use crate::workspace::commit::resolve; // #828 compat
#[doc(hidden)]
pub use crate::workspace::commit::run as commit; // #828 compat
#[doc(hidden)]
pub use crate::workspace::commit::settle; // #828 compat // #828 compat
