//! Ordinary namespace operations, their exact refusals and published results.
use crate::ViewStat;
use layerfs_content::filesystem::{PathName, SymlinkTarget};
use layerfs_overlay::Publication;

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
    /// chmod and/or utimens of one inode; absent fields are unchanged.
    SetAttributes {
        serial: u64,
        mode: Option<u32>,
        mtime: Option<Time>,
    },
}
impl Operation {
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
