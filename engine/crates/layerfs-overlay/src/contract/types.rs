//! Typed engine keys and bounded job values; SQL is the mutable source of truth.

/// Selected cell window. S5 will qualify mutation/visibility and layout costs.
pub const CELL_BYTES: usize = 4096;
/// Largest payload row. A row wider than one cell is dense, a whole number
/// of cells, and lies inside one aligned slot of this size, so every row
/// that can hold a byte is found by one indexed range of its slot. SQLite
/// reaches byte x of a stored value by walking x/4092 overflow pages: this
/// is also the page walk of one random access and of one maintenance row.
pub const RUN_BYTES: usize = 32 * 1024;
/// Per-byte validity for a cell; a written zero remains distinguishable.
pub const MASK_BYTES: usize = CELL_BYTES / 8;
/// Maximum rows in one owner service window, not a total namespace limit.
pub const PAGE_ROWS: usize = 64;
/// Maximum bytes in one operation_record record, not a total operation limit.
pub const OPERATION_RECORD_BYTES: usize = 65_536;
/// Largest byte window of one write job; larger requests arrive as several.
pub const WRITE_WINDOW: usize = 128 * 1024;
/// Largest byte window of one local read plan.
pub const READ_WINDOW: usize = 128 * 1024;
/// Most claimed ranges of one write job: one per matched run of a window.
pub const WRITE_CLAIMS: usize = 18;
/// Most reference ranges of one read window: a range is a run of whole
/// cells of the window, and no two ranges share a cell.
pub const READ_REFERENCES: usize = READ_WINDOW / CELL_BYTES + 1;
/// Maximum changed inodes in one compound namespace job, not a namespace limit.
pub const COMPOUND_INODES: usize = 4;
/// Maximum changed name bindings in one compound namespace job.
pub const COMPOUND_DIRECTORY_ENTRIES: usize = 2;

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
    /// Exact visible child bindings of a directory that are themselves
    /// directories; zero for other kinds. A directory's POSIX link count is
    /// two more than this. It is never the `nlink` reference count.
    pub subdirs: u64,
}
/// One final name binding; None is a whiteout, names remain binary in SQL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryEntry {
    /// Whether the immediately lower view binds this name.
    pub inherited: bool,
    pub parent: u64,
    pub name: Vec<u8>,
    pub serial: Option<u64>,
}
/// Final state of one name in a compound job.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Binding {
    Bound {
        serial: u64,
        inherited: bool,
    },
    /// `inherited` states whether the owned immutable base binds this name; a
    /// lower local row, when present, decides instead of this fact.
    Removed {
        inherited: bool,
    },
}
/// One changed name key of a compound job.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryEntryChange {
    pub parent: u64,
    pub name: Vec<u8>,
    pub binding: Binding,
}
/// Semantically checked final values of one atomic namespace operation. The
/// windows bound one job; they are not Workspace totals.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Changes {
    /// Semantically removed non-root inode; distinguishes zero-count root metadata.
    pub detached: Option<u64>,
    /// A directory moved to another parent by this same name publication.
    /// Updates retained kernel parent identity without enumerating open handles.
    pub moved_directory: Option<(u64, u64)>,
    /// Exact descriptor authority for mutations of independently owned orphans.
    pub open: Option<crate::OpenFile>,
    /// The serial this job creates. Its caller reserved it, so no row of it
    /// and no kernel reference on it exists: its first rows are inserted,
    /// never merged, and a row that already exists fails the whole job.
    pub created: Option<u64>,
    pub inodes: Vec<Inode>,
    pub directory_entries: Vec<DirectoryEntryChange>,
    /// One payload cell of a changed inode, published in the same transaction.
    pub cell: Option<(u64, Cell)>,
    /// One byte window written into a changed regular file, same transaction.
    pub write: Option<PayloadWrite>,
    /// Ranges of that window whose bytes equal committed content. They
    /// become reference pieces; every other byte of the window is stored.
    /// Boxed: most jobs carry none, and the job value stays small.
    pub claims: Option<Box<WriteClaims>>,
    /// A regular file a rename put in place of another: the moved serial and
    /// the replaced one. Advisory: it offers the moved file's Commit the
    /// replaced file as a delta base and states nothing about its bytes.
    pub predecessor: Option<(u64, u64)>,
}
/// Bytes of one bounded write job. Shared, so job rounds never copy them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayloadWrite {
    pub serial: u64,
    pub offset: u64,
    pub data: std::sync::Arc<[u8]>,
}
/// One range of a write whose bytes equal committed bytes of the claims'
/// source from `source_offset`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClaimedRange {
    pub offset: u64,
    pub length: u64,
    pub source_offset: u64,
}
/// What the caller established about the bytes of one write job: ranges that
/// equal committed content of one immutable root. The engine trusts the
/// claim and stores no byte of a claimed range; whole cells of it become one
/// reference piece and its ragged edges are stored from the written bytes.
/// Ranges are in file order and do not overlap. Only the first may start
/// before the written window and only the last may end after it, each by at
/// most `RUN_BYTES`: those bytes were stored by an earlier job, and that
/// part is honoured only while the file's content sequence is still `after`,
/// the revision of that job's publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteClaims {
    pub source: [u8; 32],
    pub after: Option<i64>,
    pub ranges: Vec<ClaimedRange>,
}
/// One range of a read window that committed content supplies: bytes
/// `[offset, offset + length)` of the file are the bytes of content root
/// `source` from `source_offset`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReferenceRange {
    pub offset: u64,
    pub length: u64,
    pub source: [u8; 32],
    pub source_offset: u64,
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
    /// Exact independent inherited root, when the read is orphan/captured
    /// input. Ordinary current-source reads use the source's bound root.
    pub base_root: Option<[u8; 32]>,
    /// Ranges of committed content that supply bytes of this window, at most
    /// `READ_REFERENCES`. Each is a run of cells clipped to the window, and no
    /// two overlap. Empty for a file without reference pieces.
    pub references: Vec<ReferenceRange>,
    /// Empty when `references` is; otherwise one bit per byte of `data`, in
    /// the form of `inherited`: set for a byte that the one range holding it
    /// supplies. A clear bit inside a range is a byte decided otherwise.
    pub referenced: Vec<u8>,
}
/// Local rows of one name: the active generation and the latest lower one.
/// The outer None is "no row"; the inner None is a whiteout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NameLayers {
    pub active_inherited: Option<bool>,
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
    pub(crate) engine: u64,
    pub(crate) revision: i64,
    pub generation: Generation,
}
impl Publication {
    /// Exact engine whose mutation awaits its reply-send attempt.
    pub const fn engine(self) -> u64 {
        self.engine
    }
    /// Locally published revision; reply delivery is not implied.
    pub const fn revision(self) -> i64 {
        self.revision
    }
}
/// Fixed captured domain. Identity is allocated by the engine, not a timestamp.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Capture {
    pub(crate) engine: u64,
    pub generation: Generation,
    pub revision: i64,
    pub base_root: [u8; 32],
}
impl Capture {
    /// Exact engine owning this fixed captured domain.
    pub const fn engine(self) -> u64 {
        self.engine
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
    pub dirty_directory_entries: u64,
    pub closed: bool,
    /// Maintained exact pending base-source windows, not a namespace scan.
    pub base_readers: u64,
    /// One lower namespace domain being composed after a definite fenced
    /// failure. A next capture waits; ordinary mutations remain live.
    pub consolidating: Option<Generation>,
}
/// One exact transient immutable-base source window. Release requires actual
/// request completion/fencing; open-file and command lifetime are independent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BaseSource {
    pub(crate) engine: u64,
    pub(crate) owner: u64,
    pub(crate) class: i64,
    pub(crate) root: [u8; 32],
    pub(crate) installed: i64,
}
/// Two bounded ordered local name inputs from one owner job. A capture can
/// change membership between jobs, never within this returned window.
#[derive(Clone, Debug)]
pub struct DirectoryEntryWindow {
    pub source: BaseSource,
    pub parent: u64,
    pub parent_inode: Option<Inode>,
    pub active: Vec<DirectoryEntry>,
    pub captured: Vec<DirectoryEntry>,
}
impl BaseSource {
    pub const fn engine(self) -> u64 {
        self.engine
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
/// Exact file-reference class, independent of generation or processing pins.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReferenceKind {
    Reader,
    File,
    Lookup,
}
