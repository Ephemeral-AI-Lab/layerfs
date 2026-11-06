//! The framing `project.init` copies a caller's folder to a machine's
//! worker in (#536 H11a). One `project.stage` request's `part` is a run of
//! records: a tag byte, the entry's path relative to the folder, then the
//! tag's fields. Integers are big-endian; a path, target or piece is a
//! `u32` length and that many bytes (names are bytes, never text).
//!
//! - `D path`: a directory, made empty; its `M` follows its contents.
//! - `W path offset:u64 bytes`: a piece of a regular file; offset 0 makes
//!   the file, a later piece continues it at its length.
//! - `L path target`: a symbolic link.
//! - `M path mode:u32 seconds:i64 nanoseconds:u32`: the entry's permission
//!   bits (not a link's) and modification time, after its content; the
//!   empty path is the folder itself.
//!
//! This is what Init records of an entry (its kind, permission bits,
//! modification time, a link's target and a file's bytes); owners,
//! access times, extended attributes and hard links are not carried.
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::outcome::refused;

/// One entry's record.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Record<'a> {
    Directory {
        path: &'a [u8],
    },
    Write {
        path: &'a [u8],
        offset: u64,
        bytes: &'a [u8],
    },
    Link {
        path: &'a [u8],
        target: &'a [u8],
    },
    Meta {
        path: &'a [u8],
        mode: u32,
        seconds: i64,
        nanoseconds: u32,
    },
}

const DIRECTORY: u8 = b'D';
const WRITE: u8 = b'W';
const LINK: u8 = b'L';
const META: u8 = b'M';

/// The bytes a record with `path` and no fields takes.
pub(crate) fn header(path: &[u8]) -> usize {
    1 + 4 + path.len()
}

fn put(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
}

impl Record<'_> {
    /// The bytes this record takes.
    pub(crate) fn size(&self) -> usize {
        match self {
            Self::Directory { path } => header(path),
            Self::Write { path, bytes, .. } => header(path) + 8 + 4 + bytes.len(),
            Self::Link { path, target } => header(path) + 4 + target.len(),
            Self::Meta { path, .. } => header(path) + 4 + 8 + 4,
        }
    }

    /// Appends this record to `out`.
    pub(crate) fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Directory { path } => {
                out.push(DIRECTORY);
                put(out, path);
            }
            Self::Write {
                path,
                offset,
                bytes,
            } => {
                out.push(WRITE);
                put(out, path);
                out.extend_from_slice(&offset.to_be_bytes());
                put(out, bytes);
            }
            Self::Link { path, target } => {
                out.push(LINK);
                put(out, path);
                put(out, target);
            }
            Self::Meta {
                path,
                mode,
                seconds,
                nanoseconds,
            } => {
                out.push(META);
                put(out, path);
                out.extend_from_slice(&mode.to_be_bytes());
                out.extend_from_slice(&seconds.to_be_bytes());
                out.extend_from_slice(&nanoseconds.to_be_bytes());
            }
        }
    }
}

/// Reads records off the front of a part.
pub(crate) struct Reader<'a> {
    rest: &'a [u8],
}

fn malformed() -> SdkRefusal {
    refused("InvalidInput", "a folder copy's part is malformed")
}

impl<'a> Reader<'a> {
    pub(crate) fn new(part: &'a [u8]) -> Self {
        Self { rest: part }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], SdkRefusal> {
        if self.rest.len() < len {
            return Err(malformed());
        }
        let (head, rest) = self.rest.split_at(len);
        self.rest = rest;
        Ok(head)
    }

    fn fixed<const N: usize>(&mut self) -> Result<[u8; N], SdkRefusal> {
        Ok(self.take(N)?.try_into().expect("N bytes"))
    }

    fn bytes(&mut self) -> Result<&'a [u8], SdkRefusal> {
        let len = u32::from_be_bytes(self.fixed()?) as usize;
        self.take(len)
    }

    /// The next record, or `None` at the part's end.
    pub(crate) fn next_record(&mut self) -> Result<Option<Record<'a>>, SdkRefusal> {
        let Some(&tag) = self.rest.first() else {
            return Ok(None);
        };
        self.rest = &self.rest[1..];
        let path = self.bytes()?;
        Ok(Some(match tag {
            DIRECTORY => Record::Directory { path },
            WRITE => Record::Write {
                path,
                offset: u64::from_be_bytes(self.fixed()?),
                bytes: self.bytes()?,
            },
            LINK => Record::Link {
                path,
                target: self.bytes()?,
            },
            META => Record::Meta {
                path,
                mode: u32::from_be_bytes(self.fixed()?),
                seconds: i64::from_be_bytes(self.fixed()?),
                nanoseconds: u32::from_be_bytes(self.fixed()?),
            },
            _ => return Err(malformed()),
        }))
    }
}
