//! The Store: opening it (`open`), its in-process handle and ports shared
//! by one daemon's Workspaces, its install and the kernel guard.
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

#[doc(hidden)]
pub use crate::workspace::bind::request as types;
