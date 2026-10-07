//! In-process Store ports shared by one daemon's Workspaces.
mod bind;
mod commit;
mod commit_types;
mod open;
mod operation;
mod ports;
mod settle;
mod types;
pub(crate) use bind::checked_base;

pub use commit_types::{CommitError, CommitFailure, CommitPhase, CommitSuccess};
pub use open::{Store, StoreWork};
pub use operation::{BoundWorkspace, StoreOperation};
pub use ports::{PortError, StorePorts};
pub use types::{BindError, BindPhase, BindRefusal, BindRequest, BindSuccess};

pub(crate) use commit_types::owner_uncertain;
