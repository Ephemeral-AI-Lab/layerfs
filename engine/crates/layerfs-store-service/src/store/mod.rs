//! In-process Store ports shared by one daemon's Workspaces.
pub mod bind;
pub mod captured;
pub mod commit;
pub mod commit_merge;
pub mod commit_types;
pub mod digests;
pub mod hints;
pub mod open;
pub mod operation;
pub mod ports;
pub mod producer;
pub mod read_handle;
pub mod read_service;
pub mod recognizer;
pub mod settle;
pub mod statistics;
pub mod types;
pub use bind::checked_base;
pub use commit_merge::{Stale, StaleError, LANE_MERGE_PATHS};
pub use hints::record_new_bindings;
pub use hints::CatalogHints;

pub use commit_types::{
    CapturedConstruction, CommitError, CommitFailure, CommitOptions, CommitPhase, CommitSuccess,
    MergeReport, ReleasedOwner,
};
pub use open::{MergeCounters, ReuseWork, SaveCounters, Store, StoreWork};
pub use operation::{BoundWorkspace, StoreOperation};
pub use ports::{PortError, StorePorts};
pub use read_handle::StoreReader;
pub use read_service::{ReadAdmissionError, ReadServiceWork, ReaderFactory};
pub use recognizer::{Recognizer, RecognizerWork, RECOGNIZER_SLOTS};
pub use types::{BindError, BindPhase, BindRefusal, BindRequest, BindSuccess, CloseError};

pub use commit_types::owner_uncertain;
