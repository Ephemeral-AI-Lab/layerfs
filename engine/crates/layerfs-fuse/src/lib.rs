//! Kernel connection/request ownership over existing Workspace semantics.
#![forbid(unsafe_code)]

pub mod coherence;
mod dispatch;
pub mod operations;
pub mod phase;
pub mod ports;

#[cfg(target_os = "linux")]
pub mod attributes;
#[cfg(target_os = "linux")]
pub mod mount;
#[cfg(target_os = "linux")]
pub mod request;
#[cfg(target_os = "linux")]
pub mod session;

pub use dispatch::{
    DispatchError, FailureView, InputFailure, MountRequests, MountWork, Received,
    RequestDisposition, RequestFuture, RequestOwner, MAX_INPUT_BYTES, RECEIVE_SLOTS,
};
