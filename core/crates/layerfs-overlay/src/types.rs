//! Typed engine keys and bounded job values; SQL is the mutable source of truth.

/// Selected cell window. S5 will qualify mutation/visibility and layout costs.
pub const CELL_BYTES: usize = 4096;
/// Per-byte validity for a cell; a written zero remains distinguishable.
pub const MASK_BYTES: usize = CELL_BYTES / 8;
/// Maximum rows in one owner service window, not a total namespace limit.
pub const PAGE_ROWS: usize = 64;
/// Maximum bytes in one scratch record, not a total operation limit.
pub const SCRATCH_BYTES: usize = 65_536;
/// Largest byte window of one write job; larger requests arrive as several.
pub const WRITE_WINDOW: usize = 128 * 1024;
/// Largest byte window of one local read plan.
pub const READ_WINDOW: usize = 128 * 1024;
/// Maximum changed inodes in one compound namespace job, not a namespace limit.
pub const COMPOUND_INODES: usize = 4;
/// Maximum changed name bindings in one compound namespace job.
pub const COMPOUND_NAMES: usize = 2;

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
    /// Bytes below this that are not written in the row's generation fall
    /// through to the lower view. The engine maintains it: on a serial's first
    /// local row it is the caller-supplied lower length (the inherited base
    /// file's length, zero for a new inode), and every shrink lowers it. A
    /// later value supplied by a caller is ignored.
    pub inherited_cutoff: u64,
    /// Generation that created this serial locally; zero inherits a base inode.
    /// A value above the installed floor means the bound base has no such inode.
    pub born: u64,
    /// Exact visible child bindings of a directory; zero for other kinds.
    pub entries: u64,
}
/// One final name binding; None is a whiteout, names remain binary in SQL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dentry {
    pub parent: u64,
    pub name: Vec<u8>,
    pub serial: Option<u64>,
}
/// Final state of one name in a compound job.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Binding {
    Bound(u64),
    /// `inherited` states whether the owned immutable base binds this name; a
    /// lower local row, when present, decides instead of this fact.
    Removed {
        inherited: bool,
    },
}
/// One changed name key of a compound job.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NameChange {
    pub parent: u64,
    pub name: Vec<u8>,
    pub binding: Binding,
}
/// Semantically checked final values of one atomic namespace operation. The
/// windows bound one job; they are not Workspace totals.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Changes {
    pub inodes: Vec<Inode>,
    pub names: Vec<NameChange>,
    /// One payload cell of a changed inode, published in the same transaction.
    pub cell: Option<(u64, Cell)>,
    /// One byte window written into a changed regular file, same transaction.
    pub write: Option<PayloadWrite>,
}
/// Bytes of one bounded write job. Shared, so job rounds never copy them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayloadWrite {
    pub serial: u64,
    pub offset: u64,
    pub data: std::sync::Arc<[u8]>,
}
/// Local layers of one read window, composed in one owner job. `data` holds
/// every byte decided locally (written bytes and zeros) from `offset`, clamped
/// to the file size. A set bit in `inherited` marks a byte the immutable base
/// file supplies instead; `span` is the one base range covering all of them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalRead {
    pub kind: InodeKind,
    pub size: u64,
    pub offset: u64,
    pub data: Vec<u8>,
    pub inherited: Vec<u8>,
    pub span: Option<(u64, u64)>,
}
/// Local rows of one name: the active generation and the latest lower one.
/// The outer None is "no row"; the inner None is a whiteout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NameLayers {
    pub active: Option<Option<u64>>,
    pub lower: Option<Option<u64>>,
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
    /// Maintained exact pending base-source windows, not a namespace scan.
    pub base_readers: u64,
}
/// One exact transient immutable-base source window. Release requires actual
/// request completion/fencing; open-file and command lifetime are independent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BaseSource {
    pub(crate) route: Route,
    pub(crate) owner: u64,
    pub(crate) root: [u8; 32],
    pub(crate) installed: i64,
}
/// Two bounded ordered local name inputs from one owner job. A capture can
/// change membership between jobs, never within this returned window.
#[derive(Clone, Debug)]
pub struct NameWindow {
    pub source: BaseSource,
    pub parent: u64,
    pub parent_inode: Option<Inode>,
    pub active: Vec<Dentry>,
    pub captured: Vec<Dentry>,
}
impl BaseSource {
    pub const fn route(self) -> Route {
        self.route
    }
    pub const fn owner(self) -> u64 {
        self.owner
    }
    pub const fn root(self) -> [u8; 32] {
        self.root
    }
    /// True when this source's base cannot hold an inode created at `born`:
    /// install is fenced while the source is owned, so the floor is stable.
    pub const fn created_above(self, born: u64) -> bool {
        born > self.installed as u64
    }
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
