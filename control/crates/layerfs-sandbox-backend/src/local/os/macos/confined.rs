//! macOS's confined open and `stat` (layerfs-project#178 security rule 3).
//!
//! macOS has no `openat2`, so a path is opened one name at a time, each
//! from the folder before it with `O_NOFOLLOW`: no link is followed on the
//! way or at the end. Each folder on the way must be on the base's device,
//! and the descriptor at the end must still have a path under the base's
//! (`F_GETPATH`), which a folder moved out during the walk does not.
use crate::local::confined::{Refused, View};
use nix::fcntl::{fcntl, openat, AtFlags, FcntlArg, OFlag};
use nix::sys::stat::{fstat, fstatat, FileStat, Mode};
use std::ffi::OsStr;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::{Path, PathBuf};

/// The path of what `opened` holds now (`F_GETPATH`).
pub(crate) fn path_of(opened: BorrowedFd<'_>) -> std::io::Result<PathBuf> {
    let mut path = PathBuf::new();
    fcntl(opened, FcntlArg::F_GETPATH(&mut path))?;
    Ok(path)
}

/// Opens `relative` (only names; empty for the base itself) from `base`.
pub(crate) fn open(base: BorrowedFd<'_>, relative: &Path, view: View) -> Result<OwnedFd, Refused> {
    let View::Beneath = view;
    let flags = OFlag::O_RDONLY | OFlag::O_NONBLOCK | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
    let device = fstat(base)?.st_dev;
    let mut at = openat(base, ".", flags | OFlag::O_DIRECTORY, Mode::empty())?;
    let mut names = relative.components().peekable();
    while let Some(name) = names.next() {
        let next = openat(at.as_fd(), name.as_os_str(), flags, Mode::empty())?;
        // a folder on the way that is another mount's root is not entered
        if names.peek().is_some() && fstat(next.as_fd())?.st_dev != device {
            return Err(Refused::Outside);
        }
        at = next;
    }
    let under = |path: std::io::Result<PathBuf>| path.map_err(Refused::Failed);
    if fstat(at.as_fd())?.st_dev != device
        || !under(path_of(at.as_fd()))?.starts_with(under(path_of(base))?)
    {
        return Err(Refused::Outside);
    }
    Ok(at)
}

/// The `stat` of entry `name` of `folder`, or of `folder` itself for an
/// empty name.
pub(crate) fn stat(folder: BorrowedFd<'_>, name: &OsStr) -> nix::Result<FileStat> {
    match name.is_empty() {
        true => fstat(folder),
        false => fstatat(folder, name, AtFlags::AT_SYMLINK_NOFOLLOW),
    }
}
