//! Request ownership beside each mount; no daemon-wide request dispatcher.
pub(crate) mod work;
mod requests;
pub(crate) mod disposition;
pub use work::MountWork;
pub use requests::{InputFailure, MountRequests, Received, RequestOwner};
pub use disposition::{DispatchError, FailureView, RequestDisposition, RequestFuture, MAX_INPUT_BYTES, RECEIVE_SLOTS};
