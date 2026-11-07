//! macOS platform facts for the native runtime: the daemon serves loopback
//! NFS as this user, so its mounts come from `mount`/`umount -f` (its
//! process facts, `proc_pidinfo`/`sysctl`, are `layerfs_bridge::process`).
//! Only facts live here; the Sandbox lifecycle is shared with Linux in
//! [`super`].
mod darwin;
pub(crate) mod mounts;
pub(crate) use darwin::{force_unmount, mount_points};
