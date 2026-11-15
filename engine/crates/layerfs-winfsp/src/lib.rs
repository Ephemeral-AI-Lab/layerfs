//! The Workspace filesystem face on Windows: WinFsp file system callbacks
//! over the platform-neutral [`layerfs_mount_ports`] services, as
//! `layerfs-fuse` does for FUSE (#454 A2). The FFI to winfsp.dll is
//! hand-written - no new dependency - beside its import library.

mod ffi;

pub mod mount;

// The callbacks and the mount loop land with their steps; the skeleton
// wires the crate into the workspace first.

