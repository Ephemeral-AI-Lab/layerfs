//! In-process Store ports shared by one daemon's Workspaces.
mod bind;
mod captured;
mod commit;
mod commit_types;
mod open;
mod operation;
mod ports;
mod read_handle;
mod read_scope;
mod read_service;
mod read_state;
mod settle;
mod types;
pub(crate) use bind::checked_base;

pub use commit_types::{
    CapturedConstruction, CommitError, CommitFailure, CommitPhase, CommitSuccess, ReleasedOwner,
};
pub use open::{Store, StoreWork};
pub use operation::{BoundWorkspace, StoreOperation};
pub use ports::{PortError, StorePorts};
pub use read_handle::{ReadLease, StoreReader};
pub use read_service::{ReadAdmissionError, ReadLimits, ReadServiceWork, ReadTicket};
pub use types::{BindError, BindPhase, BindRefusal, BindRequest, BindSuccess};

pub(crate) use commit_types::owner_uncertain;
