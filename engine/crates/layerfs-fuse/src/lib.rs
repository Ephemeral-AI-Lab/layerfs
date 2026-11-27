//! Kernel connection/request ownership over existing Workspace semantics.
#![forbid(unsafe_code)]

mod dispatch;

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

// #828 compat: old public module paths, removed in wave 3.
#[doc(hidden)]
#[cfg(target_os = "linux")]
pub mod coherence {
    // #828 compat
    #[doc(hidden)]
    pub use crate::request::coherence as attributes; // #828 compat
    #[doc(hidden)]
    pub use crate::request::coherence as pages; // #828 compat
    #[doc(hidden)]
    pub use crate::request::coherence as reply_order; // #828 compat
}
