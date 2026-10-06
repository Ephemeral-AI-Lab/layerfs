//! Typed acquisition facts. No engine representation appears here.
use layerfs_content::{inode_leaf::InodeKind, ObjectId};

/// Bytes of native change evidence: length, mode, mtime and ctime.
pub const EVIDENCE_BYTES: usize = 44;

/// One live operation: its identity and the provider session that began it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Owner {
    /// Operation identity; never issued twice by one Store.
    pub operation: u64,
    /// Identity of the provider session that began the operation.
    pub epoch: u64,
}

/// Facts fixed when an operation begins.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Begin {
    /// Native device of the source root.
    pub source_device: u64,
    /// Native inode of the source root.
    pub source_inode: u64,
    /// LayerStack authority identity the acquisition is for.
    pub stack: [u8; 16],
    /// Canonical inode allocation scope.
    pub scope: ObjectId,
}

/// Recorded progress of one operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    /// Source entries are being observed and placed.
    Scanning,
    /// Native regular files are being constructed.
    Files,
    /// Directories and the inode table are being built.
    Tree,
    /// Working rows are being removed.
    Discarding,
    /// The operation definitely failed; its working rows may remain.
    Failed,
    /// The operation's outcome is unknown to its owner.
    Uncertain,
}
impl Phase {
    /// Persisted code.
    pub const fn code(self) -> u8 {
        match self {
            Self::Scanning => 1,
            Self::Files => 2,
            Self::Tree => 3,
            Self::Discarding => 4,
            Self::Failed => 5,
            Self::Uncertain => 6,
        }
    }
    /// Rebuilds a phase from its persisted code.
    pub const fn from_code(code: u8) -> Option<Self> {
        Some(match code {
            1 => Self::Scanning,
            2 => Self::Files,
            3 => Self::Tree,
            4 => Self::Discarding,
            5 => Self::Failed,
            6 => Self::Uncertain,
            _ => return None,
        })
    }
}

/// Exact native identity and change evidence of one regular file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeIdentity {
    /// Native device, exact and unsigned.
    pub device: u64,
    /// Native inode, exact and unsigned.
    pub inode: u64,
    /// Opaque evidence that must be byte-equal on every path of the identity.
    pub evidence: [u8; EVIDENCE_BYTES],
}

/// Stable key of one entry: its parent's position and its name bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntryKey {
    /// Parent directory position; `None` for the source root.
    pub parent: Option<u64>,
    /// Name bytes within the parent; empty for the source root.
    pub name: Vec<u8>,
}

/// One observed entry to append.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewEntry {
    /// Stable key.
    pub key: EntryKey,
    /// Final position, or `None` for a child of a directory still being ordered.
    pub position: Option<u64>,
    /// File kind.
    pub kind: InodeKind,
    /// Constructed attribute root.
    pub metadata_root: ObjectId,
    /// Constructed symlink target root; `None` for other kinds.
    pub target_root: Option<ObjectId>,
    /// Native path: required for a directory and a regular file.
    pub native_path: Option<Vec<u8>>,
    /// Native identity: required for a regular file.
    pub native: Option<NativeIdentity>,
}

/// One unpositioned child read back for placement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Unplaced {
    /// Name bytes.
    pub name: Vec<u8>,
    /// File kind.
    pub kind: InodeKind,
    /// Native identity of a regular file.
    pub native: Option<NativeIdentity>,
}

/// One child's assigned position.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Placed {
    /// Name bytes of the unpositioned child.
    pub name: Vec<u8>,
    /// Final position.
    pub position: u64,
    /// Native identity and path of a regular file; `None` for other kinds.
    pub native: Option<(NativeIdentity, Vec<u8>)>,
}

/// One directory to read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Directory {
    /// Directory position.
    pub position: u64,
    /// Native path the directory is read through.
    pub native_path: Vec<u8>,
}

/// The first path of one native regular-file identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Job {
    /// Canonical position, shared by every later path.
    pub position: u64,
    /// Identity and the evidence every path must keep matching.
    pub native: NativeIdentity,
    /// Native path of the first path.
    pub native_path: Vec<u8>,
}

/// One native identity's constructed root and later-path count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileRoot {
    /// Canonical position.
    pub position: u64,
    /// Later in-root paths sharing the identity.
    pub aliases: u64,
    /// Constructed file root, once recorded.
    pub root: Option<ObjectId>,
}

/// One entry in acquisition order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    /// Stable key.
    pub key: EntryKey,
    /// Final position.
    pub position: u64,
    /// File kind.
    pub kind: InodeKind,
    /// Canonical position of a regular file; its own when it is the first path.
    pub canonical: Option<u64>,
    /// Constructed attribute root.
    pub metadata_root: ObjectId,
    /// Symlink target root or recorded directory root.
    pub content_root: Option<ObjectId>,
}

/// One operation another provider session began.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Abandoned {
    /// The operation and the session that began it.
    pub owner: Owner,
    /// Last recorded phase.
    pub phase: Phase,
    /// Working rows it still holds.
    pub held_rows: u64,
    /// Payload bytes those rows hold.
    pub held_bytes: u64,
}
