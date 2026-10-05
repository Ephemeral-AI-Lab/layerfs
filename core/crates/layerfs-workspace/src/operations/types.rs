//! Ordinary namespace operations, their exact refusals and published results.
use crate::ViewStat;
use layerfs_content::filesystem::{PathName, SymlinkTarget};
use layerfs_overlay::{OpenFile, Publication};
use std::{fmt, ops::Deref, sync::Arc};

/// Bytes of one write window. Shared, so an operation's owner rounds and its
/// publication never copy them, and printed by length only.
#[derive(Clone, Eq, PartialEq)]
pub struct WriteData(pub(crate) Arc<[u8]>);
impl From<Vec<u8>> for WriteData {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes.into())
    }
}
impl From<&[u8]> for WriteData {
    fn from(bytes: &[u8]) -> Self {
        Self(bytes.into())
    }
}
impl Deref for WriteData {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.0
    }
}
impl fmt::Debug for WriteData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "WriteData({} bytes)", self.0.len())
    }
}
/// Where a write lands. `End` is resolved against the size current in the
/// publishing owner job, so concurrent appends never overlap.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Position {
    At(u64),
    End,
}

/// Caller-supplied wall time. Workspace owns no clock; portable metadata keeps
/// one mtime and reports it as ctime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Time {
    pub seconds: i64,
    pub nanoseconds: u32,
}
/// One ordinary operation over stable inode serials and checked names. Mode
/// values are permission bits only; access decisions belong to the mount's
/// default-permissions identity, not to this layer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operation {
    Create {
        parent: u64,
        name: PathName,
        mode: u32,
    },
    Mkdir {
        parent: u64,
        name: PathName,
        mode: u32,
    },
    Symlink {
        parent: u64,
        name: PathName,
        target: SymlinkTarget,
    },
    Link {
        serial: u64,
        parent: u64,
        name: PathName,
    },
    Unlink {
        parent: u64,
        name: PathName,
    },
    Rmdir {
        parent: u64,
        name: PathName,
    },
    /// `replace` false is the no-replace form. `destination_path` names the
    /// destination parent from the root; it is the ancestry evidence required
    /// when a directory moves to another parent, and is verified, not trusted.
    Rename {
        parent: u64,
        name: PathName,
        new_parent: u64,
        new_name: PathName,
        replace: bool,
        destination_path: Option<Vec<PathName>>,
    },
    /// chmod, utimens and/or truncate of one inode; absent fields are
    /// unchanged. A changed size sets the mtime unless one is given.
    SetAttributes {
        serial: u64,
        mode: Option<u32>,
        mtime: Option<Time>,
        size: Option<u64>,
    },
    /// One byte window of at most `WRITE_WINDOW`; larger writes arrive as
    /// several operations. There is no total size, edit or flow limit.
    /// Descriptor mutation revalidates exact open custody in every owner round.
    WriteOpen {
        file: OpenFile,
        position: Position,
        data: WriteData,
    },
    SetOpenAttributes {
        file: OpenFile,
        mode: Option<u32>,
        mtime: Option<Time>,
        size: Option<u64>,
    },
    Write {
        serial: u64,
        position: Position,
        data: WriteData,
    },
}
impl Operation {
    pub(crate) const fn file(&self) -> Option<OpenFile> {
        match self {
            Self::WriteOpen { file, .. } | Self::SetOpenAttributes { file, .. } => Some(*file),
            _ => None,
        }
    }
    pub(crate) const fn creates(&self) -> bool {
        matches!(
            self,
            Self::Create { .. } | Self::Mkdir { .. } | Self::Symlink { .. }
        )
    }
    pub(crate) fn destination_path(&self) -> Option<&[PathName]> {
        match self {
            Self::Rename {
                destination_path, ..
            } => destination_path.as_deref(),
            _ => None,
        }
    }
}
/// Definite pre-effect refusals: nothing was published and no ticket exists.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    /// The destination name is bound (EEXIST).
    Exists,
    /// A named inode, parent or target is absent or already removed (ENOENT).
    Missing,
    /// A parent or replaced/removed inode is not a directory (ENOTDIR).
    NotDirectory,
    /// A directory where a non-directory is required (EISDIR).
    IsDirectory,
    /// The directory still has visible entries (ENOTEMPTY).
    NotEmpty,
    /// Not representable or not allowed: hard link to a directory or symlink,
    /// set-id/sticky bits outside the portable grammar (EPERM).
    NotPermitted,
    /// No such attribute change for this kind, such as symlink mode (EOPNOTSUPP).
    Unsupported,
    /// Malformed value, or a directory moved beneath itself (EINVAL).
    Invalid,
    /// The canonical reference count cannot grow further (EMLINK).
    TooManyLinks,
    /// The file would exceed the largest representable offset (EFBIG).
    TooLarge,
    /// A directory changes parent and no destination path evidence was given.
    AncestryRequired,
    /// The supplied destination path does not resolve to the destination parent.
    AncestryMismatch,
}
/// Result of one attempted operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    /// Every effect is locally published. The publication requires exactly one
    /// later reply-send-attempt release; a lost reply does not undo it.
    Applied {
        publication: Publication,
        /// The created, linked or attribute-changed inode, when there is one.
        stat: Option<ViewStat>,
    },
    /// Successful with no state change, such as renaming a name onto itself.
    Unchanged { stat: Option<ViewStat> },
}
