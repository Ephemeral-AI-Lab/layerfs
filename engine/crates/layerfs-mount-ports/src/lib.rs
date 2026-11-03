//! The route-neutral kernel ports (`ports`), operation handlers
//! (`operations`) and phase timing (`phase`) that FUSE and NFS both serve
//! over Workspace semantics. No kernel protocol lives here.
#![forbid(unsafe_code)]

pub mod operations;
pub mod phase;
pub mod ports;
