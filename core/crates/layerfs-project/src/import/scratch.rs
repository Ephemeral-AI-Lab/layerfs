//! Operation-owned acquisition scratch and the records it holds.
//!
//! Entries, the directory frontier, wide-directory ordering, native regular-file
//! identities and every per-entry root live in ordering runs the content crate's
//! file backing owns and charges. The scratch is disposable: nothing is
//! synchronized, and completion releases every run and removes the directory
//! with a checked result.
use crate::error::content;
use crate::ProjectError as Failure;
use layerfs_content::filesystem::references::{FileBacking, OrderingBacking};
use layerfs_content::{inode_leaf::InodeKind, ObjectId};
use std::{
    ffi::OsString,
    fs::{self, Metadata},
    io,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{DirBuilderExt, MetadataExt},
    },
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);
/// Rows the job queue or one resident child buffer may hold.
pub(crate) const WINDOW_ROWS: usize = 512;

/// One private scratch directory and the byte account of its runs.
pub(crate) struct Scratch {
    directory: PathBuf,
    backing: FileBacking,
}
impl Scratch {
    /// Creates an empty private directory under `parent`.
    pub(crate) fn create(parent: &Path) -> Result<Self, Failure> {
        let directory = parent.join(format!(
            "import-{}-{}",
            std::process::id(),
            NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&directory)?;
        Ok(Self {
            // Physical capacity is the only ceiling: no total-input byte cap.
            backing: FileBacking::with_capacity(&directory, u64::MAX),
            directory,
        })
    }
    pub(crate) fn backing(&mut self) -> &mut dyn OrderingBacking {
        &mut self.backing
    }
    /// Largest simultaneous run bytes this scratch owned.
    pub(crate) fn peak_bytes(&self) -> u64 {
        self.backing.peak_bytes()
    }
    /// Releases every remaining run and removes the directory.
    pub(crate) fn finish(mut self) -> Result<(), io::Error> {
        let released = self.backing.release();
        fs::remove_dir(&self.directory)?;
        released.map_err(|error| io::Error::other(content(error).to_string()))
    }
}

fn malformed() -> Failure {
    Failure::Io(io::Error::new(
        io::ErrorKind::InvalidData,
        "malformed acquisition record",
    ))
}

/// Big-endian field reader over one record.
struct Fields<'a>(&'a [u8]);
impl<'a> Fields<'a> {
    fn bytes(&mut self, len: usize) -> Result<&'a [u8], Failure> {
        if self.0.len() < len {
            return Err(malformed());
        }
        let (head, rest) = self.0.split_at(len);
        self.0 = rest;
        Ok(head)
    }
    fn u64(&mut self) -> Result<u64, Failure> {
        let mut value = [0; 8];
        value.copy_from_slice(self.bytes(8)?);
        Ok(u64::from_be_bytes(value))
    }
    fn i64(&mut self) -> Result<i64, Failure> {
        Ok(self.u64()? as i64)
    }
    fn u32(&mut self) -> Result<u32, Failure> {
        let mut value = [0; 4];
        value.copy_from_slice(self.bytes(4)?);
        Ok(u32::from_be_bytes(value))
    }
    fn u8(&mut self) -> Result<u8, Failure> {
        Ok(self.bytes(1)?[0])
    }
    fn kind(&mut self) -> Result<InodeKind, Failure> {
        InodeKind::from_code(self.u8()?).map_err(content)
    }
    /// A one-byte presence flag followed by the remaining bytes.
    fn optional_rest(mut self) -> Result<Option<Vec<u8>>, Failure> {
        Ok((self.u8()? != 0).then(|| self.0.to_vec()))
    }
}

/// Ordering key of a record that begins with one position.
pub(crate) fn by_position(record: &[u8]) -> &[u8] {
    record.get(..8).unwrap_or(record)
}
/// Ordering key of a native record: device, inode, then position.
pub(crate) fn by_identity(record: &[u8]) -> &[u8] {
    record.get(..24).unwrap_or(record)
}
/// Ordering key of a child record: its name bytes.
pub(crate) fn by_name(record: &[u8]) -> &[u8] {
    let len = match record {
        [high, low, ..] => usize::from(u16::from_be_bytes([*high, *low])),
        _ => 0,
    };
    record.get(2..2 + len).unwrap_or(&[])
}

/// Native identity and change evidence observed for one source inode.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct Stamp {
    pub dev: u64,
    pub ino: u64,
    pub len: u64,
    pub mode: u32,
    pub mtime: i64,
    pub mtime_nsec: i64,
    pub ctime: i64,
    pub ctime_nsec: i64,
}
impl Stamp {
    pub(crate) fn of(metadata: &Metadata) -> Self {
        Self {
            dev: metadata.dev(),
            ino: metadata.ino(),
            len: metadata.len(),
            mode: metadata.mode(),
            mtime: metadata.mtime(),
            mtime_nsec: metadata.mtime_nsec(),
            ctime: metadata.ctime(),
            ctime_nsec: metadata.ctime_nsec(),
        }
    }
    /// True while `metadata` is still the same unchanged regular file.
    pub(crate) fn matches_file(&self, metadata: &Metadata) -> bool {
        metadata.is_file() && !metadata.file_type().is_symlink() && *self == Self::of(metadata)
    }
    fn put(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.dev.to_be_bytes());
        out.extend_from_slice(&self.ino.to_be_bytes());
        out.extend_from_slice(&self.len.to_be_bytes());
        out.extend_from_slice(&self.mode.to_be_bytes());
        for value in [self.mtime, self.mtime_nsec, self.ctime, self.ctime_nsec] {
            out.extend_from_slice(&value.to_be_bytes());
        }
    }
    fn take(fields: &mut Fields<'_>) -> Result<Self, Failure> {
        Ok(Self {
            dev: fields.u64()?,
            ino: fields.u64()?,
            len: fields.u64()?,
            mode: fields.u32()?,
            mtime: fields.i64()?,
            mtime_nsec: fields.i64()?,
            ctime: fields.i64()?,
            ctime_nsec: fields.i64()?,
        })
    }
}

/// One observed directory child before it receives its acquisition position.
pub(crate) struct Child {
    pub name: Vec<u8>,
    pub kind: InodeKind,
    pub target: Option<Vec<u8>>,
    pub stamp: Stamp,
}
impl Child {
    /// Portable mode: the symlink grammar fixes 0777; other kinds stay exact.
    fn portable_mode(&self) -> Result<u32, Failure> {
        let (mode, mask) = match self.kind {
            InodeKind::Symlink => (0o777, 0o777),
            InodeKind::Directory => (self.stamp.mode & 0o7777, 0o1777),
            InodeKind::RegularFile => (self.stamp.mode & 0o7777, 0o777),
        };
        if mode & !mask != 0 || !(0..1_000_000_000).contains(&self.stamp.mtime_nsec) {
            return Err(Failure::InvalidInput);
        }
        Ok(mode)
    }
    /// Record ordered by [`by_name`] while a wide directory is sorted.
    pub(crate) fn record(&self) -> Result<Vec<u8>, Failure> {
        let mut out = name_prefix(&self.name)?;
        out.push(self.kind.code());
        self.stamp.put(&mut out);
        put_optional(&mut out, self.target.as_deref());
        Ok(out)
    }
    pub(crate) fn decode(record: &[u8]) -> Result<Self, Failure> {
        let mut fields = Fields(record);
        let name = take_name(&mut fields)?;
        Ok(Self {
            name,
            kind: fields.kind()?,
            stamp: Stamp::take(&mut fields)?,
            target: fields.optional_rest()?,
        })
    }
    /// Entry record of this child at one position; refuses a nonportable mode.
    pub(crate) fn entry(&self, id: u64, parent: u64) -> Result<Vec<u8>, Failure> {
        let mut out = Vec::with_capacity(48 + self.name.len());
        out.extend_from_slice(&id.to_be_bytes());
        out.extend_from_slice(&parent.to_be_bytes());
        out.push(self.kind.code());
        out.extend_from_slice(&self.portable_mode()?.to_be_bytes());
        out.extend_from_slice(&self.stamp.mtime.to_be_bytes());
        out.extend_from_slice(&(self.stamp.mtime_nsec as u32).to_be_bytes());
        out.extend_from_slice(&name_prefix(&self.name)?);
        put_optional(&mut out, self.target.as_deref());
        Ok(out)
    }
}

fn name_prefix(name: &[u8]) -> Result<Vec<u8>, Failure> {
    let len = u16::try_from(name.len()).map_err(|_| Failure::InvalidInput)?;
    let mut out = Vec::with_capacity(80 + name.len());
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(name);
    Ok(out)
}
fn take_name(fields: &mut Fields<'_>) -> Result<Vec<u8>, Failure> {
    let len = fields.bytes(2)?;
    let len = usize::from(u16::from_be_bytes([len[0], len[1]]));
    Ok(fields.bytes(len)?.to_vec())
}
fn put_optional(out: &mut Vec<u8>, bytes: Option<&[u8]>) {
    out.push(u8::from(bytes.is_some()));
    out.extend_from_slice(bytes.unwrap_or(&[]));
}

/// One entry in acquisition position order.
pub(crate) struct Entry {
    pub id: u64,
    pub parent: u64,
    pub kind: InodeKind,
    pub mode: u32,
    pub mtime: i64,
    pub nanos: u32,
    pub name: Vec<u8>,
    pub target: Option<Vec<u8>>,
}
impl Entry {
    pub(crate) fn decode(record: &[u8]) -> Result<Self, Failure> {
        let mut fields = Fields(record);
        Ok(Self {
            id: fields.u64()?,
            parent: fields.u64()?,
            kind: fields.kind()?,
            mode: fields.u32()?,
            mtime: fields.i64()?,
            nanos: fields.u32()?,
            name: take_name(&mut fields)?,
            target: fields.optional_rest()?,
        })
    }
    /// Position and kind alone, without the entry's owned bytes.
    pub(crate) fn kind_of(record: &[u8]) -> Result<(u64, InodeKind), Failure> {
        let mut fields = Fields(record);
        let id = fields.u64()?;
        fields.u64()?;
        Ok((id, fields.kind()?))
    }
}

/// One native regular-file path with the evidence it must keep matching.
pub(crate) struct Job {
    pub id: u64,
    /// Position whose constructed file this path shares; its own when first.
    pub canonical: u64,
    pub path: PathBuf,
    pub stamp: Stamp,
}
impl Job {
    fn body(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.id.to_be_bytes());
        out.extend_from_slice(&self.canonical.to_be_bytes());
        self.stamp.put(out);
        out.extend_from_slice(self.path.as_os_str().as_bytes());
    }
    /// Record ordered by [`by_identity`].
    pub(crate) fn native_record(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(128 + self.path.as_os_str().len());
        out.extend_from_slice(&self.stamp.dev.to_be_bytes());
        out.extend_from_slice(&self.stamp.ino.to_be_bytes());
        out.extend_from_slice(&self.id.to_be_bytes());
        self.body(&mut out);
        out
    }
    /// Record ordered by [`by_position`].
    pub(crate) fn alias_record(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(128 + self.path.as_os_str().len());
        out.extend_from_slice(&self.id.to_be_bytes());
        self.body(&mut out);
        out
    }
    pub(crate) fn native(record: &[u8]) -> Result<Self, Failure> {
        Self::decode(record.get(24..).ok_or_else(malformed)?)
    }
    pub(crate) fn alias(record: &[u8]) -> Result<Self, Failure> {
        Self::decode(record.get(8..).ok_or_else(malformed)?)
    }
    fn decode(body: &[u8]) -> Result<Self, Failure> {
        let mut fields = Fields(body);
        Ok(Self {
            id: fields.u64()?,
            canonical: fields.u64()?,
            stamp: Stamp::take(&mut fields)?,
            path: PathBuf::from(OsString::from_vec(fields.0.to_vec())),
        })
    }
}

/// One root an entry position owns: metadata, target, directory or file.
pub(crate) struct Rooted {
    pub id: u64,
    pub root: ObjectId,
}
impl Rooted {
    /// Record ordered by [`by_position`].
    pub(crate) fn record(id: u64, root: ObjectId) -> [u8; 40] {
        let mut out = [0; 40];
        out[..8].copy_from_slice(&id.to_be_bytes());
        out[8..].copy_from_slice(root.as_bytes());
        out
    }
    pub(crate) fn decode(record: &[u8]) -> Result<Self, Failure> {
        let mut fields = Fields(record);
        Ok(Self {
            id: fields.u64()?,
            root: ObjectId::from_bytes(fields.0).map_err(content)?,
        })
    }
}

/// Record of two positions or counts, ordered by [`by_position`] on the first.
pub(crate) fn pair_record(first: u64, second: u64) -> [u8; 16] {
    let mut out = [0; 16];
    out[..8].copy_from_slice(&first.to_be_bytes());
    out[8..].copy_from_slice(&second.to_be_bytes());
    out
}
pub(crate) fn decode_pair(record: &[u8]) -> Result<(u64, u64), Failure> {
    let mut fields = Fields(record);
    Ok((fields.u64()?, fields.u64()?))
}

/// Frontier record: one directory position and the path it is read through.
pub(crate) fn frontier_record(id: u64, path: &Path) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + path.as_os_str().len());
    out.extend_from_slice(&id.to_be_bytes());
    out.extend_from_slice(path.as_os_str().as_bytes());
    out
}
pub(crate) fn decode_frontier(record: &[u8]) -> Result<(u64, PathBuf), Failure> {
    let mut fields = Fields(record);
    let id = fields.u64()?;
    Ok((id, PathBuf::from(OsString::from_vec(fields.0.to_vec()))))
}
