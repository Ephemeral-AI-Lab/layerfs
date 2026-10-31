//! macOS platform facts for the native runtime: the daemon serves loopback
//! NFS as this user, so its process facts come from `proc_pidinfo`/`sysctl`
//! and its mounts from `mount`/`umount -f`. Only facts live here; the Sandbox
//! lifecycle is shared with Linux in [`super::native`].
mod darwin;
pub(crate) mod mounts;
pub(crate) use darwin::{boot, force_unmount, mount_points, processes, stat};
