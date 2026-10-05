//! Typed engine keys and bounded job values; SQL is the mutable source of truth.

/// Selected cell window. S5 will qualify mutation/visibility and layout costs.
pub const CELL_BYTES: usize = 4096;
/// Per-byte validity for a cell; a written zero remains distinguishable.
pub const MASK_BYTES: usize = CELL_BYTES / 8;
/// Maximum rows in one owner service window, not a total namespace limit.
pub const PAGE_ROWS: usize = 64;
/// Maximum bytes in one scratch record, not a total operation limit.
pub const SCRATCH_BYTES: usize = 65_536;

/// Incarnation-qualified routing capability issued by this daemon engine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Route {
    pub(crate) engine: u64,
    pub(crate) ns: i64,
    pub(crate) incarnation: [u8; 32],
}
impl Route {
    /// Namespace identifier for attributed diagnostics; not standalone authority.
    pub const fn namespace(self) -> i64 {
        self.ns
    }
}
/// One checked positive generation identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Generation(pub(crate) i64);
impl Generation {
    /// Numeric identity for diagnostics; cannot be allocated by a caller.
    pub const fn number(self) -> i64 {
        self.0
    }
}
/// Supported portable inode kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i64)]
pub enum InodeKind {
    File = 1,
    Directory = 2,
    Symlink = 3,
}
/// Complete changed inode value supplied after Workspace semantic validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Inode {
    pub serial: u64,
    pub kind: InodeKind,
    pub mode: u16,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: u32,
    pub nlink: u64,
    pub size: u64,
    pub inherited_cutoff: u64,
}
/// One final name binding; None is a whiteout, names remain binary in SQL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dentry {
    pub parent: u64,
    pub name: Vec<u8>,
    pub serial: Option<u64>,
}
/// A fixed bounded physical cell. Mask bits are little-bit-order per byte.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cell {
    pub offset: u64,
    pub data: Box<[u8; CELL_BYTES]>,
    pub validity: Box<[u8; MASK_BYTES]>,
}
/// Local publication requiring exactly one later reply-send-attempt release.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Publication {
    pub(crate) route: Route,
    pub(crate) revision: i64,
    pub generation: Generation,
}
impl Publication {
    /// Incarnation whose published mutation awaits its reply-send attempt.
    pub const fn route(self) -> Route {
        self.route
    }
    /// Locally published revision; reply delivery is not implied.
    pub const fn revision(self) -> i64 {
        self.revision
    }
}
/// Fixed captured domain. Identity is allocated by the engine, not a timestamp.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Capture {
    pub(crate) route: Route,
    pub generation: Generation,
    pub revision: i64,
    pub base_root: [u8; 32],
}
impl Capture {
    /// Namespace/incarnation owning this fixed captured domain.
    pub const fn route(self) -> Route {
        self.route
    }
}
/// Bounded read-only installed state; no namespace counting or payload acquisition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkspaceState {
    pub active: Generation,
    pub captured: Option<Generation>,
    /// Frozen local publication revision of the retained capture, independent
    /// of later active publication. Paired with captured in the schema.
    pub captured_revision: Option<i64>,
    /// Retired generations at/below this floor never enter the current view.
    pub installed: i64,
    pub revision: i64,
    pub base_root: [u8; 32],
    pub dirty_inodes: u64,
    pub dirty_names: u64,
    pub closed: bool,
}
/// Independently keyed custody, not a resident owner map.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i64)]
pub enum LeaseKind {
    Reader = 1,
    Open = 2,
    Lookup = 3,
    Operation = 4,
}
/// One exact owner/resource reference, not a retry token.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Lease {
    pub kind: LeaseKind,
    pub owner: u64,
    pub resource: u64,
}
/// One operation-keyed construction record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScratchRecord {
    pub kind: u32,
    pub key: u64,
    pub value: Vec<u8>,
}
