//! Linux's confined open and `stat` (layerfs-project#178 security rule 1,
//! isolation rule 5).
//!
//! The open is one `openat2` from the base's descriptor, so the kernel
//! itself refuses a path that leaves the base; a kernel before 5.6 has no
//! `openat2` and answers `ENOSYS`, which is reported as unsupported with no
//! other way tried. Every `stat` is one `statx` that asks for the entry
//! itself and for what the kernel already knows of it
//! (`AT_STATX_DONT_SYNC`), so it never waits on a remote filesystem's
//! server, and for the id of the mount it is in (`STATX_MNT_ID`).
use crate::local::confined::{Kind, Refused, Stat, View};
use nix::errno::Errno;
use nix::fcntl::{openat2, OFlag, OpenHow, ResolveFlag};
use rustix::fs::{statx, AtFlags, StatxFlags};
use std::ffi::OsStr;
use std::os::fd::{AsRawFd, BorrowedFd, OwnedFd};
use std::path::{Path, PathBuf};

/// How `view` resolves a path from its base.
fn resolve(view: View) -> ResolveFlag {
    match view {
        View::Beneath => {
            ResolveFlag::RESOLVE_BENEATH
                | ResolveFlag::RESOLVE_NO_SYMLINKS
                | ResolveFlag::RESOLVE_NO_MAGICLINKS
                | ResolveFlag::RESOLVE_NO_XDEV
        }
        // links resolve inside the base; which mounts it may end in is
        // checked on what was opened, by its mount id
        View::InRoot => ResolveFlag::RESOLVE_IN_ROOT | ResolveFlag::RESOLVE_NO_MAGICLINKS,
    }
}

/// Opens `relative` (only names; empty for the base itself) from `base`.
/// `through` opens a folder a path leads through: a link at the end is
/// then resolved as one on the way is, where the view resolves any.
pub(crate) fn open(
    base: BorrowedFd<'_>,
    relative: &Path,
    view: View,
    through: bool,
) -> Result<OwnedFd, Refused> {
    let mut flags = OFlag::O_RDONLY | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC | OFlag::O_NOCTTY;
    flags |= match through {
        true => OFlag::O_DIRECTORY,
        false => OFlag::O_NOFOLLOW,
    };
    let how = OpenHow::new().flags(flags).resolve(resolve(view));
    let path = match relative.as_os_str().is_empty() {
        true => Path::new("."),
        false => relative,
    };
    Ok(openat2(base, path, how)?)
}

/// The path of what `opened` holds now: its `/proc/self/fd` link, read and
/// not followed.
pub(crate) fn path_of(opened: BorrowedFd<'_>) -> std::io::Result<PathBuf> {
    std::fs::read_link(format!("/proc/self/fd/{}", opened.as_raw_fd()))
}

/// The `stat` of entry `name` of `folder`, or of `folder` itself for an
/// empty name: one `statx`, of the entry itself, that neither triggers an
/// automount nor asks a remote server.
pub(crate) fn stat(folder: BorrowedFd<'_>, name: &OsStr) -> nix::Result<Stat> {
    let mut flags = AtFlags::SYMLINK_NOFOLLOW | AtFlags::NO_AUTOMOUNT | AtFlags::STATX_DONT_SYNC;
    if name.is_empty() {
        flags |= AtFlags::EMPTY_PATH;
    }
    let asked = StatxFlags::BASIC_STATS | StatxFlags::MNT_ID;
    let found =
        statx(folder, name, flags, asked).map_err(|errno| Errno::from_raw(errno.raw_os_error()))?;
    let mode = u32::from(found.stx_mode);
    Ok(Stat {
        kind: Kind::of_mode(mode),
        size: found.stx_size,
        mode: mode & 0o7777,
        mtime_ms: (found.stx_mtime.tv_sec)
            .saturating_mul(1000)
            .saturating_add(i64::from(found.stx_mtime.tv_nsec) / 1_000_000),
        owner: found.stx_uid,
        links: u64::from(found.stx_nlink),
        device: nix::sys::stat::makedev(
            u64::from(found.stx_dev_major),
            u64::from(found.stx_dev_minor),
        ),
        mount: (found.stx_mask & StatxFlags::MNT_ID.bits() != 0).then_some(found.stx_mnt_id),
    })
}
