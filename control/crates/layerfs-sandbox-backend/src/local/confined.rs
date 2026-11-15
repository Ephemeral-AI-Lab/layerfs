//! Reads of a folder tree that stay inside it (layerfs-project#178 security
//! rules 1 to 3 and 8): a [`Base`] folder, opens of paths beneath it that
//! never leave it ([`Base::open_at`], [`Base::enter`]), a `stat` that
//! follows nothing ([`Opened::stat_of`]) and a mount table ([`mounts`],
//! [`parse_mounts`]). An inspection read uses nothing else of the
//! filesystem, so what a platform must supply is exactly these three, in
//! `os/{linux,macos}/confined.rs` and `mounts.rs`.
//!
//! A [`View`] says how far an open goes: [`View::Beneath`] follows no link
//! and enters no other mount; [`View::InRoot`] resolves links inside the
//! base, as a process whose root the base is would, and ends only in the
//! mounts its caller allowed ([`Base::cross`]).
//!
//! Every open is read-only and `O_NONBLOCK`, so a FIFO put in a file's
//! place is never waited on, and is followed by a `stat` of the descriptor:
//! only a regular file or a folder is answered. Nothing here takes a lock
//! or writes.
use super::os;
use nix::dir::Dir;
use nix::errno::Errno;
use nix::fcntl::OFlag;
use nix::sys::stat::{Mode, SFlag};
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
    /// at the end, no magic link, no other mount. A Sandbox's home, a
    /// LayerFS folder of a machine.
    Beneath,
    /// Inside the base as a root (a machine's `/`, a Sandbox's root): a
    /// symbolic link on the way resolves inside it (an image's `/lib` is a
    /// link to `usr/lib`), the last name is never followed, no magic link
    /// is, and what is opened must be in a mount the caller allowed
    /// ([`Base::cross`]), which its `statx` mount id says. macOS has no
    /// such resolution: there this view opens as [`View::Beneath`] does.
    InRoot,
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
    /// The id of the mount it is in, as the mount table numbers mounts
    /// (`statx` `STATX_MNT_ID`, Linux 5.8); none on macOS or an older
    /// kernel.
    pub mount: Option<u64>,
}

impl Stat {
    /// Whether this and `entry`, an entry of this folder, are in two
    /// mounts: by their mount ids where the kernel gives them, else by
    /// their devices. (A device alone is not a mount on Linux: a file of an
    /// overlay's lower layer, or a btrfs subvolume, has its own.)
    pub fn apart_from(&self, entry: &Stat) -> bool {
        match (self.mount, entry.mount) {
            (Some(folder), Some(entry)) => folder != entry,
            _ => self.device != entry.device,
        }
    }
}

impl Kind {
    /// What the type bits of a `stat` mode say.
    // the mode's width differs between Linux and macOS
    #[allow(clippy::unnecessary_cast, clippy::useless_conversion)]
    pub(crate) fn of_mode(mode: u32) -> Self {
        match SFlag::from_bits_truncate(mode as nix::libc::mode_t) & SFlag::S_IFMT {
            SFlag::S_IFREG => Kind::File,
            SFlag::S_IFDIR => Kind::Folder,
            SFlag::S_IFLNK => Kind::Symlink,
            SFlag::S_IFIFO => Kind::Fifo,
            SFlag::S_IFSOCK => Kind::Socket,
            SFlag::S_IFCHR | SFlag::S_IFBLK => Kind::Device,
            _ => Kind::Other,
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
    /// What was opened is in a mount the view does not read: one its
    /// caller did not allow ([`Base::cross`]).
    Mount,
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
    /// [`View::InRoot`]: the ids of the mounts an open may end in. The
    /// base's own, and those its caller added.
    crossed: Vec<u64>,
}

impl Base {
    /// Opens `folder` as a base for `view`. The path is the caller's own
    /// (a folder LayerFS made, or `/`); its last component is not followed
    /// when it is a link.
    pub fn open(folder: &Path, view: View) -> Result<Self, Refused> {
        let flags = OFlag::O_RDONLY
            | OFlag::O_DIRECTORY
            | OFlag::O_NOFOLLOW
            | OFlag::O_NONBLOCK
            | OFlag::O_CLOEXEC;
        Self::over(nix::fcntl::open(folder, flags, Mode::empty())?, view)
    }

    /// A base over the folder `folder` holds, for `view`. An
    /// [`View::InRoot`] base starts with its own mount as the only one an
    /// open may end in; a Linux kernel that names no mount id (before 5.8)
    /// has no such view.
    pub fn over(folder: OwnedFd, view: View) -> Result<Self, Refused> {
        let own = os::confined::stat(folder.as_fd(), OsStr::new(""))?.mount;
        if view == View::InRoot && own.is_none() && cfg!(target_os = "linux") {
            return Err(Refused::Unsupported(NO_MOUNT_ID.to_owned()));
        }
        Ok(Self {
            folder,
            view,
            crossed: own.into_iter().collect(),
        })
    }

    /// The id of the mount the base is in, where the kernel gives one.
    pub fn mount(&self) -> Option<u64> {
        self.crossed.first().copied()
    }

    /// Lets opens of an [`View::InRoot`] base end in the mounts `mounts`
    /// name by id too: the ones its caller read from the mount table of
    /// the base's namespace and judged safe to read.
    pub fn cross(&mut self, mounts: impl IntoIterator<Item = u64>) {
        self.crossed.extend(mounts);
    }

    /// The base's real path, as the mount table names paths: read from
    /// its descriptor, not resolved again.
    pub fn path(&self) -> io::Result<PathBuf> {
        os::confined::path_of(self.folder.as_fd())
    }

    /// Opens `relative` beneath the base, read-only: the base itself for an
    /// empty path. A path with a `..` or a root is [`Refused::Outside`]
    /// before any system call. Its last name is never followed. What is
    /// opened is `stat`ed through its descriptor, and only a regular file
    /// or a folder is answered; in an [`View::InRoot`] base, only one in a
    /// mount its caller allowed.
    pub fn open_at(&self, relative: &Path) -> Result<Opened, Refused> {
        self.opened(relative, false)
    }

    /// Opens the folder at `relative` as a path leads through it: as
    /// [`Base::open_at`], but in an [`View::InRoot`] base a link at the
    /// end is resolved inside the base like one on the way (`/lib` of a
    /// merged-`/usr` image is entered as `usr/lib`). Anything but a folder
    /// is [`Refused::Missing`].
    pub fn enter(&self, relative: &Path) -> Result<Opened, Refused> {
        self.opened(relative, true)
    }

    fn opened(&self, relative: &Path, through: bool) -> Result<Opened, Refused> {
        let mut clean = PathBuf::new();
        for component in relative.components() {
            match component {
                Component::Normal(name) => clean.push(name),
                Component::CurDir => {}
                _ => return Err(Refused::Outside),
            }
        }
        let opened = os::confined::open(self.folder.as_fd(), &clean, self.view, through)?;
        let stat = os::confined::stat(opened.as_fd(), OsStr::new(""))?;
        if self.view == View::InRoot && cfg!(target_os = "linux") {
            match stat.mount {
                None => return Err(Refused::Unsupported(NO_MOUNT_ID.to_owned())),
                Some(mount) if !self.crossed.contains(&mount) => return Err(Refused::Mount),
                Some(_) => {}
            }
        }
        match stat.kind {
            Kind::Folder => {}
            Kind::File if !through => {}
            Kind::File => return Err(Refused::Missing),
            other => return Err(Refused::Special(other)),
        }
        Ok(Opened {
            file: File::from(opened),
            stat,
        })
    }
}

/// Why a Linux kernel before 5.8 has no [`View::InRoot`].
const NO_MOUNT_ID: &str =
    "this kernel's statx names no mount id (Linux 5.8), which a read across mounts needs";

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
        Ok(os::confined::stat(self.file.as_fd(), name)?)
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

/// One mount of a mount namespace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mount {
    /// Its id in the kernel's table, which a `statx` of what is in it
    /// names (Linux); none on macOS.
    pub id: Option<u64>,
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
/// Linux reads `/proc/self/mountinfo`. macOS runs `/sbin/mount` once, for
/// at most half a second (`getfsstat(MNT_NOWAIT)` is the system call for
/// it, and no dependency of this crate wraps it without `unsafe`); a run
/// that fails or does not end in time answers no mounts, and a walk then
/// tells a mount point only by its device, after a `stat` of it.
pub fn mounts() -> io::Result<Vec<Mount>> {
    #[cfg(target_os = "linux")]
    {
        Ok(parse_mounts(&std::fs::read_to_string(
            "/proc/self/mountinfo",
        )?))
    }
    #[cfg(target_os = "macos")]
    {
        Ok(os::mount_table()
            .map(|table| parse_mounts(&table))
            .unwrap_or_default())
    }
}

/// The mounts a mount table's text names, in table order: a `mountinfo`
/// file on Linux (this process's, or another process's for its
/// namespace), the output of `/sbin/mount` on macOS. Where several mounts
/// are at one point, the last is the one a path reaches.
pub fn parse_mounts(table: &str) -> Vec<Mount> {
    #[cfg(target_os = "linux")]
    let found = (os::mounts::table(table).into_iter()).map(|(id, point, fs)| Mount {
        id: Some(id),
        point,
        fs,
    });
    #[cfg(target_os = "macos")]
    let found = (table.lines().filter_map(os::mount_line)).map(|(point, fs)| Mount {
        id: None,
        point,
        fs,
    });
    found.collect()
}
