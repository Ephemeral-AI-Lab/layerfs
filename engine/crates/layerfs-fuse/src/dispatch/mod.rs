//! Request ownership beside each mount; no daemon-wide request dispatcher.
mod diagnostics;
mod requests;
mod types;
pub use diagnostics::MountWork;
pub use requests::{InputFailure, MountRequests, Received, RequestOwner};
pub use types::{
    DispatchError, FailureView, RequestDisposition, RequestFuture, MAX_INPUT_BYTES, RECEIVE_SLOTS,
};
