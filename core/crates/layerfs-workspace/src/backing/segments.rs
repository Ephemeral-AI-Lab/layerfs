//! Immutable payload segment I/O. Allocation and cleanup ownership belong to the caller.
use layerfs_bridge::contract::MAX_FILE;
use std::{fs::File, io, path::Path};

pub(crate) const ALIGN: usize = 4096;
pub(crate) const DATA_BYTES: usize = 1_048_576;
pub(crate) const WINDOW_BYTES: usize = 131_072;
const MAGIC: &[u8; 8] = b"LFSWPLD1";
const VERSION: u16 = 1;
const FIELDS_END: usize = 80;

#[repr(align(4096))]
pub(crate) struct Window(pub [u8; WINDOW_BYTES]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Header {
    pub incarnation: [u8; 32],
    pub payload: u64,
    pub length: u64,
    pub index: u32,
    pub logical: u32,
}

/// Headers and aligned data for the complete declared payload; no sparse-byte claim.
pub(crate) fn planned(length: u64) -> io::Result<u64> {
    if length > MAX_FILE {
        return Err(invalid("payload length exceeds the logical input bound"));
    }
    let data = length
        .checked_add(ALIGN as u64 - 1)
        .map(|bytes| bytes / ALIGN as u64 * ALIGN as u64)
        .ok_or_else(|| invalid("payload padding overflow"))?;
    u64::from(segment_count(length))
        .checked_mul(ALIGN as u64)
        .and_then(|headers| headers.checked_add(data))
        .ok_or_else(|| invalid("payload allocation overflow"))
}

/// The caller first admits the length with `planned`.
pub(crate) fn segment_count(length: u64) -> u32 {
    assert!(length <= MAX_FILE, "payload length must first be admitted");
    length.div_ceil(DATA_BYTES as u64) as u32
}

impl Header {
    /// Writes exactly the header region; the remaining I/O window is untouched.
    pub fn fill(&self, window: &mut Window) {
        let bytes = &mut window.0[..ALIGN];
        bytes.fill(0);
        bytes[..8].copy_from_slice(MAGIC);
        bytes[8..10].copy_from_slice(&VERSION.to_be_bytes());
        bytes[10..12].copy_from_slice(&(ALIGN as u16).to_be_bytes());
        bytes[12..16].copy_from_slice(&(ALIGN as u32).to_be_bytes());
        bytes[16..20].copy_from_slice(&(DATA_BYTES as u32).to_be_bytes());
        bytes[20..24].copy_from_slice(&(WINDOW_BYTES as u32).to_be_bytes());
        bytes[24..56].copy_from_slice(&self.incarnation);
        bytes[56..64].copy_from_slice(&self.payload.to_be_bytes());
        bytes[64..72].copy_from_slice(&self.length.to_be_bytes());
        bytes[72..76].copy_from_slice(&self.index.to_be_bytes());
        bytes[76..80].copy_from_slice(&self.logical.to_be_bytes());
    }

    /// Checks an existing segment against its retained owner, never adopting it.
    pub fn verify(&self, window: &Window) -> io::Result<()> {
        planned(self.length)?;
        if self.incarnation == [0; 32]
            || self.payload == 0
            || self.length == 0
            || self.index >= segment_count(self.length)
        {
            return Err(invalid("invalid retained payload identity"));
        }
        let logical =
            (self.length - u64::from(self.index) * DATA_BYTES as u64).min(DATA_BYTES as u64) as u32;
        if self.logical != logical {
            return Err(invalid("invalid retained segment length"));
        }
        let bytes = &window.0[..ALIGN];
        if bytes[..8] != MAGIC[..]
            || bytes[8..10] != VERSION.to_be_bytes()
            || bytes[10..12] != (ALIGN as u16).to_be_bytes()
            || bytes[12..16] != (ALIGN as u32).to_be_bytes()
            || bytes[16..20] != (DATA_BYTES as u32).to_be_bytes()
            || bytes[20..24] != (WINDOW_BYTES as u32).to_be_bytes()
            || bytes[24..56] != self.incarnation
            || bytes[56..64] != self.payload.to_be_bytes()
            || bytes[64..72] != self.length.to_be_bytes()
            || bytes[72..76] != self.index.to_be_bytes()
            || bytes[76..80] != self.logical.to_be_bytes()
            || bytes[FIELDS_END..].iter().any(|byte| *byte != 0)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "payload header mismatch",
            ));
        }
        Ok(())
    }
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "payload backing requires the Linux direct-I/O profile",
    )
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use nix::{
        fcntl::{fallocate, openat, FallocateFlags, OFlag},
        sys::{
            stat::Mode,
            statfs::{fstatfs, EXT4_SUPER_MAGIC},
        },
        unistd::{geteuid, unlinkat, UnlinkatFlags},
    };
    use std::{
        fs,
        os::unix::fs::{FileExt, MetadataExt},
        path::Component,
    };

    fn os_error(error: nix::errno::Errno) -> io::Error {
        io::Error::from_raw_os_error(error as i32)
    }

    fn name(name: &str) -> io::Result<()> {
        if name.is_empty()
            || name.len() > 63
            || matches!(name, "." | "..")
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(invalid("invalid private segment name"));
        }
        Ok(())
    }

    fn io_range(offset: u64, length: usize) -> io::Result<()> {
        if length == 0
            || length > WINDOW_BYTES
            || length % ALIGN != 0
            || offset % ALIGN as u64 != 0
            || offset
                .checked_add(length as u64)
                .is_none_or(|end| end > (ALIGN + DATA_BYTES) as u64)
        {
            return Err(invalid("unaligned or oversized segment I/O"));
        }
        Ok(())
    }

    pub(crate) fn open_directory(path: &Path) -> io::Result<File> {
        if !path.is_absolute()
            || path
                .components()
                .any(|part| !matches!(part, Component::RootDir | Component::Normal(_)))
            || fs::canonicalize(path)? != path
        {
            return Err(invalid("private backing path must be canonical"));
        }
        let before = fs::symlink_metadata(path)?;
        let fd = nix::fcntl::open(
            path,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(os_error)?;
        let file = File::from(fd);
        let metadata = file.metadata()?;
        if !metadata.is_dir()
            || metadata.uid() != geteuid().as_raw()
            || metadata.mode() & 0o077 != 0
            || metadata.dev() != before.dev()
            || metadata.ino() != before.ino()
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "private backing directory ownership",
            ));
        }
        let filesystem = fstatfs(&file).map_err(os_error)?;
        // ext2/3 share this magic; the supported deployment also verifies ext4.
        if filesystem.filesystem_type() != EXT4_SUPER_MAGIC
            || u64::try_from(filesystem.block_size()).ok() != Some(ALIGN as u64)
        {
            return Err(unsupported());
        }
        Ok(file)
    }

    pub(crate) fn create(directory: &File, filename: &str) -> io::Result<File> {
        name(filename)?;
        let flags = OFlag::O_RDWR
            | OFlag::O_CREAT
            | OFlag::O_EXCL
            | OFlag::O_DIRECT
            | OFlag::O_NOFOLLOW
            | OFlag::O_CLOEXEC;
        openat(directory, filename, flags, Mode::S_IRUSR | Mode::S_IWUSR)
            .map(File::from)
            .map_err(os_error)
    }

    pub(crate) fn open(directory: &File, filename: &str) -> io::Result<File> {
        open_mode(directory, filename, false)
    }

    pub(crate) fn open_update(directory: &File, filename: &str) -> io::Result<File> {
        open_mode(directory, filename, true)
    }

    fn open_mode(directory: &File, filename: &str, update: bool) -> io::Result<File> {
        name(filename)?;
        let file = openat(
            directory,
            filename,
            (if update {
                OFlag::O_RDWR
            } else {
                OFlag::O_RDONLY
            }) | OFlag::O_DIRECT
                | OFlag::O_NOFOLLOW
                | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map(File::from)
        .map_err(os_error)?;
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || metadata.uid() != geteuid().as_raw()
            || metadata.mode() & 0o077 != 0
            || metadata.nlink() != 1
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "private payload file ownership",
            ));
        }
        Ok(file)
    }

    pub(crate) fn allocate(file: &File, length: u64) -> io::Result<u64> {
        if length < ALIGN as u64
            || length > (ALIGN + DATA_BYTES) as u64
            || length % ALIGN as u64 != 0
        {
            return Err(invalid("invalid segment allocation size"));
        }
        if file.metadata()?.len() != 0 {
            return Err(invalid("segment allocation requires a new file"));
        }
        fallocate(file, FallocateFlags::empty(), 0, length as i64).map_err(os_error)?;
        allocated(file)
    }

    pub(crate) fn allocated(file: &File) -> io::Result<u64> {
        file.metadata()?
            .blocks()
            .checked_mul(512)
            .ok_or_else(|| invalid("allocated block count overflow"))
    }

    pub(crate) fn read(
        file: &File,
        window: &mut Window,
        offset: u64,
        length: usize,
    ) -> io::Result<()> {
        io_range(offset, length)?;
        let count = file.read_at(&mut window.0[..length], offset)?;
        if count != length {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("short direct read: {count} of {length} bytes"),
            ));
        }
        Ok(())
    }

    pub(crate) fn write(
        file: &File,
        window: &Window,
        offset: u64,
        length: usize,
    ) -> io::Result<()> {
        io_range(offset, length)?;
        if offset + length as u64 > file.metadata()?.len() {
            return Err(invalid("direct write exceeds preallocated segment"));
        }
        let count = file.write_at(&window.0[..length], offset)?;
        if count != length {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                format!("short direct write: {count} of {length} bytes"),
            ));
        }
        Ok(())
    }

    pub(crate) fn unlink(directory: &File, filename: &str) -> io::Result<()> {
        name(filename)?;
        unlinkat(directory, filename, UnlinkatFlags::NoRemoveDir).map_err(os_error)
    }
}

#[cfg(target_os = "linux")]
pub(crate) use linux::{
    allocate, allocated, create, open, open_directory, open_update, read, unlink, write,
};

#[cfg(not(target_os = "linux"))]
pub(crate) fn open_directory(_: &Path) -> io::Result<File> {
    Err(unsupported())
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn create(_: &File, _: &str) -> io::Result<File> {
    Err(unsupported())
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn open(_: &File, _: &str) -> io::Result<File> {
    Err(unsupported())
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn allocate(_: &File, _: u64) -> io::Result<u64> {
    Err(unsupported())
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn allocated(_: &File) -> io::Result<u64> {
    Err(unsupported())
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn read(_: &File, _: &mut Window, _: u64, _: usize) -> io::Result<()> {
    Err(unsupported())
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn write(_: &File, _: &Window, _: u64, _: usize) -> io::Result<()> {
    Err(unsupported())
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn unlink(_: &File, _: &str) -> io::Result<()> {
    Err(unsupported())
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn open_update(_: &File, _: &str) -> io::Result<File> {
    Err(unsupported())
}
