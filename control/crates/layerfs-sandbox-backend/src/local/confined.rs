//! Reads of a folder tree that stay inside it (layerfs-project#178 security
//! rules 1 to 3 and 8): a [`Base`] folder, opens of paths beneath it that
//! never follow a link or leave it ([`Base::open_at`]), a `stat` that
//! follows nothing ([`Opened::stat_of`]) and this process's mount table
//! ([`mounts`]). An inspection read uses nothing else of the filesystem, so
//! what a platform must supply is exactly these three, in
//! `os/{linux,macos}/confined.rs` and `mounts.rs`.
//!
//! Every open is read-only and `O_NONBLOCK`, so a FIFO put in a file's
//! place is never waited on, and is followed by a `stat` of the descriptor:
//! only a regular file or a folder is answered. Nothing here takes a lock
//! or writes.
use super::os;
use nix::dir::Dir;
use nix::errno::Errno;
use nix::fcntl::OFlag;
use nix::sys::stat::{FileStat, Mode, SFlag};
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, Seek, SeekFrom};
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::fs::FileExt;
use std::path::{Component, Path, PathBuf};

/// How far an open may go from its base.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    /// Beneath the base and nowhere else: no symbolic link on the way or
    /// at the end, no magic link, no other mount. A Sandbox's home.
    Beneath,
}

/// What an entry is, from its `stat` alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A regular file.
    File,
    /// A folder.
    Folder,
    /// A symbolic link, never followed.
    Symlink,
    /// A named pipe.
    Fifo,
    /// A socket.
    Socket,
    /// A character or block device.
    Device,
    /// Anything else.
    Other,
}

/// One `stat`, of the entry itself.
#[derive(Clone, Copy, Debug)]
pub struct Stat {
    /// What it is.
    pub kind: Kind,
    /// Its size in bytes.
    pub size: u64,
    /// Its permission bits.
    pub mode: u32,
    /// When it was last modified, in milliseconds since 1970.
    pub mtime_ms: i64,
    /// Its owner's numeric user id.
    pub owner: u32,
    /// How many names it has.
    pub links: u64,
    /// The device of the filesystem that holds it.
    pub device: u64,
}

impl Stat {
    // the fields of `stat` differ in width between Linux and macOS
    #[allow(clippy::unnecessary_cast, clippy::useless_conversion)]
    pub(crate) fn of(found: &FileStat) -> Self {
        let kind = match SFlag::from_bits_truncate(found.st_mode) & SFlag::S_IFMT {
            SFlag::S_IFREG => Kind::File,
            SFlag::S_IFDIR => Kind::Folder,
            SFlag::S_IFLNK => Kind::Symlink,
            SFlag::S_IFIFO => Kind::Fifo,
            SFlag::S_IFSOCK => Kind::Socket,
            SFlag::S_IFCHR | SFlag::S_IFBLK => Kind::Device,
            _ => Kind::Other,
        };
        Self {
            kind,
            size: u64::try_from(found.st_size).unwrap_or(0),
            mode: (found.st_mode as u32) & 0o7777,
            mtime_ms: (found.st_mtime as i64)
                .saturating_mul(1000)
                .saturating_add((found.st_mtime_nsec as i64) / 1_000_000),
            owner: found.st_uid,
            links: found.st_nlink as u64,
            device: found.st_dev as u64,
        }
    }
}

/// Why a confined open or `stat` answered nothing.
#[derive(Debug)]
pub enum Refused {
    /// Nothing has that name.
    Missing,
    /// A symbolic link is on the way or at the end; none is followed.
    Symlink,
    /// The path leaves the base: a `..`, another mount, or a folder moved
    /// out while it was walked.
    Outside,
    /// It is neither a regular file nor a folder.
    Special(Kind),
    /// This kernel cannot make a confined open; the reason.
    Unsupported(String),
    /// The system call failed.
    Failed(io::Error),
}

impl From<Errno> for Refused {
    fn from(errno: Errno) -> Self {
        match errno {
            Errno::ENOENT | Errno::ENOTDIR => Self::Missing,
            Errno::ELOOP => Self::Symlink,
            Errno::EXDEV => Self::Outside,
            // what an open of a socket answers (Linux, macOS)
            Errno::ENXIO | Errno::EOPNOTSUPP => Self::Special(Kind::Socket),
            Errno::ENOSYS => Self::Unsupported(
                "this kernel has no openat2 (Linux 5.6), which a confined read needs".to_owned(),
            ),
            other => Self::Failed(other.into()),
        }
    }
}

/// A folder reads stay inside.
#[derive(Debug)]
pub struct Base {
    folder: OwnedFd,
    view: View,
}

impl Base {
    /// Opens `folder` as a base for `view`. The path is the caller's own
    /// (a folder LayerFS made); its last component is not followed when it
    /// is a link.
    pub fn open(folder: &Path, view: View) -> Result<Self, Refused> {
        let flags = OFlag::O_RDONLY
            | OFlag::O_DIRECTORY
            | OFlag::O_NOFOLLOW
            | OFlag::O_NONBLOCK
            | OFlag::O_CLOEXEC;
        let folder = nix::fcntl::open(folder, flags, Mode::empty())?;
        Ok(Self { folder, view })
    }

    /// The base's real path, as the mount table names paths: read from
    /// its descriptor, not resolved again.
    pub fn path(&self) -> io::Result<PathBuf> {
        os::confined::path_of(self.folder.as_fd())
    }

    /// Opens `relative` beneath the base, read-only: the base itself for an
    /// empty path. A path with a `..` or a root is [`Refused::Outside`]
    /// before any system call. What is opened is `stat`ed through its
    /// descriptor, and only a regular file or a folder is answered.
    pub fn open_at(&self, relative: &Path) -> Result<Opened, Refused> {
        let mut clean = PathBuf::new();
        for component in relative.components() {
            match component {
                Component::Normal(name) => clean.push(name),
                Component::CurDir => {}
                _ => return Err(Refused::Outside),
            }
        }
        let opened = os::confined::open(self.folder.as_fd(), &clean, self.view)?;
        let stat = Stat::of(&os::confined::stat(opened.as_fd(), OsStr::new(""))?);
        match stat.kind {
            Kind::File | Kind::Folder => Ok(Opened {
                file: File::from(opened),
                stat,
            }),
            other => Err(Refused::Special(other)),
        }
    }
}

/// A regular file or a folder opened beneath a [`Base`].
#[derive(Debug)]
pub struct Opened {
    file: File,
    /// Its `stat`, taken through the descriptor after the open.
    pub stat: Stat,
}

impl Opened {
    /// The `stat` of entry `name` of this folder: of the entry itself,
    /// never of what a link names or of an automount it would trigger.
    pub fn stat_of(&self, name: &OsStr) -> Result<Stat, Refused> {
        if name.is_empty() {
            return Err(Refused::Missing);
        }
        Ok(Stat::of(&os::confined::stat(self.file.as_fd(), name)?))
    }

    /// What symbolic link `name` of this folder names; it is not followed.
    pub fn target_of(&self, name: &OsStr) -> io::Result<OsString> {
        Ok(nix::fcntl::readlinkat(self.file.as_fd(), name)?)
    }

    /// Hands `each` the name of every entry of this folder, in the order
    /// the filesystem lists them, until it answers `false`. Names only: no
    /// entry is `stat`ed.
    pub fn names(&self, each: &mut dyn FnMut(&[u8]) -> bool) -> io::Result<()> {
        // the listing shares this descriptor's position: from the start
        let mut listed = self.file.try_clone()?;
        listed.seek(SeekFrom::Start(0))?;
        let mut folder = Dir::from_fd(OwnedFd::from(listed))?;
        for entry in folder.iter() {
            let entry = entry?;
            let name = entry.file_name().to_bytes();
            if name != b"." && name != b".." && !each(name) {
                break;
            }
        }
        Ok(())
    }

    /// Reads at most `bytes.len()` bytes of this file at `offset`, and
    /// nothing outside that window.
    pub fn read_at(&self, bytes: &mut [u8], offset: u64) -> io::Result<usize> {
        self.file.read_at(bytes, offset)
    }
}

/// One mount of this process's mount namespace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mount {
    /// Where it is mounted.
    pub point: PathBuf,
    /// Its filesystem type (`ext4`, `fuse.layerfs`, `nfs`).
    pub fs: String,
}

/// The mounts of this process's mount namespace, read from the kernel's
/// table without touching any of them: a walk reports a mount point from
/// this list and never `stat`s it, so a mount whose server is stopped
/// cannot hang the listing of the folder that holds it (rule 8).
///
/// Linux reads `/proc/self/mountinfo`. macOS has no table here yet: its
/// table is `getfsstat(MNT_NOWAIT)`, which no dependency of this crate
/// wraps safely, so this answers no mounts there and a walk tells a mount
/// point only by its device, after a `stat` of it.
pub fn mounts() -> io::Result<Vec<Mount>> {
    #[cfg(target_os = "linux")]
    {
        let table = os::mounts::table(Path::new("/proc/self/mountinfo"))?;
        Ok(table
            .into_iter()
            .map(|(point, fs)| Mount { point, fs })
            .collect())
    }
    #[cfg(target_os = "macos")]
    {
        Ok(Vec::new())
    }
}
