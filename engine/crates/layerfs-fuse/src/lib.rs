//! Kernel connection/request ownership over existing Workspace semantics.
#![forbid(unsafe_code)]

mod dispatch;

#[cfg(target_os = "linux")]
pub mod attributes;
#[cfg(target_os = "linux")]
pub mod request;
#[cfg(target_os = "linux")]
pub mod session;
#[cfg(target_os = "linux")]
pub(crate) mod mount;

pub use dispatch::{
    DispatchError, FailureView, InputFailure, MountRequests, MountWork, Received,
    RequestDisposition, RequestFuture, RequestOwner, MAX_INPUT_BYTES, RECEIVE_SLOTS,
};

// Linux mount profile, first-party mount controls and negotiation facts.

pub(crate) use mount::negotiate;
pub use mount::Negotiation;
pub use mount::{control_root, AbortWrite, MountEntry, CONTROL};
