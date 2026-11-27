//! macOS platform facts for the process runtime: the daemon serves loopback
//! NFS as this user, so its mounts come from `mount`/`umount -f` (its
//! process facts, `proc_pidinfo`/`sysctl`, are `layerfs_os::process`).
//! Only facts live here; the daemon lifecycle both backends use is
//! `crate::shared`, which reaches this module as `os`. `confined` is the open and `stat` of an inspection read
//! (layerfs-project#178), whose mount table is `mount` run under a
//! deadline (`darwin::mount_table`).
pub(crate) mod confined;
pub(crate) mod mount_table;
pub(crate) mod mounts;
pub(crate) use mount_table::{force_unmount, mount_line, mount_points, mount_table};
