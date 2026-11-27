//! Request ownership beside each mount; no daemon-wide request dispatcher.
mod disposition;
mod requests;
mod work;
pub use disposition::{
    DispatchError, FailureView, RequestDisposition, RequestFuture, MAX_INPUT_BYTES, RECEIVE_SLOTS,
};
pub use requests::{InputFailure, MountRequests, Received, RequestOwner};
pub use work::MountWork;
