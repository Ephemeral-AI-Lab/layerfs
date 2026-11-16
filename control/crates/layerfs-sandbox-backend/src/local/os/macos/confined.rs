//! macOS's confined open and `stat` (layerfs-project#178 security rule 3).
//!
//! macOS has no `openat2`, so a path is opened one name at a time, each
//! from the folder before it with `O_NOFOLLOW`: no link is followed on the
//! way or at the end. Each folder on the way must be on the base's device,
//! and the descriptor at the end must still have a path under the base's
//! (`F_GETPATH`), which a folder moved out during the walk does not.
//!
//! Both views open this way. macOS runs no machine's worker and no
//! Sandbox with a root of its own, so [`View::InRoot`] has nothing to
//! resolve links in here, and a link is refused in it as in the other.
use crate::local::confined::{Kind, Refused, Stat, View};
use nix::dir::Dir;
use nix::fcntl::{fcntl, openat, AtFlags, FcntlArg, OFlag};
use nix::sys::stat::{fstat, fstatat, Mode};
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
/// A folder a path leads through (`_through`) is opened as any entry is:
/// no link is resolved here, and its caller checks that it is a folder.
pub(crate) fn open(
    base: BorrowedFd<'_>,
    relative: &Path,
    view: View,
    _through: bool,
) -> Result<OwnedFd, Refused> {
    let (View::Beneath | View::InRoot) = view;
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
pub(crate) fn stat(folder: BorrowedFd<'_>, name: &OsStr) -> nix::Result<Stat> {
    let found = match name.is_empty() {
        true => fstat(folder)?,
        false => fstatat(folder, name, AtFlags::AT_SYMLINK_NOFOLLOW)?,
    };
    Ok(Stat {
        kind: Kind::of_mode(u32::from(found.st_mode)),
        size: u64::try_from(found.st_size).unwrap_or(0),
        mode: u32::from(found.st_mode) & 0o7777,
        mtime_ms: (found.st_mtime)
            .saturating_mul(1000)
            .saturating_add(found.st_mtime_nsec / 1_000_000),
        owner: found.st_uid,
        links: u64::from(found.st_nlink),
        // a device number is an `i32` here
        device: u64::from(found.st_dev as u32),
        mount: None,
    })
}

/// Hands `each` every name of the folder `listed` holds, until it answers
/// `false`.
pub(crate) fn names(listed: OwnedFd, each: &mut dyn FnMut(&[u8]) -> bool) -> std::io::Result<()> {
    let mut folder = Dir::from_fd(listed)?;
    for entry in folder.iter() {
        if !each(entry?.file_name().to_bytes()) {
            break;
        }
    }
    Ok(())
}
