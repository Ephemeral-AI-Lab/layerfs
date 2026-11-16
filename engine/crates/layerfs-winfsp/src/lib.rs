//! The Workspace filesystem face on Windows: WinFsp file system callbacks
//! over the platform-neutral [`layerfs_mount_ports`] services, as
//! `layerfs-fuse` does for FUSE (#454 A2). The FFI to winfsp.dll is
//! hand-written - no new dependency - beside its import library.

pub mod ffi;
pub mod callbacks;
pub mod data;
pub mod write;
pub mod directory;
pub mod services;

pub mod mount;
pub use mount::{mount, Mounted};

// The callbacks and the mount loop land with their steps; the skeleton
// wires the crate into the workspace first.

