//! Kernel connection/request ownership over existing Workspace semantics.
#![forbid(unsafe_code)]

pub mod coherence;
mod dispatch;
pub mod operations;
pub mod phase;
pub mod ports;

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod attributes;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod mount;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod request;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod session;

pub use dispatch::{
    DispatchError, FailureView, InputFailure, MountRequests, MountWork, Received,
    RequestDisposition, RequestFuture, RequestOwner, MAX_INPUT_BYTES, RECEIVE_SLOTS,
};
