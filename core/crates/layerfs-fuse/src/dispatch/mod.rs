//! One shared native request pool; SQL scheduling stays with the engine owner.
mod admission;
mod diagnostics;
mod queue;
mod task;
mod types;
mod workers;

pub use admission::{AdmissionFailure, MountQueue, Permit, Received};
pub use diagnostics::{DispatchWork, MountWork};
pub use task::NextTurn;
pub use types::{
    DispatchConfig, DispatchError, FailureView, RequestDisposition, RequestFuture, Shutdown,
    StartFailure, HANDOFFS, MAX_INPUT_BYTES, RECEIVE_SLOTS,
};
pub use workers::Dispatch;
