//! The route-neutral kernel ports (`ports`), operation handlers
//! (`operations`) and phase timing (`phase`) that FUSE and NFS both serve
//! over Workspace semantics. No kernel protocol lives here.
#![forbid(unsafe_code)]

/// Prototype (experiment `passthrough`): see the module.
pub mod decoded;
pub mod operations;
pub mod phase;
pub mod ports;
