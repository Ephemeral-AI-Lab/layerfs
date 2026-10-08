//! Native connection/request ownership over existing Workspace semantics.
#![forbid(unsafe_code)]

mod dispatch;
pub mod operations;
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
    AdmissionFailure, Dispatch, DispatchConfig, DispatchError, DispatchWork, FailureView,
    MountQueue, MountWork, NextTurn, Permit, Received, RequestDisposition, RequestFuture, Shutdown,
    StartFailure, HANDOFFS, MAX_INPUT_BYTES, RECEIVE_SLOTS,
};
