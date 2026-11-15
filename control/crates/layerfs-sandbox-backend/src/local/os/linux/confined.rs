//! Linux's confined open and `stat` (layerfs-project#178 security rule 1,
//! isolation rule 5).
//!
//! The open is one `openat2` from the base's descriptor, so the kernel
//! itself refuses a path that leaves the base; a kernel before 5.6 has no
//! `openat2` and answers `ENOSYS`, which is reported as unsupported with no
//! other way tried. The `stat` asks for the entry itself and for what the
//! kernel already knows of it (`AT_STATX_DONT_SYNC`), so it never waits on
//! a remote filesystem's server.
use crate::local::confined::{Refused, View};
use nix::fcntl::{openat2, AtFlags, OFlag, OpenHow, ResolveFlag};
use nix::libc;
use nix::sys::stat::{fstatat, FileStat};
use std::ffi::OsStr;
use std::os::fd::{AsRawFd, BorrowedFd, OwnedFd};
use std::path::{Path, PathBuf};

/// How `view` resolves a path from its base. Another view (a whole root,
/// where links resolve inside it) is one more arm here.
fn resolve(view: View) -> ResolveFlag {
    match view {
        View::Beneath => {
            ResolveFlag::RESOLVE_BENEATH
                | ResolveFlag::RESOLVE_NO_SYMLINKS
                | ResolveFlag::RESOLVE_NO_MAGICLINKS
                | ResolveFlag::RESOLVE_NO_XDEV
        }
    }
}

/// Opens `relative` (only names; empty for the base itself) from `base`.
pub(crate) fn open(base: BorrowedFd<'_>, relative: &Path, view: View) -> Result<OwnedFd, Refused> {
    let flags = OFlag::O_RDONLY
        | OFlag::O_NONBLOCK
        | OFlag::O_NOFOLLOW
        | OFlag::O_CLOEXEC
        | OFlag::O_NOCTTY;
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
/// empty name. The system call is `newfstatat`, which takes the `statx`
/// query flags since Linux 4.11: nix has no `statx` wrapper.
pub(crate) fn stat(folder: BorrowedFd<'_>, name: &OsStr) -> nix::Result<FileStat> {
    let mut flags = AtFlags::AT_SYMLINK_NOFOLLOW
        | AtFlags::AT_NO_AUTOMOUNT
        | AtFlags::from_bits_retain(libc::AT_STATX_DONT_SYNC);
    if name.is_empty() {
        flags |= AtFlags::AT_EMPTY_PATH;
    }
    fstatat(folder, name, flags)
}
