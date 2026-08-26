use crate::capture::initialize_empty;
use crate::{CanonicalPath, OperationCounters, VfsError};
use layerfs_core::content::rope::{
    build, read_all_bounded, read_plan, read_range_with_plan, replace, state as rope_state,
    FileStateRoot, ObjectRead, ReadPlan, RopeCounters,
};
use layerfs_core::inode::{
    inode_table_lookup, inode_table_remove, inode_table_upsert, visit_inode_table_entries, InodeId,
    InodeKind, InodeRecordV1, InodeTableCounters, InodeTableRoot,
};
use layerfs_core::metadata::{
    metadata_lookup, metadata_tree_entries, visit_metadata_entries, MetadataEntryV1, MetadataKey,
    MetadataTreeBuilder, PortableMetadataV1,
};
use layerfs_core::namespace::{
    directory_insert, directory_lookup, directory_page_after, directory_remove, empty_directory,
    DirectoryStateRoot, NamespaceCounters, NamespaceRootV1, SymlinkStateV1,
};
use layerfs_core::namespace_codec::{
    decode_inode_record, decode_namespace_root, decode_symlink, encode_inode_record,
    encode_namespace_root, encode_symlink,
};
use layerfs_core::{CanonicalName, CoreError, ObjectId};
use layerfs_engine::integrity::IntegrityMode;
use layerfs_engine::publication::Publication;
use layerfs_engine::refs::RefState;
use layerfs_engine::{Engine, EngineCounters, EngineError};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::ops::Bound::{Excluded, Unbounded};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

pub const ROOT_NODE: MountedNodeId = MountedNodeId(1);
pub const MAX_REQUEST_BYTES: usize = 1024 * 1024;
pub const MAX_OPERATION_Q_BYTES: usize = 8 * 1024 * 1024 - 1;
pub const MAX_MOUNTED_NODES: usize = 65_536;
pub const MAX_HANDLES: usize = 8_192;
pub const MAX_DIRTY_NODES: usize = 4_096;
pub const MAX_DIRTY_RANGES: usize = 8_192;
pub const MAX_DIRECTORY_CURSORS: usize = 4_096;
pub const MAX_DIRECTORY_CHANGES: usize = 8_192;
pub const SPOOL_QUOTA_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_LIVE_SPOOL_BYTES: u64 = 320 * 1024 * 1024;
pub const MAX_LOGICAL_FILE_BYTES: u64 = 320 * 1024 * 1024;
pub const MAX_LOGICAL_WORKSPACE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_COLD_LOOKUP_PRIMARY_STATEMENTS: u64 = 16;
const DIRECTORY_PAGE_ENTRIES: usize = 128;
const DIRECTORY_PAGE_BYTES: usize = 256 * 1024;
const SPOOL_COMPACTION_SLACK_BYTES: u64 = 64 * 1024 * 1024;
const MAX_CHECKPOINT_METADATA_BYTES: usize = 1024 * 1024;
const SPOOL_MAGIC: &[u8; 8] = b"LFSMNT1\0";
const SPOOL_MARKER_BYTES: u64 = 88;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MountedNodeId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MountedHandleId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MountedFileType {
    RegularFile,
    Directory,
    Symlink,
}

impl From<InodeKind> for MountedFileType {
    fn from(value: InodeKind) -> Self {
        match value {
            InodeKind::RegularFile => Self::RegularFile,
            InodeKind::Directory => Self::Directory,
            InodeKind::Symlink => Self::Symlink,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MountedAttr {
    pub node: MountedNodeId,
    pub kind: MountedFileType,
    pub size: u64,
    pub mode: u32,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: u32,
    pub links: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MountedCapacity {
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub total_files: u64,
    pub free_files: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MountedDirEntry {
    pub node: MountedNodeId,
    pub name: Vec<u8>,
    pub kind: MountedFileType,
    pub next_offset: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MountedCounters {
    pub lookups: u64,
    pub getattr: u64,
    pub opens: u64,
    pub reads: u64,
    pub writes: u64,
    pub splices: u64,
    pub creates: u64,
    pub mkdirs: u64,
    pub unlinks: u64,
    pub renames: u64,
    pub links: u64,
    pub symlinks: u64,
    pub flushes: u64,
    pub releases: u64,
    pub fsyncs: u64,
    pub fsyncdirs: u64,
    pub checkpoints: u64,
    pub no_op_checkpoints: u64,
    pub created_then_deleted: u64,
    pub lookup_refs: u64,
    pub lookup_refs_high_water: u64,
    pub live_nodes: u64,
    pub live_nodes_high_water: u64,
    pub open_handles: u64,
    pub open_handles_high_water: u64,
    pub pending_nodes: u64,
    pub pending_nodes_high_water: u64,
    pub dirty_nodes: u64,
    pub dirty_nodes_high_water: u64,
    pub dirty_ranges: u64,
    pub dirty_ranges_high_water: u64,
    pub directory_cursors: u64,
    pub directory_changes: u64,
    pub directory_changes_high_water: u64,
    pub inode_mappings: u64,
    pub inode_mappings_high_water: u64,
    pub logical_workspace_bytes: u64,
    pub logical_workspace_high_water_bytes: u64,
    pub spool_appended_bytes: u64,
    pub spool_live_bytes: u64,
    pub spool_live_high_water_bytes: u64,
    pub spool_dead_bytes: u64,
    pub spool_physical_bytes: u64,
    pub spool_physical_high_water_bytes: u64,
    pub spool_resets: u64,
    pub spool_compactions: u64,
    pub largest_request_bytes: u64,
    pub operation_q_current_bytes: u64,
    pub operation_q_high_water_bytes: u64,
    pub materializations: u64,
    pub capture_scans: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MountedLifecycle {
    Live,
    Checkpointing,
    Conflict,
    Incomplete,
    Closing,
    Closed,
}

pub struct MountedSpliceReceipt {
    pub before: RefState,
    pub after: RefState,
    pub counters: OperationCounters,
    pub remount_required: bool,
}

#[derive(Debug)]
pub enum MountedError {
    NotFound,
    AlreadyExists,
    NotDirectory,
    IsDirectory,
    NotEmpty,
    InvalidName,
    InvalidRange,
    PermissionDenied,
    ReadOnly,
    NoSpace,
    TooManyOpenFiles,
    ResourceExhausted,
    Busy,
    StaleHandle,
    InvalidHandle,
    Conflict,
    CommittedCleanup,
    Unsupported,
    Corrupt,
    Indeterminate,
    Startup(&'static str, String),
    Io(std::io::Error),
}

impl std::fmt::Display for MountedError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for MountedError {}

impl From<std::io::Error> for MountedError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<CoreError> for MountedError {
    fn from(value: CoreError) -> Self {
        match value {
            CoreError::PathNotFound | CoreError::MissingObject => Self::NotFound,
            CoreError::NameCollision => Self::AlreadyExists,
            CoreError::InvalidRange { .. } => Self::InvalidRange,
            CoreError::InvalidUtf8 | CoreError::PathLimitExceeded => Self::InvalidName,
            CoreError::Io => Self::Corrupt,
            _ => Self::Corrupt,
        }
    }
}

impl From<EngineError> for MountedError {
    fn from(value: EngineError) -> Self {
        match value {
            EngineError::PublicationConflict => Self::Conflict,
            EngineError::AmbiguousDurability => Self::Indeterminate,
            EngineError::MissingObject(_) | EngineError::MissingRoot(_) => Self::NotFound,
            EngineError::Core(error) => error.into(),
            EngineError::Sqlite {
                kind: layerfs_engine::SqliteErrorKind::NoSpace,
                ..
            } => Self::NoSpace,
            EngineError::Sqlite {
                kind: layerfs_engine::SqliteErrorKind::ReadOnly,
                ..
            } => Self::ReadOnly,
            _ => Self::Corrupt,
        }
    }
}

#[derive(Default)]
struct BudgetState {
    current: usize,
    high: usize,
    paused: bool,
    shutdown: bool,
}

pub struct ByteBudget {
    limit: usize,
    state: Mutex<BudgetState>,
    available: Condvar,
}

impl ByteBudget {
    fn new(limit: usize) -> Self {
        Self {
            limit,
            state: Mutex::new(BudgetState::default()),
            available: Condvar::new(),
        }
    }

    pub fn reserve(self: &Arc<Self>, bytes: usize) -> Result<ByteReservation, MountedError> {
        if bytes > self.limit {
            return Err(MountedError::ResourceExhausted);
        }
        let mut state = self.state.lock().map_err(|_| MountedError::Indeterminate)?;
        while !state.shutdown && (state.paused || state.current + bytes > self.limit) {
            state = self
                .available
                .wait(state)
                .map_err(|_| MountedError::Indeterminate)?;
        }
        if state.shutdown {
            return Err(MountedError::Busy);
        }
        state.current += bytes;
        state.high = state.high.max(state.current);
        Ok(ByteReservation {
            budget: self.clone(),
            bytes,
        })
    }

    pub fn try_reserve(self: &Arc<Self>, bytes: usize) -> Result<ByteReservation, MountedError> {
        if bytes > self.limit {
            return Err(MountedError::ResourceExhausted);
        }
        let mut state = self.state.lock().map_err(|_| MountedError::Indeterminate)?;
        if state.shutdown {
            return Err(MountedError::Busy);
        }
        if state.current + bytes > self.limit {
            return Err(MountedError::ResourceExhausted);
        }
        state.current += bytes;
        state.high = state.high.max(state.current);
        Ok(ByteReservation {
            budget: self.clone(),
            bytes,
        })
    }

    fn observation(&self) -> Result<(usize, usize), MountedError> {
        let state = self.state.lock().map_err(|_| MountedError::Indeterminate)?;
        Ok((state.current, state.high))
    }

    pub fn pause_and_wait(&self) -> Result<(), MountedError> {
        let mut state = self.state.lock().map_err(|_| MountedError::Indeterminate)?;
        if state.shutdown {
            return Err(MountedError::Busy);
        }
        state.paused = true;
        while state.current != 0 {
            state = self
                .available
                .wait(state)
                .map_err(|_| MountedError::Indeterminate)?;
        }
        Ok(())
    }

    pub fn resume(&self) {
        if let Ok(mut state) = self.state.lock() {
            if !state.shutdown {
                state.paused = false;
                self.available.notify_all();
            }
        }
    }

    pub fn close_and_wait(&self) -> Result<(), MountedError> {
        let mut state = self.state.lock().map_err(|_| MountedError::Indeterminate)?;
        state.shutdown = true;
        self.available.notify_all();
        while state.current != 0 {
            state = self
                .available
                .wait(state)
                .map_err(|_| MountedError::Indeterminate)?;
        }
        Ok(())
    }

    fn shutdown(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.shutdown = true;
            self.available.notify_all();
        }
    }
}

pub struct ByteReservation {
    budget: Arc<ByteBudget>,
    bytes: usize,
}

impl Drop for ByteReservation {
    fn drop(&mut self) {
        if let Ok(mut state) = self.budget.state.lock() {
            state.current -= self.bytes;
            self.budget.available.notify_all();
        }
    }
}

#[derive(Clone)]
struct DirtyRange {
    end: u64,
    spool_offset: u64,
}

struct SpoolRangeLocation {
    node: MountedNodeId,
    start: u64,
    old_offset: u64,
    len: u64,
}

#[derive(Clone)]
enum NodeContent {
    File {
        base: Option<FileStateRoot>,
        base_visible_len: u64,
        logical_len: u64,
        ranges: BTreeMap<u64, DirtyRange>,
        plan: Option<Arc<ReadPlan>>,
    },
    Directory {
        base: Option<DirectoryStateRoot>,
        changes: BTreeMap<CanonicalName, Option<MountedNodeId>>,
    },
    Symlink {
        target: Vec<u8>,
    },
}

struct MountedNode {
    canonical: Option<InodeId>,
    record: Option<InodeRecordV1>,
    kind: MountedFileType,
    mode: u32,
    mtime_seconds: i64,
    mtime_nanoseconds: u32,
    namespace_refs: u64,
    parent: MountedNodeId,
    lookup_refs: u64,
    open_refs: u64,
    deleted: bool,
    dirty_content: bool,
    dirty_metadata: bool,
    dirty_links: bool,
    directory_mtime_before: Option<(i64, u32, bool)>,
    content: NodeContent,
}

impl MountedNode {
    fn pending(&self) -> bool {
        self.canonical.is_none() && !self.deleted
    }

    fn dirty(&self) -> bool {
        self.pending() || self.dirty_content || self.dirty_metadata || self.dirty_links
    }

    fn attr(&self, id: MountedNodeId) -> MountedAttr {
        let size = match &self.content {
            NodeContent::File { logical_len, .. } => *logical_len,
            NodeContent::Directory { .. } => 0,
            NodeContent::Symlink { target } => target.len() as u64,
        };
        MountedAttr {
            node: id,
            kind: self.kind,
            size,
            mode: self.mode,
            mtime_seconds: self.mtime_seconds,
            mtime_nanoseconds: self.mtime_nanoseconds,
            links: u32::try_from(self.namespace_refs).unwrap_or(u32::MAX),
        }
    }
}

#[derive(Clone)]
struct DirectoryCursor {
    node: MountedNodeId,
    scan_after: Option<CanonicalName>,
    base_after: Option<CanonicalName>,
    base: VecDeque<(CanonicalName, InodeId)>,
    base_done: bool,
    cookie: u64,
}

struct DirectoryMutation {
    parent: MountedNodeId,
    name: CanonicalName,
    normalized: Option<Option<MountedNodeId>>,
    change_delta: i8,
    timestamp: (i64, u32),
}

impl DirectoryCursor {
    fn new(node: MountedNodeId) -> Self {
        Self {
            node,
            scan_after: None,
            base_after: None,
            base: VecDeque::new(),
            base_done: false,
            cookie: 0,
        }
    }
}

enum Handle {
    File(MountedNodeId),
    Directory(Box<DirectoryHandle>),
}

struct DirectoryHandle {
    committed: DirectoryCursor,
    pending: Option<(MountedDirEntry, DirectoryCursor)>,
}

struct Spool {
    path: PathBuf,
    marker: [u8; SPOOL_MARKER_BYTES as usize],
    file: Option<File>,
    appended: u64,
    total_appended: u64,
    live: u64,
}

impl Spool {
    fn new(
        path: PathBuf,
        store_id: [u8; 32],
        owner_id: [u8; 32],
        session_id: [u8; 16],
    ) -> Result<Self, MountedError> {
        let mut marker = [0_u8; SPOOL_MARKER_BYTES as usize];
        marker[..8].copy_from_slice(SPOOL_MAGIC);
        marker[8..40].copy_from_slice(&store_id);
        marker[40..72].copy_from_slice(&owner_id);
        marker[72..88].copy_from_slice(&session_id);
        let compact = Self::compaction_path(&path);
        if compact.exists() {
            let mut prior = OpenOptions::new().read(true).open(&compact)?;
            let mut actual = [0_u8; SPOOL_MARKER_BYTES as usize];
            prior.read_exact(&mut actual)?;
            if actual[..72] != marker[..72] {
                return Err(MountedError::Corrupt);
            }
            drop(prior);
            std::fs::remove_file(compact)?;
        }
        if path.exists() {
            let mut prior = OpenOptions::new().read(true).open(&path)?;
            let mut actual = [0_u8; SPOOL_MARKER_BYTES as usize];
            prior.read_exact(&mut actual)?;
            if actual[..72] != marker[..72] {
                return Err(MountedError::Corrupt);
            }
            drop(prior);
            std::fs::remove_file(&path)?;
        }
        Ok(Self {
            path,
            marker,
            file: None,
            appended: 0,
            total_appended: 0,
            live: 0,
        })
    }

    fn next_offset(&self, bytes: usize) -> Result<u64, MountedError> {
        let bytes = u64::try_from(bytes).map_err(|_| MountedError::NoSpace)?;
        if self
            .appended
            .checked_add(bytes)
            .is_none_or(|value| value > SPOOL_QUOTA_BYTES)
        {
            return Err(MountedError::NoSpace);
        }
        Ok(SPOOL_MARKER_BYTES + self.appended)
    }

    fn append(&mut self, bytes: &[u8]) -> Result<u64, MountedError> {
        let offset = self.next_offset(bytes.len())?;
        if self.file.is_none() {
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut file = OpenOptions::new()
                .create_new(true)
                .read(true)
                .write(true)
                .open(&self.path)?;
            file.write_all(&self.marker)?;
            self.file = Some(file);
        }
        let file = self.file.as_mut().ok_or(MountedError::Indeterminate)?;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(bytes)?;
        let length = u64::try_from(bytes.len()).map_err(|_| MountedError::NoSpace)?;
        self.appended = self
            .appended
            .checked_add(length)
            .ok_or(MountedError::Indeterminate)?;
        self.total_appended = self
            .total_appended
            .checked_add(length)
            .ok_or(MountedError::Indeterminate)?;
        Ok(offset)
    }

    fn read_exact_at(&mut self, offset: u64, output: &mut [u8]) -> Result<(), MountedError> {
        let file = self.file.as_mut().ok_or(MountedError::Corrupt)?;
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(output)?;
        Ok(())
    }

    fn slice(&mut self, offset: u64, len: u64) -> Result<SpoolSlice<'_>, MountedError> {
        let file = self.file.as_mut().ok_or(MountedError::Corrupt)?;
        file.seek(SeekFrom::Start(offset))?;
        Ok(SpoolSlice {
            file,
            remaining: len,
        })
    }

    fn reset(&mut self) -> Result<bool, MountedError> {
        self.file.take();
        let existed = self.path.exists();
        if existed {
            std::fs::remove_file(&self.path)?;
        }
        self.appended = 0;
        self.live = 0;
        Ok(existed)
    }

    fn physical(&self) -> u64 {
        self.appended
    }

    fn compaction_path(path: &Path) -> PathBuf {
        let mut value = path.as_os_str().to_os_string();
        value.push(".compact");
        PathBuf::from(value)
    }
}

struct SpoolSlice<'a> {
    file: &'a mut File,
    remaining: u64,
}

impl Read for SpoolSlice<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if self.remaining == 0 {
            return Ok(0);
        }
        let allowed = output.len().min(self.remaining as usize);
        let read = self.file.read(&mut output[..allowed])?;
        self.remaining -= read as u64;
        Ok(read)
    }
}

pub struct MountedWorkspace {
    engine: Engine,
    accepted: RefState,
    namespace: NamespaceRootV1,
    lifecycle: MountedLifecycle,
    nodes: HashMap<MountedNodeId, MountedNode>,
    by_inode: HashMap<InodeId, MountedNodeId>,
    reclaimable_inode_mappings: BTreeSet<InodeId>,
    handles: HashMap<MountedHandleId, Handle>,
    next_node: u64,
    next_handle: u64,
    live_ranges: usize,
    dirty_nodes: HashSet<MountedNodeId>,
    pending_nodes: HashSet<MountedNodeId>,
    directory_cursors: usize,
    directory_changes: usize,
    lookup_refs: u64,
    logical_workspace_bytes: u64,
    spool: Spool,
    budget: Arc<ByteBudget>,
    counters: MountedCounters,
    #[cfg(test)]
    splice_post_visibility_uncertainty: bool,
}

impl MountedWorkspace {
    pub fn open(
        store: &Path,
        ref_name: &str,
        integrity: IntegrityMode,
        spool: PathBuf,
        owner_id: [u8; 32],
    ) -> Result<Self, MountedError> {
        let engine = Engine::open_with_mode(store, integrity)
            .map_err(|error| startup("engine open", error))?;
        let accepted = match engine
            .read_ref(ref_name)
            .map_err(|error| startup("ref read", error))?
        {
            Some(state) => state,
            None if ref_name == "main" => {
                initialize_empty(&engine).map_err(|error| startup("empty root", error))?
            }
            None => return Err(MountedError::NotFound),
        };
        let namespace = engine
            .with_authenticated_canonical(accepted.root, decode_namespace_root)
            .map_err(|error| startup("namespace root", error))?;
        let logical_workspace_bytes = accepted_logical_bytes(&engine, namespace.inode_table_root)
            .map_err(|error| startup("logical workspace", error))?;
        if logical_workspace_bytes > MAX_LOGICAL_WORKSPACE_BYTES {
            return Err(MountedError::ResourceExhausted);
        }
        let session_hash = ObjectId::for_bytes(
            &SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| MountedError::Indeterminate)?
                .as_nanos()
                .to_be_bytes(),
        );
        let mut session_id = [0_u8; 16];
        session_id.copy_from_slice(&session_hash.as_bytes()[..16]);
        let store_id = engine
            .store_id()
            .map_err(|error| startup("StoreId", error))?;
        let spool = Spool::new(spool, store_id, owner_id, session_id)
            .map_err(|error| startup("spool", error))?;
        let mut workspace = Self {
            engine,
            accepted,
            namespace,
            lifecycle: MountedLifecycle::Live,
            nodes: HashMap::new(),
            by_inode: HashMap::new(),
            reclaimable_inode_mappings: BTreeSet::new(),
            handles: HashMap::new(),
            next_node: ROOT_NODE.0 + 1,
            next_handle: 1,
            live_ranges: 0,
            dirty_nodes: HashSet::new(),
            pending_nodes: HashSet::new(),
            directory_cursors: 0,
            directory_changes: 0,
            lookup_refs: 0,
            logical_workspace_bytes,
            spool,
            budget: Arc::new(ByteBudget::new(MAX_OPERATION_Q_BYTES)),
            counters: MountedCounters::default(),
            #[cfg(test)]
            splice_post_visibility_uncertainty: false,
        };
        let root = workspace
            .load_canonical_node(namespace.root_directory_inode, ROOT_NODE)
            .map_err(|error| startup("root inode", error))?;
        if root != ROOT_NODE {
            return Err(MountedError::Corrupt);
        }
        let root = workspace
            .nodes
            .get_mut(&ROOT_NODE)
            .ok_or(MountedError::Corrupt)?;
        root.parent = ROOT_NODE;
        root.lookup_refs = 1;
        workspace.lookup_refs = 1;
        workspace.observe_resources()?;
        Ok(workspace)
    }

    pub fn accepted(&self) -> &RefState {
        &self.accepted
    }

    pub fn splice_path(
        &mut self,
        path: &CanonicalPath,
        start: u64,
        delete_len: u64,
        replacement: &[u8],
    ) -> Result<MountedSpliceReceipt, MountedError> {
        self.require_live()?;
        if self.has_dirty_state() {
            return Err(MountedError::Busy);
        }
        if replacement.len() > MAX_REQUEST_BYTES {
            return Err(MountedError::ResourceExhausted);
        }
        let before = self.accepted.clone();
        let reservation = self.budget.try_reserve(MAX_OPERATION_Q_BYTES)?;
        let result = crate::resolver::replace_range_at_ref(
            &self.engine,
            &before,
            path,
            start,
            delete_len,
            Cursor::new(replacement),
        );
        drop(reservation);
        let (after, mut counters) = match result {
            Ok(result) => result,
            Err(error) => {
                let mapped = mounted_vfs_error(error);
                return Err(self.classify_publication_error(mapped));
            }
        };
        #[cfg(test)]
        if self.splice_post_visibility_uncertainty {
            self.accepted = after;
            self.lifecycle = MountedLifecycle::Incomplete;
            return Err(MountedError::Indeterminate);
        }
        let (current, high) = self.budget.observation()?;
        counters.operation_q_current_bytes = current as u64;
        counters.operation_q_high_water_bytes = high as u64;
        counters.operation_q_terminal_bytes = current as u64;
        self.accepted = after.clone();
        self.counters.splices += 1;
        self.lifecycle = MountedLifecycle::Closed;
        self.budget.shutdown();
        Ok(MountedSpliceReceipt {
            before,
            after,
            counters,
            remount_required: true,
        })
    }

    pub fn lifecycle(&self) -> MountedLifecycle {
        self.lifecycle
    }

    pub fn mark_incomplete(&mut self) {
        self.lifecycle = MountedLifecycle::Incomplete;
    }

    fn classify_publication_error(&mut self, error: MountedError) -> MountedError {
        self.lifecycle = match error {
            MountedError::Conflict => MountedLifecycle::Conflict,
            MountedError::Indeterminate | MountedError::CommittedCleanup => {
                MountedLifecycle::Incomplete
            }
            _ => MountedLifecycle::Live,
        };
        error
    }

    pub fn byte_budget(&self) -> Arc<ByteBudget> {
        self.budget.clone()
    }

    pub fn counters(&mut self) -> Result<MountedCounters, MountedError> {
        self.observe_resources()?;
        Ok(self.counters)
    }

    pub fn capacity(&self) -> Result<MountedCapacity, MountedError> {
        self.require_live_or_incomplete_read()?;
        let free_bytes = MAX_LOGICAL_WORKSPACE_BYTES
            .saturating_sub(self.logical_workspace_bytes)
            .min(MAX_LIVE_SPOOL_BYTES.saturating_sub(self.spool.live));
        let free_files = MAX_MOUNTED_NODES
            .saturating_sub(self.nodes.len())
            .min(MAX_DIRTY_NODES.saturating_sub(self.dirty_nodes.len()))
            .min(MAX_DIRECTORY_CHANGES.saturating_sub(self.directory_changes));
        Ok(MountedCapacity {
            total_bytes: MAX_LOGICAL_WORKSPACE_BYTES,
            free_bytes,
            total_files: MAX_MOUNTED_NODES as u64,
            free_files: free_files as u64,
        })
    }

    pub fn engine_counters(&self) -> Result<EngineCounters, MountedError> {
        Ok(self.engine.counters()?)
    }

    pub fn active_store_connections(&self) -> Result<u64, MountedError> {
        Ok(self.engine.active_connection_count()?)
    }

    pub fn close_store_connection(&self) -> Result<(), MountedError> {
        Ok(self.engine.close_primary_connection()?)
    }

    pub fn reset_engine_counters(&self) -> Result<(), MountedError> {
        Ok(self.engine.reset_counters()?)
    }

    pub fn getattr(&mut self, node: MountedNodeId) -> Result<MountedAttr, MountedError> {
        self.require_live_or_incomplete_read()?;
        self.counters.getattr += 1;
        self.nodes
            .get(&node)
            .filter(|node| !node.deleted || node.open_refs != 0)
            .map(|value| value.attr(node))
            .ok_or(MountedError::NotFound)
    }

    pub fn lookup_child(
        &mut self,
        parent: MountedNodeId,
        name: &[u8],
    ) -> Result<MountedAttr, MountedError> {
        self.require_live_or_incomplete_read()?;
        let name = CanonicalName::from_bytes(name).map_err(|_| MountedError::InvalidName)?;
        let node = self
            .find_child(parent, &name)?
            .ok_or(MountedError::NotFound)?;
        let entry = self.nodes.get_mut(&node).ok_or(MountedError::Corrupt)?;
        entry.lookup_refs = entry.lookup_refs.saturating_add(1);
        self.lookup_refs = self.lookup_refs.saturating_add(1);
        self.counters.lookups += 1;
        Ok(entry.attr(node))
    }

    pub fn forget(&mut self, node: MountedNodeId, count: u64) {
        if node == ROOT_NODE {
            return;
        }
        if let Some(entry) = self.nodes.get_mut(&node) {
            let forgotten = entry.lookup_refs.min(count);
            entry.lookup_refs -= forgotten;
            self.lookup_refs = self.lookup_refs.saturating_sub(forgotten);
        }
        self.reclaim_node(node);
    }

    pub fn open_file(
        &mut self,
        node: MountedNodeId,
        truncate: bool,
    ) -> Result<MountedHandleId, MountedError> {
        self.require_live()?;
        self.preflight_handle(false)?;
        if self.nodes.get(&node).ok_or(MountedError::NotFound)?.kind != MountedFileType::RegularFile
        {
            return Err(MountedError::IsDirectory);
        }
        if truncate {
            self.truncate(node, 0)?;
        }
        let handle = self.allocate_handle()?;
        self.handles.insert(handle, Handle::File(node));
        self.nodes
            .get_mut(&node)
            .ok_or(MountedError::Corrupt)?
            .open_refs += 1;
        self.counters.opens += 1;
        self.observe_resources()?;
        Ok(handle)
    }

    pub fn open_directory(&mut self, node: MountedNodeId) -> Result<MountedHandleId, MountedError> {
        self.require_live_or_incomplete_read()?;
        self.preflight_handle(true)?;
        if self.nodes.get(&node).ok_or(MountedError::NotFound)?.kind != MountedFileType::Directory {
            return Err(MountedError::NotDirectory);
        }
        let handle = self.allocate_handle()?;
        self.handles.insert(
            handle,
            Handle::Directory(Box::new(DirectoryHandle {
                committed: DirectoryCursor::new(node),
                pending: None,
            })),
        );
        self.directory_cursors += 1;
        self.nodes
            .get_mut(&node)
            .ok_or(MountedError::Corrupt)?
            .open_refs += 1;
        self.counters.opens += 1;
        self.observe_resources()?;
        Ok(handle)
    }

    pub fn read(
        &mut self,
        node: MountedNodeId,
        handle: MountedHandleId,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, MountedError> {
        self.require_live_or_incomplete_read()?;
        if length > MAX_REQUEST_BYTES {
            return Err(MountedError::ResourceExhausted);
        }
        self.require_file_handle(node, handle)?;
        let (base, base_visible_len, mut plan, logical_len) =
            match &self.nodes.get(&node).ok_or(MountedError::NotFound)?.content {
                NodeContent::File {
                    base,
                    base_visible_len,
                    plan,
                    logical_len,
                    ..
                } => (*base, *base_visible_len, plan.clone(), *logical_len),
                _ => return Err(MountedError::IsDirectory),
            };
        if plan.is_none() {
            if let Some(base) = base {
                let mut counters = RopeCounters::default();
                let loaded = Arc::new(read_plan(&self.engine, base, &mut counters)?);
                let entry = self.nodes.get_mut(&node).ok_or(MountedError::Corrupt)?;
                let NodeContent::File { plan: cache, .. } = &mut entry.content else {
                    return Err(MountedError::IsDirectory);
                };
                *cache = Some(loaded.clone());
                plan = Some(loaded);
            }
        }
        if offset >= logical_len || length == 0 {
            return Ok(Vec::new());
        }
        let end = offset
            .checked_add(length as u64)
            .ok_or(MountedError::InvalidRange)?
            .min(logical_len);
        let mut output =
            vec![0_u8; usize::try_from(end - offset).map_err(|_| MountedError::InvalidRange)?];
        if let Some(base) = base {
            let base_len = plan.as_ref().map_or(0, |plan| plan.logical_len());
            let persisted_end = end.min(base_len).min(base_visible_len);
            if offset < persisted_end {
                let mut sink = Cursor::new(
                    &mut output[..usize::try_from(persisted_end - offset)
                        .map_err(|_| MountedError::InvalidRange)?],
                );
                let _rope = if let Some(plan) = plan {
                    read_range_with_plan(&self.engine, &plan, offset..persisted_end, &mut sink)?
                } else {
                    let mut counters = RopeCounters::default();
                    let plan = read_plan(&self.engine, base, &mut counters)?;
                    let read = read_range_with_plan(
                        &self.engine,
                        &plan,
                        offset..persisted_end,
                        &mut sink,
                    )?;
                    merge_rope(&mut counters, read)?;
                    counters
                };
            }
        }
        let (nodes, spool) = (&self.nodes, &mut self.spool);
        let ranges = match &nodes.get(&node).ok_or(MountedError::NotFound)?.content {
            NodeContent::File { ranges, .. } => ranges,
            _ => return Err(MountedError::IsDirectory),
        };
        for (start, range) in ranges.range(..end) {
            if range.end <= offset {
                continue;
            }
            let overlap_start = (*start).max(offset);
            let overlap_end = range.end.min(end);
            let destination =
                usize::try_from(overlap_start - offset).map_err(|_| MountedError::InvalidRange)?;
            let count = usize::try_from(overlap_end - overlap_start)
                .map_err(|_| MountedError::InvalidRange)?;
            let source = range
                .spool_offset
                .checked_add(overlap_start - *start)
                .ok_or(MountedError::InvalidRange)?;
            spool.read_exact_at(source, &mut output[destination..destination + count])?;
        }
        self.counters.reads += 1;
        self.observe_request(length)?;
        Ok(output)
    }

    pub fn write(
        &mut self,
        node: MountedNodeId,
        handle: MountedHandleId,
        offset: u64,
        bytes: &[u8],
    ) -> Result<usize, MountedError> {
        self.require_live()?;
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(MountedError::ResourceExhausted);
        }
        self.require_file_handle(node, handle)?;
        if bytes.is_empty() {
            return Ok(0);
        }
        let end = offset
            .checked_add(bytes.len() as u64)
            .ok_or(MountedError::InvalidRange)?;
        let entry = self.nodes.get(&node).ok_or(MountedError::NotFound)?;
        let deleted = entry.deleted;
        let old_len = match &entry.content {
            NodeContent::File { logical_len, .. } => *logical_len,
            _ => return Err(MountedError::IsDirectory),
        };
        let new_len = old_len.max(end);
        let projected_logical = if deleted {
            if new_len > MAX_LOGICAL_FILE_BYTES {
                return Err(MountedError::NoSpace);
            }
            None
        } else {
            self.preflight_dirty(&[node], 0)?;
            Some(self.preflight_logical_file(old_len, new_len)?)
        };
        if self.live_ranges.saturating_add(2) > MAX_DIRTY_RANGES {
            return Err(MountedError::ResourceExhausted);
        }
        if self
            .spool
            .live
            .checked_add(bytes.len() as u64)
            .is_none_or(|value| value > MAX_LIVE_SPOOL_BYTES)
        {
            return Err(MountedError::NoSpace);
        }
        if let Err(error) = self.compact_spool_if_needed(bytes.len() as u64) {
            self.lifecycle = MountedLifecycle::Incomplete;
            return Err(error);
        }
        let spool_offset = self.spool.next_offset(bytes.len())?;
        let timestamp = now_timestamp()?;
        let actual = match self.spool.append(bytes) {
            Ok(actual) => actual,
            Err(error) => {
                self.lifecycle = MountedLifecycle::Incomplete;
                return Err(error);
            }
        };
        if actual != spool_offset {
            self.lifecycle = MountedLifecycle::Incomplete;
            return Err(MountedError::Indeterminate);
        }
        self.counters.spool_physical_high_water_bytes = self
            .counters
            .spool_physical_high_water_bytes
            .max(self.spool.physical());
        let (old_count, new_count, removed, preserved) = {
            let entry = self.nodes.get_mut(&node).ok_or(MountedError::NotFound)?;
            let NodeContent::File {
                logical_len,
                ranges,
                plan,
                ..
            } = &mut entry.content
            else {
                self.lifecycle = MountedLifecycle::Incomplete;
                return Err(MountedError::IsDirectory);
            };
            let old_count = ranges.len();
            let (removed, preserved) = match install_dirty_range(ranges, offset, end, spool_offset)
            {
                Ok(effect) => effect,
                Err(error) => {
                    self.lifecycle = MountedLifecycle::Incomplete;
                    return Err(error);
                }
            };
            *logical_len = new_len;
            *plan = None;
            if !entry.deleted {
                entry.dirty_content = true;
                entry.dirty_metadata = true;
            }
            entry.mtime_seconds = timestamp.0;
            entry.mtime_nanoseconds = timestamp.1;
            (old_count, ranges.len(), removed, preserved)
        };
        self.live_ranges = self.live_ranges - old_count + new_count;
        self.spool.live = match self
            .spool
            .live
            .checked_sub(removed)
            .and_then(|value| value.checked_add(preserved))
            .and_then(|value| value.checked_add(bytes.len() as u64))
        {
            Some(live) => live,
            None => {
                self.lifecycle = MountedLifecycle::Incomplete;
                return Err(MountedError::Indeterminate);
            }
        };
        if let Some(projected) = projected_logical {
            self.logical_workspace_bytes = projected;
        }
        self.sync_node_state(node);
        self.counters.writes += 1;
        if let Err(error) = self.compact_spool_if_needed(0) {
            self.lifecycle = MountedLifecycle::Incomplete;
            return Err(error);
        }
        self.observe_request(bytes.len())?;
        self.observe_resources()?;
        Ok(bytes.len())
    }

    pub fn flush(&mut self, handle: MountedHandleId) -> Result<(), MountedError> {
        self.require_handle(handle)?;
        self.counters.flushes += 1;
        Ok(())
    }

    pub fn release(&mut self, handle: MountedHandleId) -> Result<(), MountedError> {
        let handle = self
            .handles
            .remove(&handle)
            .ok_or(MountedError::InvalidHandle)?;
        let node = match handle {
            Handle::File(node) => node,
            Handle::Directory(directory) => {
                self.directory_cursors = self.directory_cursors.saturating_sub(1);
                directory.committed.node
            }
        };
        let entry = self.nodes.get_mut(&node).ok_or(MountedError::Corrupt)?;
        entry.open_refs = entry.open_refs.saturating_sub(1);
        self.counters.releases += 1;
        if let Err(error) = self.finalize_deleted_pending(node) {
            self.lifecycle = MountedLifecycle::Incomplete;
            return Err(error);
        }
        self.reclaim_node(node);
        self.observe_resources()?;
        Ok(())
    }

    pub fn create_file(
        &mut self,
        parent: MountedNodeId,
        name: &[u8],
        mode: u32,
    ) -> Result<(MountedAttr, MountedHandleId), MountedError> {
        self.preflight_handle(false)?;
        let node =
            self.create_node(parent, name, MountedFileType::RegularFile, mode, Vec::new())?;
        let handle = self.open_file(node, false)?;
        self.counters.creates += 1;
        Ok((self.getattr(node)?, handle))
    }

    pub fn mknod_file(
        &mut self,
        parent: MountedNodeId,
        name: &[u8],
        mode: u32,
    ) -> Result<MountedAttr, MountedError> {
        let node =
            self.create_node(parent, name, MountedFileType::RegularFile, mode, Vec::new())?;
        self.counters.creates += 1;
        self.getattr(node)
    }

    pub fn mkdir(
        &mut self,
        parent: MountedNodeId,
        name: &[u8],
        mode: u32,
    ) -> Result<MountedAttr, MountedError> {
        let node = self.create_node(parent, name, MountedFileType::Directory, mode, Vec::new())?;
        self.counters.mkdirs += 1;
        self.getattr(node)
    }

    pub fn symlink(
        &mut self,
        parent: MountedNodeId,
        name: &[u8],
        target: Vec<u8>,
    ) -> Result<MountedAttr, MountedError> {
        SymlinkStateV1::new(target.clone())?;
        let node = self.create_node(parent, name, MountedFileType::Symlink, 0o777, target)?;
        self.counters.symlinks += 1;
        self.getattr(node)
    }

    pub fn readlink(&self, node: MountedNodeId) -> Result<Vec<u8>, MountedError> {
        match &self.nodes.get(&node).ok_or(MountedError::NotFound)?.content {
            NodeContent::Symlink { target } => Ok(target.clone()),
            _ => Err(MountedError::InvalidRange),
        }
    }

    pub fn link(
        &mut self,
        node: MountedNodeId,
        parent: MountedNodeId,
        name: &[u8],
    ) -> Result<MountedAttr, MountedError> {
        self.require_live()?;
        let name = CanonicalName::from_bytes(name).map_err(|_| MountedError::InvalidName)?;
        if self.find_child(parent, &name)?.is_some() {
            return Err(MountedError::AlreadyExists);
        }
        let entry = self.nodes.get(&node).ok_or(MountedError::NotFound)?;
        if entry.kind != MountedFileType::RegularFile || entry.deleted {
            return Err(MountedError::Unsupported);
        }
        let namespace_refs = entry
            .namespace_refs
            .checked_add(1)
            .ok_or(MountedError::ResourceExhausted)?;
        self.preflight_dirty(&[node, parent], 0)?;
        let mutation = self.prepare_directory_entry(parent, name, Some(node))?;
        self.apply_directory_mutations([mutation])?;
        let entry = self.nodes.get_mut(&node).ok_or(MountedError::Corrupt)?;
        entry.namespace_refs = namespace_refs;
        entry.dirty_links = entry.canonical.is_some();
        self.sync_node_state(node);
        self.counters.links += 1;
        self.getattr(node)
    }

    pub fn unlink(&mut self, parent: MountedNodeId, name: &[u8]) -> Result<(), MountedError> {
        self.unlink_inner(parent, name, false)
    }

    pub fn rmdir(&mut self, parent: MountedNodeId, name: &[u8]) -> Result<(), MountedError> {
        self.unlink_inner(parent, name, true)
    }

    fn unlink_inner(
        &mut self,
        parent: MountedNodeId,
        name: &[u8],
        directory: bool,
    ) -> Result<(), MountedError> {
        self.require_live()?;
        let name = CanonicalName::from_bytes(name).map_err(|_| MountedError::InvalidName)?;
        let node = self
            .find_child(parent, &name)?
            .ok_or(MountedError::NotFound)?;
        let kind = self.nodes.get(&node).ok_or(MountedError::Corrupt)?.kind;
        if directory != (kind == MountedFileType::Directory) {
            return Err(if directory {
                MountedError::NotDirectory
            } else {
                MountedError::IsDirectory
            });
        }
        if directory && !self.directory_is_empty(node)? {
            return Err(MountedError::NotEmpty);
        }
        self.preflight_dirty(&[parent, node], 0)?;
        let mutation = self.prepare_directory_entry(parent, name, None)?;
        self.apply_directory_mutations([mutation])?;
        self.detach_node(node)?;
        self.counters.unlinks += 1;
        if let Err(error) = self.normalize_spool() {
            self.lifecycle = MountedLifecycle::Incomplete;
            return Err(error);
        }
        self.observe_resources()?;
        Ok(())
    }

    pub fn rename(
        &mut self,
        parent: MountedNodeId,
        name: &[u8],
        new_parent: MountedNodeId,
        new_name: &[u8],
        no_replace: bool,
    ) -> Result<(), MountedError> {
        self.require_live()?;
        let name = CanonicalName::from_bytes(name).map_err(|_| MountedError::InvalidName)?;
        let new_name =
            CanonicalName::from_bytes(new_name).map_err(|_| MountedError::InvalidName)?;
        let source = self
            .find_child(parent, &name)?
            .ok_or(MountedError::NotFound)?;
        if parent == new_parent && name == new_name {
            return Ok(());
        }
        let source_kind = self.nodes.get(&source).ok_or(MountedError::Corrupt)?.kind;
        if source_kind == MountedFileType::Directory {
            let mut ancestor = new_parent;
            loop {
                if ancestor == source {
                    return Err(MountedError::InvalidRange);
                }
                let next = self
                    .nodes
                    .get(&ancestor)
                    .ok_or(MountedError::NotFound)?
                    .parent;
                if next == ancestor {
                    break;
                }
                ancestor = next;
            }
        }
        let target = self.find_child(new_parent, &new_name)?;
        if let Some(target) = target {
            if no_replace {
                return Err(MountedError::AlreadyExists);
            }
            if target == source {
                return Ok(());
            }
            let target_kind = self.nodes.get(&target).ok_or(MountedError::Corrupt)?.kind;
            if (source_kind == MountedFileType::Directory)
                != (target_kind == MountedFileType::Directory)
            {
                return Err(if source_kind == MountedFileType::Directory {
                    MountedError::NotDirectory
                } else {
                    MountedError::IsDirectory
                });
            }
            if target_kind == MountedFileType::Directory && !self.directory_is_empty(target)? {
                return Err(MountedError::NotEmpty);
            }
        }
        let mut affected = vec![parent, new_parent];
        if let Some(target) = target {
            affected.push(target);
        }
        self.preflight_dirty(&affected, 0)?;
        let mutations = vec![
            self.prepare_directory_entry(parent, name, None)?,
            self.prepare_directory_entry(new_parent, new_name, Some(source))?,
        ];
        self.apply_directory_mutations(mutations)?;
        if let Some(target) = target {
            self.detach_node(target)?;
        }
        if source_kind == MountedFileType::Directory {
            self.nodes
                .get_mut(&source)
                .ok_or(MountedError::Corrupt)?
                .parent = new_parent;
        }
        self.counters.renames += 1;
        if let Err(error) = self.normalize_spool() {
            self.lifecycle = MountedLifecycle::Incomplete;
            return Err(error);
        }
        self.observe_resources()?;
        Ok(())
    }

    pub fn truncate(&mut self, node: MountedNodeId, length: u64) -> Result<(), MountedError> {
        self.require_live()?;
        let entry = self.nodes.get(&node).ok_or(MountedError::NotFound)?;
        let deleted = entry.deleted;
        let old_len = match &entry.content {
            NodeContent::File { logical_len, .. } => *logical_len,
            _ => return Err(MountedError::IsDirectory),
        };
        let projected_logical = if deleted {
            if length > MAX_LOGICAL_FILE_BYTES {
                return Err(MountedError::NoSpace);
            }
            None
        } else {
            self.preflight_dirty(&[node], 0)?;
            Some(self.preflight_logical_file(old_len, length)?)
        };
        let timestamp = now_timestamp()?;
        let (removed, old_count, new_count) = {
            let entry = self.nodes.get_mut(&node).ok_or(MountedError::NotFound)?;
            let NodeContent::File {
                base_visible_len,
                logical_len,
                ranges,
                plan,
                ..
            } = &mut entry.content
            else {
                return Err(MountedError::IsDirectory);
            };
            let old_count = ranges.len();
            let removed = truncate_dirty_ranges(ranges, length);
            *base_visible_len = (*base_visible_len).min(length);
            *logical_len = length;
            *plan = None;
            if !entry.deleted {
                entry.dirty_content = true;
                entry.dirty_metadata = true;
            }
            entry.mtime_seconds = timestamp.0;
            entry.mtime_nanoseconds = timestamp.1;
            (removed, old_count, ranges.len())
        };
        self.live_ranges = self.live_ranges - old_count + new_count;
        self.spool.live = self.spool.live.saturating_sub(removed);
        if let Some(projected) = projected_logical {
            self.logical_workspace_bytes = projected;
        }
        self.sync_node_state(node);
        if let Err(error) = self.normalize_spool() {
            self.lifecycle = MountedLifecycle::Incomplete;
            return Err(error);
        }
        self.observe_resources()?;
        Ok(())
    }

    pub fn chmod(&mut self, node: MountedNodeId, mode: u32) -> Result<MountedAttr, MountedError> {
        self.require_live()?;
        if !self.nodes.get(&node).ok_or(MountedError::NotFound)?.deleted {
            self.preflight_dirty(&[node], 0)?;
        }
        let entry = self.nodes.get_mut(&node).ok_or(MountedError::NotFound)?;
        entry.mode = mode
            & if entry.kind == MountedFileType::Directory {
                0o1777
            } else {
                0o777
            };
        if !entry.deleted {
            entry.dirty_metadata = true;
        }
        self.sync_node_state(node);
        self.getattr(node)
    }

    pub fn set_mtime(
        &mut self,
        node: MountedNodeId,
        seconds: i64,
        nanoseconds: u32,
    ) -> Result<MountedAttr, MountedError> {
        self.require_live()?;
        if nanoseconds > 999_999_999 {
            return Err(MountedError::InvalidRange);
        }
        if !self.nodes.get(&node).ok_or(MountedError::NotFound)?.deleted {
            self.preflight_dirty(&[node], 0)?;
        }
        let entry = self.nodes.get_mut(&node).ok_or(MountedError::NotFound)?;
        entry.mtime_seconds = seconds;
        entry.mtime_nanoseconds = nanoseconds;
        if !entry.deleted {
            entry.dirty_metadata = true;
        }
        self.sync_node_state(node);
        self.getattr(node)
    }

    pub fn readdir(
        &mut self,
        handle: MountedHandleId,
        offset: u64,
        max_entries: usize,
    ) -> Result<Vec<MountedDirEntry>, MountedError> {
        let mut output = Vec::with_capacity(max_entries.min(DIRECTORY_PAGE_ENTRIES));
        let mut committed = offset;
        while output.len() < max_entries {
            let Some(entry) = self.readdir_next(handle, committed)? else {
                break;
            };
            committed = entry.next_offset;
            self.commit_readdir(handle, committed)?;
            output.push(entry);
        }
        Ok(output)
    }

    pub fn readdir_next(
        &mut self,
        handle: MountedHandleId,
        offset: u64,
    ) -> Result<Option<MountedDirEntry>, MountedError> {
        self.require_live_or_incomplete_read()?;
        let mut directory = match self.handles.remove(&handle) {
            Some(Handle::Directory(directory)) => directory,
            Some(other) => {
                self.handles.insert(handle, other);
                return Err(MountedError::InvalidHandle);
            }
            None => return Err(MountedError::InvalidHandle),
        };
        let result = (|| {
            if offset != directory.committed.cookie {
                directory.committed = DirectoryCursor::new(directory.committed.node);
                directory.pending = None;
                while directory.committed.cookie < offset {
                    if self
                        .next_directory_entry(&mut directory.committed)?
                        .is_none()
                    {
                        return Ok(None);
                    }
                }
            }
            if let Some((entry, _)) = &directory.pending {
                return Ok(Some(entry.clone()));
            }
            let mut after = directory.committed.clone();
            let entry = self.next_directory_entry(&mut after)?;
            if let Some(entry) = &entry {
                directory.pending = Some((entry.clone(), after));
            }
            Ok(entry)
        })();
        self.handles.insert(handle, Handle::Directory(directory));
        result
    }

    pub fn commit_readdir(
        &mut self,
        handle: MountedHandleId,
        next_offset: u64,
    ) -> Result<(), MountedError> {
        let Some(Handle::Directory(directory)) = self.handles.get_mut(&handle) else {
            return Err(MountedError::InvalidHandle);
        };
        let (entry, after) = directory.pending.take().ok_or(MountedError::InvalidRange)?;
        if entry.next_offset != next_offset {
            directory.pending = Some((entry, after));
            return Err(MountedError::InvalidRange);
        }
        directory.committed = after;
        Ok(())
    }

    pub fn discard_readdir_pending(&mut self, handle: MountedHandleId) -> Result<(), MountedError> {
        let Some(Handle::Directory(directory)) = self.handles.get_mut(&handle) else {
            return Err(MountedError::InvalidHandle);
        };
        directory.pending = None;
        Ok(())
    }

    pub fn reclaim_readdir_nodes(&mut self, nodes: &[MountedNodeId]) {
        for node in nodes {
            self.reclaim_node(*node);
        }
    }

    pub fn fsync(&mut self) -> Result<RefState, MountedError> {
        self.counters.fsyncs += 1;
        self.checkpoint()
    }

    pub fn fsyncdir(&mut self) -> Result<RefState, MountedError> {
        self.counters.fsyncdirs += 1;
        self.checkpoint()
    }

    pub fn checkpoint(&mut self) -> Result<RefState, MountedError> {
        self.require_live()?;
        if !self.has_dirty_state() {
            self.counters.no_op_checkpoints += 1;
            return Ok(self.accepted.clone());
        }
        self.lifecycle = MountedLifecycle::Checkpointing;
        let result = self.checkpoint_inner();
        match result {
            Ok(state) => {
                self.lifecycle = MountedLifecycle::Live;
                self.counters.checkpoints += 1;
                Ok(state)
            }
            Err(error) => Err(self.classify_publication_error(error)),
        }
    }

    pub fn fork_ref(&self, name: &str) -> Result<RefState, MountedError> {
        self.require_live()?;
        if self.has_dirty_state() {
            return Err(MountedError::Busy);
        }
        Ok(self.engine.fork_ref(&self.accepted, name)?)
    }

    pub fn rollback(&mut self, target: ObjectId) -> Result<RefState, MountedError> {
        self.require_live()?;
        if self.has_dirty_state() || !self.handles.is_empty() {
            return Err(MountedError::Busy);
        }
        let state = match self.engine.move_ref(&self.accepted, target) {
            Ok(state) => state,
            Err(error) => return Err(self.classify_publication_error(error.into())),
        };
        self.accepted = state.clone();
        self.lifecycle = MountedLifecycle::Closed;
        self.budget.shutdown();
        Ok(state)
    }

    pub fn shutdown(&mut self) -> Result<RefState, MountedError> {
        if self.lifecycle == MountedLifecycle::Closed {
            return Ok(self.accepted.clone());
        }
        self.require_live()?;
        self.lifecycle = MountedLifecycle::Closing;
        let dirty = self.has_dirty_state();
        let result = if dirty {
            self.checkpoint_inner()
        } else {
            self.counters.no_op_checkpoints += 1;
            Ok(self.accepted.clone())
        };
        let state = match result {
            Ok(state) => {
                if dirty {
                    self.counters.checkpoints += 1;
                }
                state
            }
            Err(MountedError::Conflict) => {
                self.lifecycle = MountedLifecycle::Conflict;
                return Err(MountedError::Conflict);
            }
            Err(error) => {
                self.lifecycle = MountedLifecycle::Incomplete;
                return Err(error);
            }
        };
        match self.spool.reset() {
            Ok(true) => self.counters.spool_resets += 1,
            Ok(false) => {}
            Err(error) => {
                self.lifecycle = MountedLifecycle::Incomplete;
                return Err(error);
            }
        }
        self.budget.shutdown();
        self.namespace = self
            .engine
            .with_authenticated_canonical(self.accepted.root, decode_namespace_root)?;
        self.logical_workspace_bytes =
            accepted_logical_bytes(&self.engine, self.namespace.inode_table_root)?;
        self.lifecycle = MountedLifecycle::Closed;
        self.observe_resources()?;
        Ok(state)
    }

    pub fn release_kernel_cache_ownership(&mut self) -> Result<(), MountedError> {
        if self.lifecycle != MountedLifecycle::Closed {
            return Err(MountedError::Busy);
        }
        let (q_current, _) = self.budget.observation()?;
        if !self.dirty_nodes.is_empty()
            || !self.pending_nodes.is_empty()
            || self.directory_changes != 0
            || self.spool.live != 0
            || self.spool.physical() != 0
            || q_current != 0
        {
            self.lifecycle = MountedLifecycle::Incomplete;
            return Err(MountedError::Indeterminate);
        }
        self.namespace = self
            .engine
            .with_authenticated_canonical(self.accepted.root, decode_namespace_root)?;
        self.logical_workspace_bytes =
            accepted_logical_bytes(&self.engine, self.namespace.inode_table_root)?;
        let root_inode = self.namespace.root_directory_inode;
        let root = self
            .nodes
            .get_mut(&ROOT_NODE)
            .ok_or(MountedError::Corrupt)?;
        root.lookup_refs = 1;
        root.open_refs = 0;
        self.handles.clear();
        self.directory_cursors = 0;
        self.live_ranges = 0;
        self.nodes.retain(|id, _| *id == ROOT_NODE);
        self.by_inode.clear();
        self.by_inode.insert(root_inode, ROOT_NODE);
        self.reclaimable_inode_mappings.clear();
        self.lookup_refs = 1;
        self.observe_resources()
    }

    fn checkpoint_inner(&mut self) -> Result<RefState, MountedError> {
        let _snapshot_reservation = self.budget.try_reserve(MAX_OPERATION_Q_BYTES)?;
        let mut dirty_ids = self.dirty_nodes.iter().copied().collect::<Vec<_>>();
        dirty_ids.sort_unstable();
        if dirty_ids.len() > MAX_DIRTY_NODES {
            return Err(MountedError::ResourceExhausted);
        }
        let mut canonical_ids = HashMap::new();
        for id in &dirty_ids {
            let node = self.nodes.get(id).ok_or(MountedError::Corrupt)?;
            if let Some(inode) = node.canonical {
                canonical_ids.insert(*id, inode);
            }
            if let NodeContent::Directory { changes, .. } = &node.content {
                for child in changes.values().flatten() {
                    if let Some(inode) = self
                        .nodes
                        .get(child)
                        .ok_or(MountedError::Corrupt)?
                        .canonical
                    {
                        canonical_ids.insert(*child, inode);
                    }
                }
            }
        }
        let mut publication = self
            .engine
            .begin_publication(Some(&self.accepted), &self.accepted.name)?;
        for id in &dirty_ids {
            let node = self.nodes.get(id).ok_or(MountedError::Corrupt)?;
            if node.canonical.is_none() && !node.deleted {
                canonical_ids.insert(*id, publication.allocate_inode_id()?);
            }
        }
        let mut table = InodeTableRoot(self.namespace.inode_table_root);
        let mut persisted = HashMap::new();
        for id in &dirty_ids {
            let node = self.nodes.get(id).ok_or(MountedError::Corrupt)?;
            if node.deleted {
                continue;
            }
            let metadata_entries = (node.dirty_metadata && node.record.is_some())
                .then(|| metadata_tree_entries(&publication, node.record.unwrap().metadata_root))
                .transpose()?;
            let snapshot = CheckpointNode {
                canonical: node.canonical,
                record: node.record,
                kind: node.kind,
                mode: node.mode,
                mtime_seconds: node.mtime_seconds,
                mtime_nanoseconds: node.mtime_nanoseconds,
                namespace_refs: node.namespace_refs,
                dirty_content: node.dirty_content,
                dirty_metadata: node.dirty_metadata,
                content: node.content.clone(),
                metadata_entries,
            };
            let inode = *canonical_ids.get(id).ok_or(MountedError::Corrupt)?;
            let content_root = if snapshot.canonical.is_none() || snapshot.dirty_content {
                Self::persist_content(&mut self.spool, &mut publication, &snapshot, &canonical_ids)?
            } else {
                snapshot.record.ok_or(MountedError::Corrupt)?.content_root
            };
            let metadata_root = if snapshot.canonical.is_none() || snapshot.dirty_metadata {
                persist_metadata(&mut publication, &snapshot)?
            } else {
                snapshot.record.ok_or(MountedError::Corrupt)?.metadata_root
            };
            let record = InodeRecordV1 {
                kind: inode_kind(snapshot.kind),
                namespace_ref_count: snapshot.namespace_refs,
                content_root,
                metadata_root,
            };
            record.validate(*id == ROOT_NODE)?;
            let record_id = publication.put_object(&encode_inode_record(record)?)?;
            let (next, _) = inode_table_upsert(&mut publication, table, inode, record_id)?;
            table = next;
            persisted.insert(*id, (inode, record));
        }
        for id in &dirty_ids {
            let node = self.nodes.get(id).ok_or(MountedError::Corrupt)?;
            if node.deleted {
                if let Some(inode) = node.canonical {
                    let (next, _, _) = inode_table_remove(&mut publication, table, inode)?;
                    table = next;
                }
            }
        }
        let namespace = NamespaceRootV1 {
            inode_table_root: table.0,
            ..self.namespace
        };
        let state = publication.publish_namespace(&encode_namespace_root(namespace)?)?;
        self.accepted = state.clone();
        self.namespace = namespace;
        let cleanup = (|| -> Result<(), MountedError> {
            for id in &dirty_ids {
                if self.nodes.get(id).ok_or(MountedError::Corrupt)?.deleted {
                    continue;
                }
                let (inode, record) = *persisted.get(id).ok_or(MountedError::Corrupt)?;
                let entry = self.nodes.get_mut(id).ok_or(MountedError::Corrupt)?;
                entry.canonical = Some(inode);
                entry.record = Some(record);
                entry.dirty_content = false;
                entry.dirty_metadata = false;
                entry.dirty_links = false;
                entry.directory_mtime_before = None;
                let mut cleared_ranges = (0, 0_u64);
                match &mut entry.content {
                    NodeContent::File {
                        base,
                        base_visible_len,
                        logical_len,
                        ranges,
                        plan,
                    } => {
                        *base = Some(FileStateRoot(record.content_root));
                        *base_visible_len = *logical_len;
                        cleared_ranges = (
                            ranges.len(),
                            ranges.iter().map(|(start, range)| range.end - *start).sum(),
                        );
                        ranges.clear();
                        *plan = None;
                    }
                    NodeContent::Directory { base, changes } => {
                        *base = Some(DirectoryStateRoot(record.content_root));
                        self.directory_changes =
                            self.directory_changes.saturating_sub(changes.len());
                        changes.clear();
                    }
                    NodeContent::Symlink { .. } => {}
                }
                self.live_ranges = self.live_ranges.saturating_sub(cleared_ranges.0);
                self.spool.live = self.spool.live.saturating_sub(cleared_ranges.1);
                self.by_inode.insert(inode, *id);
                self.sync_node_state(*id);
            }
            for id in dirty_ids
                .iter()
                .copied()
                .filter(|id| self.nodes.get(id).is_some_and(|node| node.deleted))
                .collect::<Vec<_>>()
            {
                if self
                    .nodes
                    .get(&id)
                    .is_some_and(|entry| entry.open_refs == 0)
                {
                    self.drain_node_file_ranges(id)?;
                }
                if let Some(entry) = self.nodes.get_mut(&id) {
                    if let Some(inode) = entry.canonical.take() {
                        self.by_inode.remove(&inode);
                        self.reclaimable_inode_mappings.remove(&inode);
                    }
                    entry.dirty_content = false;
                    entry.dirty_metadata = false;
                    entry.dirty_links = false;
                    entry.directory_mtime_before = None;
                }
                self.sync_node_state(id);
                self.reclaim_node(id);
            }
            self.normalize_spool_during_checkpoint()?;
            self.observe_resources()?;
            Ok(())
        })();
        if cleanup.is_err() {
            self.lifecycle = MountedLifecycle::Incomplete;
            return Err(MountedError::CommittedCleanup);
        }
        Ok(state)
    }

    fn persist_content(
        spool: &mut Spool,
        publication: &mut Publication<'_>,
        node: &CheckpointNode,
        canonical_ids: &HashMap<MountedNodeId, InodeId>,
    ) -> Result<ObjectId, MountedError> {
        match &node.content {
            NodeContent::File {
                base,
                base_visible_len,
                logical_len,
                ranges,
                ..
            } => {
                let (mut root, mut current_len) = if let Some(root) = base {
                    let mut counters = RopeCounters::default();
                    let state =
                        layerfs_core::content::rope::state(publication, *root, &mut counters)?;
                    (*root, state.logical_len)
                } else {
                    let (root, _) = build(publication, Cursor::new(&[]))?;
                    (root, 0)
                };
                if current_len > *base_visible_len {
                    (root, _) = replace(
                        publication,
                        root,
                        *base_visible_len,
                        current_len - *base_visible_len,
                        Cursor::new(&[]),
                    )?;
                    current_len = *base_visible_len;
                }
                for (start, range) in ranges {
                    if *start > current_len {
                        let gap = *start - current_len;
                        (root, _) = replace(publication, root, current_len, 0, ZeroReader(gap))?;
                        current_len = *start;
                    }
                    let length = range.end - *start;
                    let delete = length.min(current_len.saturating_sub(*start));
                    let slice = spool.slice(range.spool_offset, length)?;
                    (root, _) = replace(publication, root, *start, delete, slice)?;
                    current_len = current_len.max(range.end);
                }
                match current_len.cmp(logical_len) {
                    Ordering::Greater => {
                        (root, _) = replace(
                            publication,
                            root,
                            *logical_len,
                            current_len - *logical_len,
                            Cursor::new(&[]),
                        )?;
                    }
                    Ordering::Less => {
                        (root, _) = replace(
                            publication,
                            root,
                            current_len,
                            0,
                            ZeroReader(*logical_len - current_len),
                        )?;
                    }
                    Ordering::Equal => {}
                }
                Ok(root.0)
            }
            NodeContent::Directory { base, changes } => {
                let mut root = match base {
                    Some(root) => *root,
                    None => empty_directory(publication)?,
                };
                for (name, desired) in changes {
                    if directory_lookup(publication, root, name, &mut NamespaceCounters::default())?
                        .is_some()
                    {
                        (root, _, _) = directory_remove(publication, root, name)?;
                    }
                    if let Some(child) = desired {
                        let inode = *canonical_ids.get(child).ok_or(MountedError::Corrupt)?;
                        (root, _) = directory_insert(publication, root, name.clone(), inode)?;
                    }
                }
                Ok(root.0)
            }
            NodeContent::Symlink { target } => {
                Ok(publication
                    .put_object(&encode_symlink(&SymlinkStateV1::new(target.clone())?)?)?)
            }
        }
    }

    fn create_node(
        &mut self,
        parent: MountedNodeId,
        name: &[u8],
        kind: MountedFileType,
        mode: u32,
        target: Vec<u8>,
    ) -> Result<MountedNodeId, MountedError> {
        self.require_live()?;
        if self.nodes.len() == MAX_MOUNTED_NODES {
            return Err(MountedError::ResourceExhausted);
        }
        let name = CanonicalName::from_bytes(name).map_err(|_| MountedError::InvalidName)?;
        if self.nodes.get(&parent).ok_or(MountedError::NotFound)?.kind != MountedFileType::Directory
        {
            return Err(MountedError::NotDirectory);
        }
        if self.directory_changes == MAX_DIRECTORY_CHANGES {
            let NodeContent::Directory { changes, .. } = &self
                .nodes
                .get(&parent)
                .ok_or(MountedError::NotFound)?
                .content
            else {
                return Err(MountedError::NotDirectory);
            };
            match changes.get(&name) {
                Some(None) => {}
                Some(Some(_)) => return Err(MountedError::AlreadyExists),
                None => return Err(MountedError::ResourceExhausted),
            }
        }
        if self.find_child(parent, &name)?.is_some() {
            return Err(MountedError::AlreadyExists);
        }
        self.preflight_dirty(&[parent], 1)?;
        let id = MountedNodeId(self.next_node);
        let (mtime_seconds, mtime_nanoseconds) = now_timestamp()?;
        let content = match kind {
            MountedFileType::RegularFile => NodeContent::File {
                base: None,
                base_visible_len: 0,
                logical_len: 0,
                ranges: BTreeMap::new(),
                plan: None,
            },
            MountedFileType::Directory => NodeContent::Directory {
                base: None,
                changes: BTreeMap::new(),
            },
            MountedFileType::Symlink => NodeContent::Symlink { target },
        };
        let mutation = self.prepare_directory_entry(parent, name, Some(id))?;
        self.preflight_directory_mutations(std::slice::from_ref(&mutation))?;
        let allocated = self.allocate_node()?;
        debug_assert_eq!(allocated, id);
        self.nodes.insert(
            id,
            MountedNode {
                canonical: None,
                record: None,
                kind,
                mode: mode
                    & if kind == MountedFileType::Directory {
                        0o1777
                    } else {
                        0o777
                    },
                mtime_seconds,
                mtime_nanoseconds,
                namespace_refs: 1,
                parent,
                lookup_refs: 1,
                open_refs: 0,
                deleted: false,
                dirty_content: true,
                dirty_metadata: true,
                dirty_links: false,
                directory_mtime_before: None,
                content,
            },
        );
        self.lookup_refs = self.lookup_refs.saturating_add(1);
        self.sync_node_state(id);
        self.apply_directory_mutations([mutation])?;
        self.observe_resources()?;
        Ok(id)
    }

    fn find_child(
        &mut self,
        parent: MountedNodeId,
        name: &CanonicalName,
    ) -> Result<Option<MountedNodeId>, MountedError> {
        let (change, base) = match &self
            .nodes
            .get(&parent)
            .ok_or(MountedError::NotFound)?
            .content
        {
            NodeContent::Directory { base, changes } => (changes.get(name).copied(), *base),
            _ => return Err(MountedError::NotDirectory),
        };
        if let Some(change) = change {
            return Ok(change);
        }
        let Some(base) = base else {
            return Ok(None);
        };
        let inode = directory_lookup(&self.engine, base, name, &mut NamespaceCounters::default())?;
        inode
            .map(|inode| self.ensure_canonical_node(inode, parent))
            .transpose()
    }

    fn prepare_directory_entry(
        &self,
        parent: MountedNodeId,
        name: CanonicalName,
        desired: Option<MountedNodeId>,
    ) -> Result<DirectoryMutation, MountedError> {
        let (base, change_exists) = match &self
            .nodes
            .get(&parent)
            .ok_or(MountedError::NotFound)?
            .content
        {
            NodeContent::Directory { base, changes } => (*base, changes.contains_key(&name)),
            _ => return Err(MountedError::NotDirectory),
        };
        let base_inode = base
            .map(|root| {
                directory_lookup(&self.engine, root, &name, &mut NamespaceCounters::default())
            })
            .transpose()?
            .flatten();
        let desired_inode =
            desired.and_then(|id| self.nodes.get(&id).and_then(|node| node.canonical));
        let normalized = match desired {
            None if base_inode.is_none() => None,
            Some(_) if base_inode.is_some() && desired_inode == base_inode => None,
            value => Some(value),
        };
        let change_delta = match (change_exists, normalized.is_some()) {
            (false, true) => 1,
            (true, false) => -1,
            _ => 0,
        };
        Ok(DirectoryMutation {
            parent,
            name,
            normalized,
            change_delta,
            timestamp: now_timestamp()?,
        })
    }

    fn apply_directory_mutations(
        &mut self,
        mutations: impl IntoIterator<Item = DirectoryMutation>,
    ) -> Result<(), MountedError> {
        let mutations = mutations.into_iter().collect::<Vec<_>>();
        self.preflight_directory_mutations(&mutations)?;
        for mutation in mutations {
            let result = (|| {
                let parent_node = self
                    .nodes
                    .get_mut(&mutation.parent)
                    .ok_or(MountedError::Corrupt)?;
                let NodeContent::Directory { changes, .. } = &mut parent_node.content else {
                    return Err(MountedError::Corrupt);
                };
                match mutation.normalized {
                    Some(value) => {
                        changes.insert(mutation.name.clone(), value);
                    }
                    None => {
                        changes.remove(&mutation.name);
                    }
                }
                self.directory_changes = usize::try_from(
                    i64::try_from(self.directory_changes)
                        .map_err(|_| MountedError::Indeterminate)?
                        .checked_add(i64::from(mutation.change_delta))
                        .ok_or(MountedError::Indeterminate)?,
                )
                .map_err(|_| MountedError::Indeterminate)?;
                if parent_node.directory_mtime_before.is_none() {
                    parent_node.directory_mtime_before = Some((
                        parent_node.mtime_seconds,
                        parent_node.mtime_nanoseconds,
                        parent_node.dirty_metadata,
                    ));
                }
                parent_node.dirty_content = parent_node.canonical.is_none() || !changes.is_empty();
                if changes.is_empty() {
                    if let Some((seconds, nanos, dirty)) = parent_node.directory_mtime_before.take()
                    {
                        parent_node.mtime_seconds = seconds;
                        parent_node.mtime_nanoseconds = nanos;
                        parent_node.dirty_metadata = dirty;
                    }
                } else {
                    parent_node.mtime_seconds = mutation.timestamp.0;
                    parent_node.mtime_nanoseconds = mutation.timestamp.1;
                    parent_node.dirty_metadata = true;
                }
                Ok(())
            })();
            if let Err(error) = result {
                self.lifecycle = MountedLifecycle::Incomplete;
                return Err(error);
            }
            self.sync_node_state(mutation.parent);
        }
        Ok(())
    }

    fn preflight_directory_mutations(
        &self,
        mutations: &[DirectoryMutation],
    ) -> Result<(), MountedError> {
        let projected = mutations.iter().try_fold(
            i64::try_from(self.directory_changes).map_err(|_| MountedError::ResourceExhausted)?,
            |current, mutation| {
                current
                    .checked_add(i64::from(mutation.change_delta))
                    .ok_or(MountedError::ResourceExhausted)
            },
        )?;
        if !(0..=MAX_DIRECTORY_CHANGES as i64).contains(&projected) {
            return Err(MountedError::ResourceExhausted);
        }
        Ok(())
    }

    fn detach_node(&mut self, node: MountedNodeId) -> Result<(), MountedError> {
        let entry = self.nodes.get_mut(&node).ok_or(MountedError::NotFound)?;
        entry.namespace_refs = entry.namespace_refs.saturating_sub(1);
        if entry.namespace_refs != 0 {
            entry.dirty_links = entry.canonical.is_some();
            self.sync_node_state(node);
            return Ok(());
        }
        entry.deleted = true;
        if entry.canonical.is_some() {
            entry.dirty_links = true;
        } else {
            self.counters.created_then_deleted += 1;
            entry.dirty_content = false;
            entry.dirty_metadata = false;
            entry.dirty_links = false;
        }
        let removed_logical = match &entry.content {
            NodeContent::File { logical_len, .. } => *logical_len,
            _ => 0,
        };
        let _ = entry;
        self.logical_workspace_bytes =
            match self.logical_workspace_bytes.checked_sub(removed_logical) {
                Some(bytes) => bytes,
                None => {
                    self.lifecycle = MountedLifecycle::Incomplete;
                    return Err(MountedError::Indeterminate);
                }
            };
        self.sync_node_state(node);
        if let Err(error) = self.finalize_deleted_pending(node) {
            self.lifecycle = MountedLifecycle::Incomplete;
            return Err(error);
        }
        self.reclaim_node(node);
        Ok(())
    }

    fn directory_is_empty(&mut self, node: MountedNodeId) -> Result<bool, MountedError> {
        let (base, changes) = match &self.nodes.get(&node).ok_or(MountedError::NotFound)?.content {
            NodeContent::Directory { base, changes } => (*base, changes.clone()),
            _ => return Err(MountedError::NotDirectory),
        };
        if changes.values().any(Option::is_some) {
            return Ok(false);
        }
        let Some(base) = base else {
            return Ok(true);
        };
        let mut after = None;
        loop {
            let page = directory_page_after(
                &self.engine,
                base,
                after.as_ref(),
                DIRECTORY_PAGE_ENTRIES,
                DIRECTORY_PAGE_BYTES,
                &mut NamespaceCounters::default(),
            )?;
            if page
                .entries
                .iter()
                .any(|(name, _)| !matches!(changes.get(name), Some(None)))
            {
                return Ok(false);
            }
            after = page.continuation;
            if after.is_none() {
                return Ok(true);
            }
        }
    }

    fn next_directory_entry(
        &mut self,
        cursor: &mut DirectoryCursor,
    ) -> Result<Option<MountedDirEntry>, MountedError> {
        if cursor.cookie == 0 {
            cursor.cookie = 1;
            return Ok(Some(MountedDirEntry {
                node: cursor.node,
                name: b".".to_vec(),
                kind: MountedFileType::Directory,
                next_offset: cursor.cookie,
            }));
        }
        if cursor.cookie == 1 {
            cursor.cookie = 2;
            let parent = self
                .nodes
                .get(&cursor.node)
                .ok_or(MountedError::NotFound)?
                .parent;
            return Ok(Some(MountedDirEntry {
                node: parent,
                name: b"..".to_vec(),
                kind: MountedFileType::Directory,
                next_offset: cursor.cookie,
            }));
        }
        loop {
            self.fill_directory_base(cursor)?;
            let change = match &self
                .nodes
                .get(&cursor.node)
                .ok_or(MountedError::NotFound)?
                .content
            {
                NodeContent::Directory { changes, .. } => changes
                    .range((
                        cursor.scan_after.as_ref().map_or(Unbounded, Excluded),
                        Unbounded,
                    ))
                    .next()
                    .map(|(name, node)| (name.clone(), *node)),
                _ => return Err(MountedError::NotDirectory),
            };
            let base = cursor.base.front().cloned();
            let (name, desired) = match (base, change) {
                (None, None) => return Ok(None),
                (Some((name, inode)), None) => {
                    cursor.base.pop_front();
                    let node = self.ensure_canonical_node(inode, cursor.node)?;
                    (name, Some(node))
                }
                (None, Some(change)) => change,
                (Some((base_name, inode)), Some((change_name, desired))) => {
                    match base_name.cmp(&change_name) {
                        Ordering::Less => {
                            cursor.base.pop_front();
                            let node = self.ensure_canonical_node(inode, cursor.node)?;
                            (base_name, Some(node))
                        }
                        Ordering::Equal => {
                            cursor.base.pop_front();
                            (change_name, desired)
                        }
                        Ordering::Greater => (change_name, desired),
                    }
                }
            };
            cursor.scan_after = Some(name.clone());
            let Some(node) = desired else {
                continue;
            };
            let kind = self.nodes.get(&node).ok_or(MountedError::Corrupt)?.kind;
            cursor.cookie = cursor
                .cookie
                .checked_add(1)
                .ok_or(MountedError::ResourceExhausted)?;
            return Ok(Some(MountedDirEntry {
                node,
                name: name.as_bytes().to_vec(),
                kind,
                next_offset: cursor.cookie,
            }));
        }
    }

    fn fill_directory_base(&mut self, cursor: &mut DirectoryCursor) -> Result<(), MountedError> {
        if !cursor.base.is_empty() || cursor.base_done {
            return Ok(());
        }
        let base = match &self
            .nodes
            .get(&cursor.node)
            .ok_or(MountedError::NotFound)?
            .content
        {
            NodeContent::Directory { base, .. } => *base,
            _ => return Err(MountedError::NotDirectory),
        };
        let Some(base) = base else {
            cursor.base_done = true;
            return Ok(());
        };
        let page = directory_page_after(
            &self.engine,
            base,
            cursor.base_after.as_ref(),
            DIRECTORY_PAGE_ENTRIES,
            DIRECTORY_PAGE_BYTES,
            &mut NamespaceCounters::default(),
        )?;
        cursor.base.extend(page.entries);
        cursor.base_after = page.continuation;
        cursor.base_done = cursor.base_after.is_none();
        Ok(())
    }

    fn load_canonical_node(
        &mut self,
        inode: InodeId,
        mut id: MountedNodeId,
    ) -> Result<MountedNodeId, MountedError> {
        if let Some(existing) = self.by_inode.get(&inode) {
            id = *existing;
            if self.nodes.contains_key(&id) {
                return Ok(id);
            }
        }
        if self.nodes.len() == MAX_MOUNTED_NODES {
            return Err(MountedError::ResourceExhausted);
        }
        if !self.by_inode.contains_key(&inode) && self.by_inode.len() == MAX_MOUNTED_NODES {
            let mut evicted = false;
            while let Some(candidate) = self.reclaimable_inode_mappings.pop_first() {
                let reclaimable = self
                    .by_inode
                    .get(&candidate)
                    .is_some_and(|node| !self.nodes.contains_key(node));
                if reclaimable {
                    self.by_inode.remove(&candidate);
                    evicted = true;
                    break;
                }
            }
            if !evicted {
                return Err(MountedError::ResourceExhausted);
            }
        }
        let mut counters = InodeTableCounters::default();
        let record_id = inode_table_lookup(
            &self.engine,
            InodeTableRoot(self.namespace.inode_table_root),
            inode,
            &mut counters,
        )?
        .ok_or(MountedError::Corrupt)?;
        let record = self
            .engine
            .with_authenticated_canonical(record_id, decode_inode_record)?;
        record.validate(id == ROOT_NODE)?;
        let metadata = read_portable_metadata(&self.engine, record)?;
        let content = match record.kind {
            InodeKind::RegularFile => {
                let mut rope = RopeCounters::default();
                let plan = Arc::new(read_plan(
                    &self.engine,
                    FileStateRoot(record.content_root),
                    &mut rope,
                )?);
                NodeContent::File {
                    base: Some(FileStateRoot(record.content_root)),
                    base_visible_len: plan.logical_len(),
                    logical_len: plan.logical_len(),
                    ranges: BTreeMap::new(),
                    plan: Some(plan),
                }
            }
            InodeKind::Directory => NodeContent::Directory {
                base: Some(DirectoryStateRoot(record.content_root)),
                changes: BTreeMap::new(),
            },
            InodeKind::Symlink => NodeContent::Symlink {
                target: self
                    .engine
                    .with_authenticated_canonical(record.content_root, decode_symlink)?
                    .target,
            },
        };
        self.nodes.insert(
            id,
            MountedNode {
                canonical: Some(inode),
                record: Some(record),
                kind: record.kind.into(),
                mode: metadata.permission_mode,
                mtime_seconds: metadata.mtime_seconds,
                mtime_nanoseconds: metadata.mtime_nanoseconds,
                namespace_refs: record.namespace_ref_count,
                parent: ROOT_NODE,
                lookup_refs: 0,
                open_refs: 0,
                deleted: false,
                dirty_content: false,
                dirty_metadata: false,
                dirty_links: false,
                directory_mtime_before: None,
                content,
            },
        );
        self.by_inode.insert(inode, id);
        self.observe_resources()?;
        Ok(id)
    }

    fn ensure_canonical_node(
        &mut self,
        inode: InodeId,
        parent: MountedNodeId,
    ) -> Result<MountedNodeId, MountedError> {
        if let Some(id) = self.by_inode.get(&inode).copied() {
            if !self.nodes.contains_key(&id) {
                let id = self.load_canonical_node(inode, id)?;
                if self
                    .nodes
                    .get(&id)
                    .is_some_and(|node| node.kind == MountedFileType::Directory)
                {
                    self.nodes.get_mut(&id).ok_or(MountedError::Corrupt)?.parent = parent;
                }
                return Ok(id);
            }
            if self
                .nodes
                .get(&id)
                .is_some_and(|node| node.kind == MountedFileType::Directory)
            {
                self.nodes.get_mut(&id).ok_or(MountedError::Corrupt)?.parent = parent;
            }
            return Ok(id);
        }
        let id = self.allocate_node()?;
        let id = self.load_canonical_node(inode, id)?;
        if self
            .nodes
            .get(&id)
            .is_some_and(|node| node.kind == MountedFileType::Directory)
        {
            self.nodes.get_mut(&id).ok_or(MountedError::Corrupt)?.parent = parent;
        }
        Ok(id)
    }

    fn allocate_node(&mut self) -> Result<MountedNodeId, MountedError> {
        let id = MountedNodeId(self.next_node);
        self.next_node = self
            .next_node
            .checked_add(1)
            .ok_or(MountedError::ResourceExhausted)?;
        Ok(id)
    }

    fn allocate_handle(&mut self) -> Result<MountedHandleId, MountedError> {
        let id = MountedHandleId(self.next_handle);
        self.next_handle = self
            .next_handle
            .checked_add(1)
            .ok_or(MountedError::TooManyOpenFiles)?;
        Ok(id)
    }

    fn require_file_handle(
        &self,
        node: MountedNodeId,
        handle: MountedHandleId,
    ) -> Result<(), MountedError> {
        match self.handles.get(&handle) {
            Some(Handle::File(actual)) if *actual == node => Ok(()),
            _ => Err(MountedError::InvalidHandle),
        }
    }

    fn require_handle(&self, handle: MountedHandleId) -> Result<(), MountedError> {
        self.handles
            .contains_key(&handle)
            .then_some(())
            .ok_or(MountedError::InvalidHandle)
    }

    fn reclaim_node(&mut self, node: MountedNodeId) {
        if node == ROOT_NODE {
            return;
        }
        let reclaim = self
            .nodes
            .get(&node)
            .is_some_and(|entry| entry.lookup_refs == 0 && entry.open_refs == 0 && !entry.dirty());
        if reclaim {
            self.dirty_nodes.remove(&node);
            self.pending_nodes.remove(&node);
            if let Some(removed) = self.nodes.remove(&node) {
                if let NodeContent::Directory { changes, .. } = &removed.content {
                    self.directory_changes = self.directory_changes.saturating_sub(changes.len());
                }
                if let Some(inode) = removed.canonical {
                    if self.by_inode.get(&inode) == Some(&node) {
                        self.reclaimable_inode_mappings.insert(inode);
                    }
                }
            }
        }
    }

    fn finalize_deleted_pending(&mut self, node: MountedNodeId) -> Result<(), MountedError> {
        let should_clear = self.nodes.get(&node).is_some_and(|entry| {
            entry.deleted && entry.canonical.is_none() && entry.open_refs == 0
        });
        if !should_clear {
            return Ok(());
        }
        self.drain_node_file_ranges(node)?;
        let entry = self.nodes.get_mut(&node).ok_or(MountedError::Corrupt)?;
        entry.dirty_content = false;
        entry.dirty_metadata = false;
        entry.dirty_links = false;
        entry.directory_mtime_before = None;
        self.sync_node_state(node);
        self.normalize_spool()
    }

    fn drain_node_file_ranges(&mut self, node: MountedNodeId) -> Result<(), MountedError> {
        let (count, bytes) = match &self.nodes.get(&node).ok_or(MountedError::Corrupt)?.content {
            NodeContent::File { ranges, .. } => (
                ranges.len(),
                ranges.iter().try_fold(0_u64, |total, (start, range)| {
                    total
                        .checked_add(range.end - *start)
                        .ok_or(MountedError::Indeterminate)
                })?,
            ),
            _ => return Ok(()),
        };
        let live_ranges = self
            .live_ranges
            .checked_sub(count)
            .ok_or(MountedError::Indeterminate)?;
        let spool_live = self
            .spool
            .live
            .checked_sub(bytes)
            .ok_or(MountedError::Indeterminate)?;
        let entry = self.nodes.get_mut(&node).ok_or(MountedError::Corrupt)?;
        let NodeContent::File { ranges, plan, .. } = &mut entry.content else {
            return Err(MountedError::Indeterminate);
        };
        ranges.clear();
        *plan = None;
        self.live_ranges = live_ranges;
        self.spool.live = spool_live;
        Ok(())
    }

    fn normalize_spool(&mut self) -> Result<(), MountedError> {
        if self.spool.live == 0 {
            self.reset_spool_if_unused()
        } else {
            self.compact_spool_if_needed(0)
        }
    }

    fn normalize_spool_during_checkpoint(&mut self) -> Result<(), MountedError> {
        if self.spool.live == 0 {
            self.reset_spool_if_unused()
        } else if self.spool_needs_compaction(0)? {
            self.compact_spool_inner()
        } else {
            Ok(())
        }
    }

    fn reset_spool_if_unused(&mut self) -> Result<(), MountedError> {
        if self.spool.live == 0 && self.spool.reset()? {
            self.counters.spool_resets += 1;
        }
        Ok(())
    }

    fn compact_spool_if_needed(&mut self, additional_live: u64) -> Result<(), MountedError> {
        if self.spool_needs_compaction(additional_live)? {
            self.compact_spool()?;
        }
        Ok(())
    }

    fn spool_needs_compaction(&self, additional_live: u64) -> Result<bool, MountedError> {
        let projected_physical = self
            .spool
            .appended
            .checked_add(additional_live)
            .ok_or(MountedError::NoSpace)?;
        let projected_live = self
            .spool
            .live
            .checked_add(additional_live)
            .ok_or(MountedError::NoSpace)?;
        let steady_limit = projected_live
            .checked_mul(2)
            .and_then(|value| value.checked_add(SPOOL_COMPACTION_SLACK_BYTES))
            .ok_or(MountedError::NoSpace)?;
        Ok(projected_physical > SPOOL_QUOTA_BYTES || projected_physical > steady_limit)
    }

    fn compact_spool(&mut self) -> Result<(), MountedError> {
        let _reservation = self.budget.try_reserve(
            MAX_DIRTY_RANGES
                .checked_mul(std::mem::size_of::<SpoolRangeLocation>())
                .and_then(|bytes| bytes.checked_add(64 * 1024))
                .ok_or(MountedError::ResourceExhausted)?,
        )?;
        self.compact_spool_inner()
    }

    fn compact_spool_inner(&mut self) -> Result<(), MountedError> {
        if self.spool.appended == self.spool.live {
            return Ok(());
        }
        let mut locations = Vec::with_capacity(self.live_ranges);
        for (node, entry) in &self.nodes {
            if let NodeContent::File { ranges, .. } = &entry.content {
                for (start, range) in ranges {
                    locations.push(SpoolRangeLocation {
                        node: *node,
                        start: *start,
                        old_offset: range.spool_offset,
                        len: range.end - *start,
                    });
                }
            }
        }
        let live = locations.iter().try_fold(0_u64, |total, range| {
            total
                .checked_add(range.len)
                .ok_or(MountedError::Indeterminate)
        })?;
        if live != self.spool.live || locations.len() != self.live_ranges {
            return Err(MountedError::Indeterminate);
        }
        if live == 0 {
            return self.reset_spool_if_unused();
        }
        let compact = Spool::compaction_path(&self.spool.path);
        if compact.exists() {
            return Err(MountedError::Corrupt);
        }
        let result = (|| {
            let mut output = OpenOptions::new()
                .create_new(true)
                .read(true)
                .write(true)
                .open(&compact)?;
            output.write_all(&self.spool.marker)?;
            let input = self.spool.file.as_mut().ok_or(MountedError::Corrupt)?;
            let mut buffer = [0_u8; 64 * 1024];
            let mut next = SPOOL_MARKER_BYTES;
            let mut offsets = Vec::with_capacity(locations.len());
            for location in &locations {
                input.seek(SeekFrom::Start(location.old_offset))?;
                offsets.push(next);
                let mut remaining = location.len;
                while remaining != 0 {
                    let count = buffer.len().min(remaining as usize);
                    input.read_exact(&mut buffer[..count])?;
                    output.write_all(&buffer[..count])?;
                    remaining -= count as u64;
                    next = next
                        .checked_add(count as u64)
                        .ok_or(MountedError::Indeterminate)?;
                }
            }
            output.sync_data()?;
            std::fs::rename(&compact, &self.spool.path)?;
            let old = self.spool.file.replace(output);
            drop(old);
            for (location, offset) in locations.iter().zip(offsets) {
                let entry = self
                    .nodes
                    .get_mut(&location.node)
                    .ok_or(MountedError::Indeterminate)?;
                let NodeContent::File { ranges, .. } = &mut entry.content else {
                    return Err(MountedError::Indeterminate);
                };
                ranges
                    .get_mut(&location.start)
                    .ok_or(MountedError::Indeterminate)?
                    .spool_offset = offset;
            }
            self.spool.appended = live;
            self.counters.spool_compactions += 1;
            Ok(())
        })();
        if result.is_err() && compact.exists() {
            let _ = std::fs::remove_file(compact);
        }
        result
    }

    fn sync_node_state(&mut self, id: MountedNodeId) {
        let Some(node) = self.nodes.get(&id) else {
            self.dirty_nodes.remove(&id);
            self.pending_nodes.remove(&id);
            return;
        };
        if node.dirty() {
            self.dirty_nodes.insert(id);
        } else {
            self.dirty_nodes.remove(&id);
        }
        if node.pending() {
            self.pending_nodes.insert(id);
        } else {
            self.pending_nodes.remove(&id);
        }
    }

    fn preflight_dirty(
        &self,
        nodes: &[MountedNodeId],
        additional: usize,
    ) -> Result<(), MountedError> {
        let mut newly_dirty = additional;
        for (index, node) in nodes.iter().enumerate() {
            let entry = self.nodes.get(node).ok_or(MountedError::NotFound)?;
            if entry.record.is_some()
                && !entry.dirty_metadata
                && self.checkpoint_metadata_bytes(entry)? > MAX_CHECKPOINT_METADATA_BYTES
            {
                return Err(MountedError::ResourceExhausted);
            }
            if !self.dirty_nodes.contains(node) && !nodes[..index].contains(node) {
                newly_dirty = newly_dirty
                    .checked_add(1)
                    .ok_or(MountedError::ResourceExhausted)?;
            }
        }
        if self
            .dirty_nodes
            .len()
            .checked_add(newly_dirty)
            .is_none_or(|count| count > MAX_DIRTY_NODES)
        {
            return Err(MountedError::ResourceExhausted);
        }
        Ok(())
    }

    fn checkpoint_metadata_bytes(&self, node: &MountedNode) -> Result<usize, MountedError> {
        let Some(record) = node.record else {
            return Ok(0);
        };
        let mut bytes = 0_usize;
        visit_metadata_entries(&self.engine, record.metadata_root, |entries| {
            for entry in entries {
                bytes = bytes.saturating_add(
                    std::mem::size_of::<MetadataEntryV1>()
                        + entry.key.domain.len()
                        + entry.key.key.len()
                        + 64,
                );
            }
            Ok(())
        })?;
        Ok(bytes)
    }

    fn preflight_handle(&self, directory: bool) -> Result<(), MountedError> {
        if self.handles.len() == MAX_HANDLES
            || directory && self.directory_cursors == MAX_DIRECTORY_CURSORS
            || self.next_handle.checked_add(1).is_none()
        {
            return Err(MountedError::TooManyOpenFiles);
        }
        Ok(())
    }

    fn preflight_logical_file(&self, old: u64, new: u64) -> Result<u64, MountedError> {
        if new > MAX_LOGICAL_FILE_BYTES {
            return Err(MountedError::NoSpace);
        }
        let projected = self
            .logical_workspace_bytes
            .checked_sub(old)
            .and_then(|value| value.checked_add(new))
            .ok_or(MountedError::Indeterminate)?;
        if projected > MAX_LOGICAL_WORKSPACE_BYTES {
            return Err(MountedError::NoSpace);
        }
        Ok(projected)
    }

    fn has_dirty_state(&self) -> bool {
        !self.dirty_nodes.is_empty()
    }

    fn observe_request(&mut self, bytes: usize) -> Result<(), MountedError> {
        self.counters.largest_request_bytes = self.counters.largest_request_bytes.max(bytes as u64);
        let (current, high) = self.budget.observation()?;
        self.counters.operation_q_current_bytes = current as u64;
        self.counters.operation_q_high_water_bytes = high as u64;
        Ok(())
    }

    fn observe_resources(&mut self) -> Result<(), MountedError> {
        self.counters.lookup_refs = self.lookup_refs;
        self.counters.lookup_refs_high_water = self
            .counters
            .lookup_refs_high_water
            .max(self.counters.lookup_refs);
        self.counters.live_nodes = self.nodes.len() as u64;
        self.counters.live_nodes_high_water = self
            .counters
            .live_nodes_high_water
            .max(self.counters.live_nodes);
        self.counters.open_handles = self.handles.len() as u64;
        self.counters.open_handles_high_water = self
            .counters
            .open_handles_high_water
            .max(self.counters.open_handles);
        self.counters.pending_nodes = self.pending_nodes.len() as u64;
        self.counters.pending_nodes_high_water = self
            .counters
            .pending_nodes_high_water
            .max(self.counters.pending_nodes);
        self.counters.dirty_nodes = self.dirty_nodes.len() as u64;
        self.counters.dirty_nodes_high_water = self
            .counters
            .dirty_nodes_high_water
            .max(self.counters.dirty_nodes);
        self.counters.dirty_ranges = self.live_ranges as u64;
        self.counters.dirty_ranges_high_water = self
            .counters
            .dirty_ranges_high_water
            .max(self.counters.dirty_ranges);
        self.counters.directory_cursors = self.directory_cursors as u64;
        self.counters.directory_changes = self.directory_changes as u64;
        self.counters.directory_changes_high_water = self
            .counters
            .directory_changes_high_water
            .max(self.counters.directory_changes);
        self.counters.inode_mappings = self.by_inode.len() as u64;
        self.counters.inode_mappings_high_water = self
            .counters
            .inode_mappings_high_water
            .max(self.counters.inode_mappings);
        self.counters.logical_workspace_bytes = self.logical_workspace_bytes;
        self.counters.logical_workspace_high_water_bytes = self
            .counters
            .logical_workspace_high_water_bytes
            .max(self.logical_workspace_bytes);
        self.counters.spool_appended_bytes = self.spool.total_appended;
        self.counters.spool_live_bytes = self.spool.live;
        self.counters.spool_live_high_water_bytes = self
            .counters
            .spool_live_high_water_bytes
            .max(self.spool.live);
        self.counters.spool_dead_bytes = self.spool.appended.saturating_sub(self.spool.live);
        self.counters.spool_physical_bytes = self.spool.physical();
        self.counters.spool_physical_high_water_bytes = self
            .counters
            .spool_physical_high_water_bytes
            .max(self.counters.spool_physical_bytes);
        self.observe_request(0)
    }

    fn require_live(&self) -> Result<(), MountedError> {
        match self.lifecycle {
            MountedLifecycle::Live => Ok(()),
            MountedLifecycle::Checkpointing | MountedLifecycle::Closing => Err(MountedError::Busy),
            MountedLifecycle::Conflict | MountedLifecycle::Incomplete => {
                Err(MountedError::Indeterminate)
            }
            MountedLifecycle::Closed => Err(MountedError::StaleHandle),
        }
    }

    fn require_live_or_incomplete_read(&self) -> Result<(), MountedError> {
        match self.lifecycle {
            MountedLifecycle::Live
            | MountedLifecycle::Conflict
            | MountedLifecycle::Incomplete
            | MountedLifecycle::Closing => Ok(()),
            MountedLifecycle::Checkpointing => Err(MountedError::Busy),
            MountedLifecycle::Closed => Err(MountedError::StaleHandle),
        }
    }
}

#[derive(Clone)]
struct CheckpointNode {
    canonical: Option<InodeId>,
    record: Option<InodeRecordV1>,
    kind: MountedFileType,
    mode: u32,
    mtime_seconds: i64,
    mtime_nanoseconds: u32,
    namespace_refs: u64,
    dirty_content: bool,
    dirty_metadata: bool,
    content: NodeContent,
    metadata_entries: Option<Vec<MetadataEntryV1>>,
}

fn persist_metadata(
    publication: &mut Publication<'_>,
    node: &CheckpointNode,
) -> Result<ObjectId, MountedError> {
    let portable = PortableMetadataV1 {
        permission_mode: node.mode,
        mtime_seconds: node.mtime_seconds,
        mtime_nanoseconds: node.mtime_nanoseconds,
    };
    portable.validate(inode_kind(node.kind))?;
    let mode_key = MetadataKey::new("portable".to_owned(), b"mode".to_vec())?;
    let mtime_key = MetadataKey::new("portable".to_owned(), b"mtime".to_vec())?;
    let mode = metadata_value(
        publication,
        mode_key.clone(),
        &portable.mode_bytes(inode_kind(node.kind))?,
    )?;
    let mtime = metadata_value(publication, mtime_key.clone(), &portable.mtime_bytes()?)?;
    let mut tree = MetadataTreeBuilder::new();
    let mut inserted_mode = false;
    let mut inserted_mtime = false;
    for entry in node.metadata_entries.iter().flatten() {
        if entry.key == mode_key {
            tree.push(publication, mode.clone())?;
            inserted_mode = true;
        } else if entry.key == mtime_key {
            tree.push(publication, mtime.clone())?;
            inserted_mtime = true;
        } else {
            tree.push(publication, entry.clone())?;
        }
    }
    if node.metadata_entries.is_none() {
        tree.push(publication, mode)?;
        tree.push(publication, mtime)?;
    } else if !inserted_mode || !inserted_mtime {
        return Err(MountedError::Corrupt);
    }
    Ok(tree.finish(publication)?)
}

fn metadata_value(
    publication: &mut Publication<'_>,
    key: MetadataKey,
    value: &[u8],
) -> Result<MetadataEntryV1, MountedError> {
    let (root, _) = build(publication, Cursor::new(value))?;
    Ok(MetadataEntryV1 {
        key,
        value_file_root: root.0,
    })
}

fn read_portable_metadata(
    engine: &Engine,
    record: InodeRecordV1,
) -> Result<PortableMetadataV1, MountedError> {
    let mode = metadata_lookup(
        engine,
        record.metadata_root,
        &MetadataKey::new("portable".to_owned(), b"mode".to_vec())?,
    )?
    .ok_or(MountedError::Corrupt)?;
    let mtime = metadata_lookup(
        engine,
        record.metadata_root,
        &MetadataKey::new("portable".to_owned(), b"mtime".to_vec())?,
    )?
    .ok_or(MountedError::Corrupt)?;
    let mut mode_bytes = Vec::new();
    read_all_bounded(
        engine,
        FileStateRoot(mode.value_file_root),
        4,
        &mut mode_bytes,
    )?;
    let mut mtime_bytes = Vec::new();
    read_all_bounded(
        engine,
        FileStateRoot(mtime.value_file_root),
        12,
        &mut mtime_bytes,
    )?;
    if mode_bytes.len() != 4 || mtime_bytes.len() != 12 {
        return Err(MountedError::Corrupt);
    }
    let metadata = PortableMetadataV1 {
        permission_mode: u32::from_be_bytes(mode_bytes.try_into().unwrap()),
        mtime_seconds: i64::from_be_bytes(mtime_bytes[..8].try_into().unwrap()),
        mtime_nanoseconds: u32::from_be_bytes(mtime_bytes[8..].try_into().unwrap()),
    };
    metadata.validate(record.kind)?;
    Ok(metadata)
}

fn accepted_logical_bytes(engine: &Engine, inode_table: ObjectId) -> Result<u64, CoreError> {
    let mut total = 0_u64;
    visit_inode_table_entries(
        engine,
        InodeTableRoot(inode_table),
        &mut InodeTableCounters::default(),
        |entries| {
            for (_, record_id) in entries {
                let record =
                    engine.with_authenticated_canonical(*record_id, decode_inode_record)?;
                if record.kind == InodeKind::RegularFile {
                    let state = rope_state(
                        engine,
                        FileStateRoot(record.content_root),
                        &mut RopeCounters::default(),
                    )?;
                    total = total
                        .checked_add(state.logical_len)
                        .ok_or(CoreError::LengthOverflow)?;
                }
            }
            Ok(())
        },
    )?;
    Ok(total)
}

fn install_dirty_range(
    ranges: &mut BTreeMap<u64, DirtyRange>,
    start: u64,
    end: u64,
    spool_offset: u64,
) -> Result<(u64, u64), MountedError> {
    let mut removed = 0_u64;
    let mut preserved = 0_u64;
    if let Some((&key, range)) = ranges.range(..start).next_back() {
        if range.end > start {
            let range = ranges.remove(&key).ok_or(MountedError::Indeterminate)?;
            removed += range.end - key;
            ranges.insert(
                key,
                DirtyRange {
                    end: start,
                    spool_offset: range.spool_offset,
                },
            );
            preserved += start - key;
            if range.end > end {
                ranges.insert(
                    end,
                    DirtyRange {
                        end: range.end,
                        spool_offset: range.spool_offset + (end - key),
                    },
                );
                preserved += range.end - end;
            }
        }
    }
    while let Some((&key, range)) = ranges.range(start..end).next() {
        let range = range.clone();
        ranges.remove(&key);
        removed += range.end - key;
        if range.end > end {
            ranges.insert(
                end,
                DirtyRange {
                    end: range.end,
                    spool_offset: range.spool_offset + (end - key),
                },
            );
            preserved += range.end - end;
            break;
        }
    }
    ranges.insert(start, DirtyRange { end, spool_offset });
    merge_adjacent_ranges(ranges, start)?;
    Ok((removed, preserved))
}

fn merge_adjacent_ranges(
    ranges: &mut BTreeMap<u64, DirtyRange>,
    mut key: u64,
) -> Result<(), MountedError> {
    if let Some((&previous, range)) = ranges.range(..key).next_back() {
        let current = ranges.get(&key).ok_or(MountedError::Indeterminate)?;
        if range.end == key && range.spool_offset + (range.end - previous) == current.spool_offset {
            let end = current.end;
            ranges.remove(&key);
            ranges
                .get_mut(&previous)
                .ok_or(MountedError::Indeterminate)?
                .end = end;
            key = previous;
        }
    }
    let current = ranges.get(&key).ok_or(MountedError::Indeterminate)?.clone();
    if let Some((&next, range)) = ranges.range((Excluded(key), Unbounded)).next() {
        if current.end == next && current.spool_offset + (current.end - key) == range.spool_offset {
            let end = range.end;
            ranges.remove(&next);
            ranges.get_mut(&key).ok_or(MountedError::Indeterminate)?.end = end;
        }
    }
    Ok(())
}

fn truncate_dirty_ranges(ranges: &mut BTreeMap<u64, DirtyRange>, length: u64) -> u64 {
    let mut removed = 0;
    if let Some((&start, range)) = ranges.range(..length).next_back() {
        if range.end > length {
            removed += range.end - length;
            if let Some(range) = ranges.get_mut(&start) {
                range.end = length;
            }
        }
    }
    while let Some((&start, range)) = ranges.range(length..).next() {
        removed += range.end - start;
        ranges.remove(&start);
    }
    removed
}

fn inode_kind(kind: MountedFileType) -> InodeKind {
    match kind {
        MountedFileType::RegularFile => InodeKind::RegularFile,
        MountedFileType::Directory => InodeKind::Directory,
        MountedFileType::Symlink => InodeKind::Symlink,
    }
}

fn now_timestamp() -> Result<(i64, u32), MountedError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| MountedError::Indeterminate)?;
    Ok((
        i64::try_from(now.as_secs()).map_err(|_| MountedError::Indeterminate)?,
        now.subsec_nanos(),
    ))
}

fn startup(step: &'static str, error: impl std::fmt::Debug) -> MountedError {
    MountedError::Startup(step, format!("{error:?}"))
}

fn mounted_vfs_error(error: VfsError) -> MountedError {
    match error {
        VfsError::Core(error) => error.into(),
        VfsError::Engine(error) => error.into(),
        VfsError::Io(error) => MountedError::Io(error),
        VfsError::WorkspaceBusy => MountedError::Busy,
        VfsError::CommittedCleanup { .. } => MountedError::CommittedCleanup,
        VfsError::Indeterminate | VfsError::IncompleteDerived => MountedError::Indeterminate,
        VfsError::Driver(_)
        | VfsError::ExternalDirtyConflict
        | VfsError::ExternalHardLinkBoundary
        | VfsError::NativeProtected
        | VfsError::InvalidState => MountedError::Corrupt,
    }
}

fn merge_rope(target: &mut RopeCounters, source: RopeCounters) -> Result<(), MountedError> {
    target.payload_bytes_read = target
        .payload_bytes_read
        .checked_add(source.payload_bytes_read)
        .ok_or(MountedError::ResourceExhausted)?;
    target.nodes_read = target
        .nodes_read
        .checked_add(source.nodes_read)
        .ok_or(MountedError::ResourceExhausted)?;
    Ok(())
}

struct ZeroReader(u64);

impl Read for ZeroReader {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let count = output.len().min(self.0 as usize);
        output[..count].fill(0);
        self.0 -= count as u64;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(label: &str) -> (PathBuf, PathBuf, PathBuf) {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "layerfs-mounted-{label}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).unwrap();
        (
            directory.join("store.sqlite"),
            directory.join("mount.spool"),
            directory,
        )
    }

    #[test]
    fn pending_write_overlap_unlink_open_cancels_without_publication() {
        let (store, spool, directory) = paths("cancel");
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x91; 32],
        )
        .unwrap();
        mounted.reset_engine_counters().unwrap();
        let (attr, handle) = mounted.create_file(ROOT_NODE, b"file", 0o644).unwrap();
        mounted.write(attr.node, handle, 0, b"abcdef").unwrap();
        mounted.write(attr.node, handle, 2, b"XY").unwrap();
        assert_eq!(mounted.read(attr.node, handle, 0, 16).unwrap(), b"abXYef");
        mounted.truncate(attr.node, 4).unwrap();
        assert_eq!(mounted.read(attr.node, handle, 0, 16).unwrap(), b"abXY");
        mounted.unlink(ROOT_NODE, b"file").unwrap();
        assert_eq!(mounted.read(attr.node, handle, 0, 16).unwrap(), b"abXY");
        mounted.release(handle).unwrap();
        mounted.forget(attr.node, 1);
        let counters = mounted.counters().unwrap();
        assert_eq!(counters.pending_nodes, 0);
        assert_eq!(counters.dirty_nodes, 0);
        assert_eq!(counters.dirty_ranges, 0);
        assert_eq!(counters.spool_live_bytes, 0);
        assert_eq!(counters.spool_physical_bytes, 0);
        assert_eq!(counters.spool_appended_bytes, 8);
        assert_eq!(counters.spool_live_high_water_bytes, 6);
        assert_eq!(counters.spool_physical_high_water_bytes, 8);
        assert_eq!(counters.logical_workspace_high_water_bytes, 6);
        assert_eq!(counters.live_nodes_high_water, 2);
        assert_eq!(counters.open_handles_high_water, 1);
        assert_eq!(counters.pending_nodes_high_water, 1);
        assert_eq!(counters.dirty_nodes_high_water, 2);
        let engine = mounted.engine_counters().unwrap();
        assert_eq!(engine.transactions_started, 0);
        assert_eq!(engine.publication_commits, 0);
        assert_eq!(engine.objects_created, 0);
        mounted.shutdown().unwrap();
        drop(mounted);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn existing_non_main_ref_remounts_and_diverges_from_main() {
        let (store, spool, directory) = paths("branch-scope");
        let main_at_fork = {
            let mut mounted = MountedWorkspace::open(
                &store,
                "main",
                IntegrityMode::TrustedLocalDev,
                spool.clone(),
                [0x90; 32],
            )
            .unwrap();
            let (file, handle) = mounted.create_file(ROOT_NODE, b"file", 0o644).unwrap();
            mounted.write(file.node, handle, 0, b"main-one").unwrap();
            let first = mounted.fsync().unwrap();
            let branch = mounted.fork_ref("branch").unwrap();
            assert_eq!(branch.root, first.root);
            mounted.write(file.node, handle, 0, b"main-two").unwrap();
            mounted.truncate(file.node, 8).unwrap();
            mounted.fsync().unwrap();
            mounted.release(handle).unwrap();
            mounted.shutdown().unwrap();
            first
        };
        let branch_after = {
            let mut mounted = MountedWorkspace::open(
                &store,
                "branch",
                IntegrityMode::TrustedLocalDev,
                directory.join("branch.spool"),
                [0x90; 32],
            )
            .unwrap();
            assert_eq!(mounted.accepted().root, main_at_fork.root);
            let file = mounted.lookup_child(ROOT_NODE, b"file").unwrap();
            let handle = mounted.open_file(file.node, false).unwrap();
            assert_eq!(mounted.read(file.node, handle, 0, 32).unwrap(), b"main-one");
            mounted.write(file.node, handle, 0, b"branch!!").unwrap();
            let state = mounted.fsync().unwrap();
            mounted.release(handle).unwrap();
            mounted.shutdown().unwrap();
            state
        };
        let mut main = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x90; 32],
        )
        .unwrap();
        let file = main.lookup_child(ROOT_NODE, b"file").unwrap();
        let handle = main.open_file(file.node, false).unwrap();
        assert_eq!(main.read(file.node, handle, 0, 32).unwrap(), b"main-two");
        main.release(handle).unwrap();
        main.shutdown().unwrap();
        drop(main);
        let branch = MountedWorkspace::open(
            &store,
            "branch",
            IntegrityMode::TrustedLocalDev,
            directory.join("branch-reopen.spool"),
            [0x90; 32],
        )
        .unwrap();
        assert_eq!(branch.accepted(), &branch_after);
        drop(branch);
        assert!(matches!(
            MountedWorkspace::open(
                &store,
                "missing",
                IntegrityMode::TrustedLocalDev,
                directory.join("missing.spool"),
                [0x90; 32]
            ),
            Err(MountedError::NotFound)
        ));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn closing_budget_cancels_waiters_and_drains_existing_reservations() {
        let budget = Arc::new(ByteBudget::new(4));
        let held = budget.reserve(4).unwrap();
        let waiter_budget = budget.clone();
        let waiter = std::thread::spawn(move || waiter_budget.reserve(1));
        let closer_budget = budget.clone();
        let closer = std::thread::spawn(move || closer_budget.close_and_wait());
        assert!(matches!(waiter.join().unwrap(), Err(MountedError::Busy)));
        drop(held);
        closer.join().unwrap().unwrap();
        assert_eq!(budget.observation().unwrap().0, 0);
        assert!(matches!(budget.reserve(1), Err(MountedError::Busy)));
        assert!(matches!(budget.try_reserve(1), Err(MountedError::Busy)));
    }

    #[test]
    fn mounted_splice_reuses_direct_range_replace_and_requires_remount() {
        let (store, spool, directory) = paths("splice");
        let original = (0..256 * 1024)
            .map(|index| (index as u8).wrapping_mul(13))
            .collect::<Vec<_>>();
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool.clone(),
            [0x8f; 32],
        )
        .unwrap();
        let (file, handle) = mounted.create_file(ROOT_NODE, b"file", 0o644).unwrap();
        mounted.write(file.node, handle, 0, &original).unwrap();
        mounted.fsync().unwrap();
        mounted.release(handle).unwrap();
        let receipt = mounted
            .splice_path(
                &CanonicalPath::from_bytes(b"file").unwrap(),
                64 * 1024,
                0,
                &[0x5a; 4096],
            )
            .unwrap();
        assert!(receipt.remount_required);
        assert_eq!(mounted.lifecycle(), MountedLifecycle::Closed);
        assert_eq!(receipt.counters.rope.cdc_bytes_scanned, 4096);
        assert_eq!(receipt.counters.content_payload_bytes_read(), Some(0));
        assert!(receipt.counters.content_payload_bytes_written() <= Some(4096));
        assert_eq!(receipt.counters.namespace.nodes_created, 0);
        assert_eq!(receipt.counters.operation_q_terminal_bytes, 0);
        assert_eq!(
            receipt.counters.operation_q_high_water_bytes,
            MAX_OPERATION_Q_BYTES as u64
        );
        assert_eq!(mounted.counters().unwrap().splices, 1);
        assert!(matches!(
            mounted.lookup_child(ROOT_NODE, b"file"),
            Err(MountedError::StaleHandle)
        ));
        drop(mounted);

        let engine = Engine::open_with_mode(&store, IntegrityMode::TrustedLocalDev).unwrap();
        let namespace = crate::resolver::namespace(&engine, receipt.before.root).unwrap();
        let (_, record) = crate::resolver::resolve(
            &engine,
            namespace,
            &CanonicalPath::from_bytes(b"file").unwrap(),
            &mut OperationCounters::default(),
        )
        .unwrap();
        let mut rope = RopeCounters::default();
        let plan = read_plan(&engine, FileStateRoot(record.content_root), &mut rope).unwrap();
        let mut old = Vec::new();
        read_range_with_plan(&engine, &plan, 0..plan.logical_len(), &mut old).unwrap();
        assert_eq!(old, original);
        drop(engine);

        let mut expected = original;
        expected.splice(64 * 1024..64 * 1024, [0x5a; 4096]);
        let mut reopened = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x8f; 32],
        )
        .unwrap();
        assert_eq!(reopened.accepted(), &receipt.after);
        let file = reopened.lookup_child(ROOT_NODE, b"file").unwrap();
        let handle = reopened.open_file(file.node, false).unwrap();
        assert_eq!(
            reopened.read(file.node, handle, 0, expected.len()).unwrap(),
            expected
        );
        reopened.release(handle).unwrap();
        reopened.shutdown().unwrap();
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn mounted_splice_conflict_and_post_visibility_uncertainty_fail_closed() {
        let (store, spool, directory) = paths("splice-fail-closed");
        let mut initial = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool.clone(),
            [0x8f; 32],
        )
        .unwrap();
        let (file, handle) = initial.create_file(ROOT_NODE, b"file", 0o644).unwrap();
        initial.write(file.node, handle, 0, b"original").unwrap();
        initial.fsync().unwrap();
        initial.release(handle).unwrap();
        initial.shutdown().unwrap();
        drop(initial);

        let mut winner = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool.clone(),
            [0x8f; 32],
        )
        .unwrap();
        let mut loser = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            directory.join("loser.spool"),
            [0x8f; 32],
        )
        .unwrap();
        winner
            .splice_path(
                &CanonicalPath::from_bytes(b"file").unwrap(),
                0,
                8,
                b"winner",
            )
            .unwrap();
        assert!(matches!(
            loser.splice_path(&CanonicalPath::from_bytes(b"file").unwrap(), 0, 8, b"loser"),
            Err(MountedError::Conflict)
        ));
        assert_eq!(loser.lifecycle(), MountedLifecycle::Conflict);
        assert!(matches!(
            loser.mknod_file(ROOT_NODE, b"late", 0o644),
            Err(MountedError::Indeterminate)
        ));
        drop(winner);
        drop(loser);

        let mut uncertain = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x8f; 32],
        )
        .unwrap();
        uncertain.splice_post_visibility_uncertainty = true;
        assert!(matches!(
            uncertain.splice_path(&CanonicalPath::from_bytes(b"file").unwrap(), 0, 6, b"final"),
            Err(MountedError::Indeterminate)
        ));
        assert_eq!(uncertain.lifecycle(), MountedLifecycle::Incomplete);
        assert!(matches!(
            uncertain.mknod_file(ROOT_NODE, b"late", 0o644),
            Err(MountedError::Indeterminate)
        ));
        drop(uncertain);

        let mut reopened = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            directory.join("reopened.spool"),
            [0x8f; 32],
        )
        .unwrap();
        let file = reopened.lookup_child(ROOT_NODE, b"file").unwrap();
        let handle = reopened.open_file(file.node, false).unwrap();
        assert_eq!(reopened.read(file.node, handle, 0, 32).unwrap(), b"final");
        reopened.release(handle).unwrap();
        reopened.shutdown().unwrap();
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn dirty_spool_compacts_inside_the_steady_physical_bound() {
        let (store, spool, directory) = paths("spool-compact");
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x8e; 32],
        )
        .unwrap();
        let (file, handle) = mounted.create_file(ROOT_NODE, b"file", 0o644).unwrap();
        let mut bytes = vec![0_u8; MAX_REQUEST_BYTES];
        for value in 0..70_u8 {
            bytes.fill(value);
            mounted.write(file.node, handle, 0, &bytes).unwrap();
            let counters = mounted.counters().unwrap();
            assert!(
                counters.spool_physical_bytes
                    <= counters.spool_live_bytes * 2 + SPOOL_COMPACTION_SLACK_BYTES
            );
        }
        let counters = mounted.counters().unwrap();
        assert!(counters.spool_compactions >= 1);
        assert_eq!(counters.spool_live_bytes, MAX_REQUEST_BYTES as u64);
        assert_eq!(
            mounted
                .read(file.node, handle, 0, MAX_REQUEST_BYTES)
                .unwrap(),
            bytes
        );
        mounted.unlink(ROOT_NODE, b"file").unwrap();
        mounted.release(handle).unwrap();
        mounted.forget(file.node, 1);
        let counters = mounted.counters().unwrap();
        assert_eq!(counters.spool_live_bytes, 0);
        assert_eq!(counters.spool_physical_bytes, 0);
        drop(mounted);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn live_byte_decreases_normalize_spool_for_removal_and_open_orphan_checkpoint() {
        let (store, spool, directory) = paths("spool-live-decrease");
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x8e; 32],
        )
        .unwrap();
        let (large, large_handle) = mounted.create_file(ROOT_NODE, b"large", 0o644).unwrap();
        let (small, small_handle) = mounted.create_file(ROOT_NODE, b"small", 0o644).unwrap();
        let chunk = vec![0x5a; MAX_REQUEST_BYTES];
        for index in 0..70_u64 {
            mounted
                .write(
                    large.node,
                    large_handle,
                    index * MAX_REQUEST_BYTES as u64,
                    &chunk,
                )
                .unwrap();
        }
        mounted.write(small.node, small_handle, 0, b"s").unwrap();
        mounted.unlink(ROOT_NODE, b"large").unwrap();
        mounted.release(large_handle).unwrap();
        mounted.forget(large.node, 1);
        let after_removal = mounted.counters().unwrap();
        assert_eq!(after_removal.spool_live_bytes, 1);
        assert!(
            after_removal.spool_physical_bytes
                <= after_removal.spool_live_bytes * 2 + SPOOL_COMPACTION_SLACK_BYTES
        );
        assert!(after_removal.spool_compactions >= 1);
        mounted.fsync().unwrap();
        mounted.release(small_handle).unwrap();

        let (orphan, orphan_handle) = mounted.create_file(ROOT_NODE, b"orphan", 0o644).unwrap();
        mounted
            .write(orphan.node, orphan_handle, 0, b"old")
            .unwrap();
        mounted.fsync().unwrap();
        mounted.unlink(ROOT_NODE, b"orphan").unwrap();
        mounted.fsync().unwrap();
        mounted.write(orphan.node, orphan_handle, 0, b"x").unwrap();
        let (checkpoint, checkpoint_handle) = mounted
            .create_file(ROOT_NODE, b"checkpoint", 0o644)
            .unwrap();
        for index in 0..70_u64 {
            mounted
                .write(
                    checkpoint.node,
                    checkpoint_handle,
                    index * MAX_REQUEST_BYTES as u64,
                    &chunk,
                )
                .unwrap();
        }
        mounted.fsync().unwrap();
        let after_checkpoint = mounted.counters().unwrap();
        assert_eq!(after_checkpoint.spool_live_bytes, 1);
        assert!(
            after_checkpoint.spool_physical_bytes
                <= after_checkpoint.spool_live_bytes * 2 + SPOOL_COMPACTION_SLACK_BYTES
        );
        assert_eq!(
            mounted.read(orphan.node, orphan_handle, 0, 8).unwrap(),
            b"xld"
        );
        mounted.release(checkpoint_handle).unwrap();
        mounted.release(orphan_handle).unwrap();
        assert_eq!(mounted.counters().unwrap().spool_physical_bytes, 0);
        mounted.shutdown().unwrap();
        drop(mounted);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn accepted_dirty_closed_unlink_drains_ranges_and_spool_after_checkpoint() {
        let (store, spool, directory) = paths("accepted-dirty-unlink");
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool.clone(),
            [0x8e; 32],
        )
        .unwrap();
        let (file, handle) = mounted.create_file(ROOT_NODE, b"file", 0o644).unwrap();
        mounted.write(file.node, handle, 0, b"accepted").unwrap();
        mounted.fsync().unwrap();
        mounted.release(handle).unwrap();
        let handle = mounted.open_file(file.node, false).unwrap();
        mounted.write(file.node, handle, 0, b"modified").unwrap();
        mounted.release(handle).unwrap();
        assert!(mounted.counters().unwrap().dirty_ranges > 0);
        assert!(mounted.counters().unwrap().spool_live_bytes > 0);
        mounted.unlink(ROOT_NODE, b"file").unwrap();
        let removed = mounted.fsync().unwrap();
        let counters = mounted.counters().unwrap();
        assert_eq!(counters.dirty_ranges, 0);
        assert_eq!(counters.spool_live_bytes, 0);
        assert_eq!(counters.spool_physical_bytes, 0);
        mounted.reset_engine_counters().unwrap();
        assert_eq!(mounted.fsync().unwrap(), removed);
        let engine = mounted.engine_counters().unwrap();
        assert_eq!(engine.transactions_started, 0);
        assert_eq!(engine.transactions_committed, 0);
        assert_eq!(engine.transactions_rolled_back, 0);
        assert_eq!(engine.publication_commits, 0);
        mounted.shutdown().unwrap();
        drop(mounted);

        let mut reopened = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x8e; 32],
        )
        .unwrap();
        assert!(matches!(
            reopened.lookup_child(ROOT_NODE, b"file"),
            Err(MountedError::NotFound)
        ));
        reopened.shutdown().unwrap();
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn dirty_closed_rename_target_drains_ranges_and_spool_after_checkpoint() {
        let (store, spool, directory) = paths("dirty-rename-target");
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool.clone(),
            [0x8e; 32],
        )
        .unwrap();
        let (source, source_handle) = mounted.create_file(ROOT_NODE, b"source", 0o644).unwrap();
        let (target, target_handle) = mounted.create_file(ROOT_NODE, b"target", 0o644).unwrap();
        mounted
            .write(source.node, source_handle, 0, b"source-bytes")
            .unwrap();
        mounted
            .write(target.node, target_handle, 0, b"target-bytes")
            .unwrap();
        mounted.fsync().unwrap();
        mounted.release(source_handle).unwrap();
        mounted.release(target_handle).unwrap();
        let target_handle = mounted.open_file(target.node, false).unwrap();
        mounted
            .write(target.node, target_handle, 0, b"dirty-target")
            .unwrap();
        mounted.release(target_handle).unwrap();
        mounted
            .rename(ROOT_NODE, b"source", ROOT_NODE, b"target", false)
            .unwrap();
        let replaced = mounted.fsync().unwrap();
        let counters = mounted.counters().unwrap();
        assert_eq!(counters.dirty_ranges, 0);
        assert_eq!(counters.spool_live_bytes, 0);
        assert_eq!(counters.spool_physical_bytes, 0);
        mounted.reset_engine_counters().unwrap();
        assert_eq!(mounted.fsync().unwrap(), replaced);
        let engine = mounted.engine_counters().unwrap();
        assert_eq!(engine.transactions_started, 0);
        assert_eq!(engine.transactions_committed, 0);
        assert_eq!(engine.transactions_rolled_back, 0);
        assert_eq!(engine.publication_commits, 0);
        mounted.shutdown().unwrap();
        drop(mounted);

        let mut reopened = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x8e; 32],
        )
        .unwrap();
        assert!(matches!(
            reopened.lookup_child(ROOT_NODE, b"source"),
            Err(MountedError::NotFound)
        ));
        let target = reopened.lookup_child(ROOT_NODE, b"target").unwrap();
        let handle = reopened.open_file(target.node, false).unwrap();
        assert_eq!(
            reopened.read(target.node, handle, 0, 32).unwrap(),
            b"source-bytes"
        );
        reopened.release(handle).unwrap();
        reopened.shutdown().unwrap();
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn mount_wide_directory_changes_and_inode_mappings_are_bounded_and_observed() {
        let (store, spool, directory) = paths("directory-change-cap");
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x8d; 32],
        )
        .unwrap();
        let evictable = mounted.mknod_file(ROOT_NODE, b"evictable", 0o644).unwrap();
        mounted.mknod_file(ROOT_NODE, b"victim", 0o644).unwrap();
        mounted.fsync().unwrap();
        let evictable_inode = mounted.nodes[&evictable.node].canonical.unwrap();
        mounted.forget(evictable.node, 1);
        assert!(!mounted.nodes.contains_key(&evictable.node));
        assert!(mounted.by_inode.contains_key(&evictable_inode));
        assert!(mounted
            .reclaimable_inode_mappings
            .contains(&evictable_inode));
        mounted.unlink(ROOT_NODE, b"victim").unwrap();
        let file = mounted.mknod_file(ROOT_NODE, b"source", 0o644).unwrap();
        for index in 2..MAX_DIRECTORY_CHANGES {
            mounted
                .link(file.node, ROOT_NODE, format!("alias-{index}").as_bytes())
                .unwrap();
        }
        let before = mounted.counters().unwrap();
        assert_eq!(before.directory_changes, MAX_DIRECTORY_CHANGES as u64);
        assert_eq!(
            before.directory_changes_high_water,
            MAX_DIRECTORY_CHANGES as u64
        );
        assert!(before.inode_mappings <= MAX_MOUNTED_NODES as u64);
        assert!(matches!(
            mounted.link(file.node, ROOT_NODE, b"one-too-many"),
            Err(MountedError::ResourceExhausted)
        ));
        assert_eq!(mounted.counters().unwrap(), before);
        let node_ids = mounted.nodes.keys().copied().collect::<HashSet<_>>();
        let by_inode = mounted.by_inode.clone();
        let dirty_nodes = mounted.dirty_nodes.clone();
        let pending_nodes = mounted.pending_nodes.clone();
        let engine_counters = mounted.engine_counters().unwrap();
        let scalar_state = (
            mounted.next_node,
            mounted.next_handle,
            mounted.live_ranges,
            mounted.directory_cursors,
            mounted.directory_changes,
            mounted.lookup_refs,
            mounted.logical_workspace_bytes,
            mounted.spool.appended,
            mounted.spool.total_appended,
            mounted.spool.live,
            mounted.spool.physical(),
        );
        let root_state = {
            let root = mounted.nodes.get(&ROOT_NODE).unwrap();
            let NodeContent::Directory { changes, .. } = &root.content else {
                panic!("root is not a directory")
            };
            (
                root.attr(ROOT_NODE),
                root.dirty_content,
                root.dirty_metadata,
                root.dirty_links,
                root.directory_mtime_before,
                changes.clone(),
            )
        };
        assert!(matches!(
            mounted.mknod_file(ROOT_NODE, b"cap-rejected-create", 0o644),
            Err(MountedError::ResourceExhausted)
        ));
        assert_eq!(
            mounted.nodes.keys().copied().collect::<HashSet<_>>(),
            node_ids
        );
        assert_eq!(mounted.by_inode, by_inode);
        assert_eq!(mounted.dirty_nodes, dirty_nodes);
        assert_eq!(mounted.pending_nodes, pending_nodes);
        assert_eq!(
            (
                mounted.next_node,
                mounted.next_handle,
                mounted.live_ranges,
                mounted.directory_cursors,
                mounted.directory_changes,
                mounted.lookup_refs,
                mounted.logical_workspace_bytes,
                mounted.spool.appended,
                mounted.spool.total_appended,
                mounted.spool.live,
                mounted.spool.physical(),
            ),
            scalar_state
        );
        let root = mounted.nodes.get(&ROOT_NODE).unwrap();
        let NodeContent::Directory { changes, .. } = &root.content else {
            panic!("root is not a directory")
        };
        assert_eq!(
            (
                root.attr(ROOT_NODE),
                root.dirty_content,
                root.dirty_metadata,
                root.dirty_links,
                root.directory_mtime_before,
                changes.clone(),
            ),
            root_state
        );
        assert_eq!(mounted.counters().unwrap(), before);
        assert_eq!(mounted.engine_counters().unwrap(), engine_counters);
        mounted.mknod_file(ROOT_NODE, b"victim", 0o644).unwrap();
        assert_eq!(
            mounted.counters().unwrap().directory_changes,
            MAX_DIRECTORY_CHANGES as u64
        );
        let mut nonce = 0_u64;
        while mounted.by_inode.len() < MAX_MOUNTED_NODES {
            let mut bytes = [0_u8; 32];
            bytes[..8].copy_from_slice(&nonce.to_be_bytes());
            mounted
                .by_inode
                .entry(InodeId(bytes))
                .or_insert(MountedNodeId(u64::MAX - nonce));
            nonce += 1;
        }
        let full_mappings = mounted.counters().unwrap();
        assert_eq!(full_mappings.inode_mappings, MAX_MOUNTED_NODES as u64);
        assert!(matches!(
            mounted.load_canonical_node(InodeId([0xff; 32]), MountedNodeId(u64::MAX / 2)),
            Err(MountedError::Corrupt)
        ));
        assert!(!mounted.by_inode.contains_key(&evictable_inode));
        let mappings = mounted.counters().unwrap();
        assert_eq!(mappings.inode_mappings, MAX_MOUNTED_NODES as u64 - 1);
        assert_eq!(mappings.inode_mappings_high_water, MAX_MOUNTED_NODES as u64);
        drop(mounted);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn replacement_rename_reclaims_directory_changes_after_checkpoint() {
        let (store, spool, directory) = paths("replacement-rename");
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x8d; 32],
        )
        .unwrap();
        let (source, source_handle) = mounted.create_file(ROOT_NODE, b"source", 0o644).unwrap();
        let (_, target_handle) = mounted.create_file(ROOT_NODE, b"target", 0o644).unwrap();
        mounted
            .write(source.node, source_handle, 0, b"same-directory")
            .unwrap();
        mounted.release(source_handle).unwrap();
        mounted.release(target_handle).unwrap();
        mounted.fsync().unwrap();
        mounted
            .rename(ROOT_NODE, b"source", ROOT_NODE, b"target", false)
            .unwrap();
        mounted.fsync().unwrap();
        assert_eq!(mounted.counters().unwrap().directory_changes, 0);
        assert!(matches!(
            mounted.lookup_child(ROOT_NODE, b"source"),
            Err(MountedError::NotFound)
        ));
        let target = mounted.lookup_child(ROOT_NODE, b"target").unwrap();
        let handle = mounted.open_file(target.node, false).unwrap();
        assert_eq!(
            mounted.read(target.node, handle, 0, 32).unwrap(),
            b"same-directory"
        );
        mounted.release(handle).unwrap();

        let left = mounted.mkdir(ROOT_NODE, b"left", 0o755).unwrap();
        let right = mounted.mkdir(ROOT_NODE, b"right", 0o755).unwrap();
        let (source, source_handle) = mounted.create_file(left.node, b"source", 0o644).unwrap();
        let (_, target_handle) = mounted.create_file(right.node, b"target", 0o644).unwrap();
        mounted
            .write(source.node, source_handle, 0, b"cross-directory")
            .unwrap();
        mounted.release(source_handle).unwrap();
        mounted.release(target_handle).unwrap();
        mounted.fsync().unwrap();
        mounted
            .rename(left.node, b"source", right.node, b"target", false)
            .unwrap();
        mounted.fsync().unwrap();
        assert_eq!(mounted.counters().unwrap().directory_changes, 0);
        assert!(matches!(
            mounted.lookup_child(left.node, b"source"),
            Err(MountedError::NotFound)
        ));
        let target = mounted.lookup_child(right.node, b"target").unwrap();
        let handle = mounted.open_file(target.node, false).unwrap();
        assert_eq!(
            mounted.read(target.node, handle, 0, 32).unwrap(),
            b"cross-directory"
        );
        mounted.release(handle).unwrap();
        mounted.shutdown().unwrap();
        drop(mounted);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn post_unmount_cache_release_is_root_only_and_logically_empty() {
        let (store, spool, directory) = paths("terminal-cache-release");
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x8d; 32],
        )
        .unwrap();
        let file = mounted.mknod_file(ROOT_NODE, b"temporary", 0o644).unwrap();
        mounted.fsync().unwrap();
        mounted.unlink(ROOT_NODE, b"temporary").unwrap();
        mounted.fsync().unwrap();
        assert!(mounted.nodes.contains_key(&file.node));
        mounted.shutdown().unwrap();
        mounted.release_kernel_cache_ownership().unwrap();
        let counters = mounted.counters().unwrap();
        assert_eq!(counters.lookup_refs, 1);
        assert_eq!(counters.live_nodes, 1);
        assert_eq!(counters.inode_mappings, 1);
        assert_eq!(counters.open_handles, 0);
        assert_eq!(counters.pending_nodes, 0);
        assert_eq!(counters.dirty_nodes, 0);
        assert_eq!(counters.dirty_ranges, 0);
        assert_eq!(counters.directory_cursors, 0);
        assert_eq!(counters.directory_changes, 0);
        assert_eq!(counters.logical_workspace_bytes, 0);
        assert_eq!(counters.spool_live_bytes, 0);
        assert_eq!(counters.spool_physical_bytes, 0);
        assert_eq!(counters.operation_q_current_bytes, 0);
        drop(mounted);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn one_checkpoint_reopens_nested_bytes_and_hard_link_identity() {
        let (store, spool, directory) = paths("checkpoint");
        let accepted = {
            let mut mounted = MountedWorkspace::open(
                &store,
                "main",
                IntegrityMode::TrustedLocalDev,
                spool.clone(),
                [0x92; 32],
            )
            .unwrap();
            let directory_attr = mounted.mkdir(ROOT_NODE, b"dir", 0o755).unwrap();
            let (file, handle) = mounted
                .create_file(directory_attr.node, b"file", 0o640)
                .unwrap();
            mounted
                .write(file.node, handle, 0, b"persistent bytes")
                .unwrap();
            mounted
                .link(file.node, directory_attr.node, b"alias")
                .unwrap();
            mounted.reset_engine_counters().unwrap();
            let accepted = mounted.fsync().unwrap();
            let engine = mounted.engine_counters().unwrap();
            assert_eq!(engine.transactions_started, 1);
            assert_eq!(engine.transactions_committed, 1);
            assert_eq!(engine.publication_commits, 1);
            mounted.release(handle).unwrap();
            mounted.shutdown().unwrap();
            accepted
        };
        let mut reopened = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x92; 32],
        )
        .unwrap();
        assert_eq!(reopened.accepted(), &accepted);
        let dir = reopened.lookup_child(ROOT_NODE, b"dir").unwrap();
        let file = reopened.lookup_child(dir.node, b"file").unwrap();
        let alias = reopened.lookup_child(dir.node, b"alias").unwrap();
        assert_eq!(file.node, alias.node);
        assert_eq!(file.links, 2);
        let handle = reopened.open_file(file.node, false).unwrap();
        assert_eq!(
            reopened.read(file.node, handle, 0, 64).unwrap(),
            b"persistent bytes"
        );
        reopened.release(handle).unwrap();
        reopened.shutdown().unwrap();
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn later_checkpoints_preserve_untouched_hard_link_content() {
        let (store, spool, directory) = paths("multi-checkpoint");
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x94; 32],
        )
        .unwrap();
        let (guard, guard_handle) = mounted.create_file(ROOT_NODE, b"guard", 0o644).unwrap();
        mounted
            .write(guard.node, guard_handle, 0, b"guard-bytes")
            .unwrap();
        mounted.link(guard.node, ROOT_NODE, b"guard-alias").unwrap();
        mounted.fsync().unwrap();
        mounted.release(guard_handle).unwrap();
        let handle = mounted.open_file(guard.node, false).unwrap();
        assert_eq!(
            mounted.read(guard.node, handle, 0, 64).unwrap(),
            b"guard-bytes"
        );
        mounted.release(handle).unwrap();

        let temp = mounted.mkdir(ROOT_NODE, b"temporary", 0o755).unwrap();
        let (child, child_handle) = mounted.create_file(temp.node, b"child", 0o644).unwrap();
        mounted
            .write(child.node, child_handle, 0, b"temporary")
            .unwrap();
        mounted.fsync().unwrap();
        mounted.release(child_handle).unwrap();
        let handle = mounted.open_file(guard.node, false).unwrap();
        assert_eq!(
            mounted.read(guard.node, handle, 0, 64).unwrap(),
            b"guard-bytes"
        );
        mounted.release(handle).unwrap();
        mounted.unlink(temp.node, b"child").unwrap();
        mounted.rmdir(ROOT_NODE, b"temporary").unwrap();
        let (other, other_handle) = mounted.create_file(ROOT_NODE, b"other", 0o644).unwrap();
        mounted
            .write(other.node, other_handle, 0, b"other")
            .unwrap();
        mounted.fsync().unwrap();
        mounted.release(other_handle).unwrap();

        let guard = mounted.lookup_child(ROOT_NODE, b"guard-alias").unwrap();
        let handle = mounted.open_file(guard.node, false).unwrap();
        assert_eq!(
            mounted.read(guard.node, handle, 0, 64).unwrap(),
            b"guard-bytes"
        );
        mounted.release(handle).unwrap();
        mounted.shutdown().unwrap();
        drop(mounted);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn shrink_then_extend_or_write_never_reveals_truncated_base_bytes() {
        let (store, spool, directory) = paths("truncate-watermark");
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool.clone(),
            [0x96; 32],
        )
        .unwrap();
        let (extended, extended_handle) =
            mounted.create_file(ROOT_NODE, b"extended", 0o644).unwrap();
        let (written, written_handle) = mounted.create_file(ROOT_NODE, b"written", 0o644).unwrap();
        mounted
            .write(extended.node, extended_handle, 0, b"abcdef")
            .unwrap();
        mounted
            .write(written.node, written_handle, 0, b"abcdef")
            .unwrap();
        mounted.fsync().unwrap();

        mounted.truncate(extended.node, 2).unwrap();
        mounted.truncate(extended.node, 6).unwrap();
        mounted.truncate(written.node, 2).unwrap();
        mounted
            .write(written.node, written_handle, 4, b"Z")
            .unwrap();
        assert_eq!(
            mounted.read(extended.node, extended_handle, 0, 16).unwrap(),
            b"ab\0\0\0\0"
        );
        assert_eq!(
            mounted.read(written.node, written_handle, 0, 16).unwrap(),
            b"ab\0\0Z"
        );
        mounted.fsync().unwrap();
        mounted.release(extended_handle).unwrap();
        mounted.release(written_handle).unwrap();
        mounted.shutdown().unwrap();
        drop(mounted);

        let mut reopened = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x96; 32],
        )
        .unwrap();
        for (name, expected) in [
            (b"extended".as_slice(), b"ab\0\0\0\0".as_slice()),
            (b"written".as_slice(), b"ab\0\0Z".as_slice()),
        ] {
            let file = reopened.lookup_child(ROOT_NODE, name).unwrap();
            let handle = reopened.open_file(file.node, false).unwrap();
            assert_eq!(reopened.read(file.node, handle, 0, 16).unwrap(), expected);
            reopened.release(handle).unwrap();
        }
        reopened.shutdown().unwrap();
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn repeated_fsync_of_accepted_unlinked_open_orphan_is_exact() {
        let (store, spool, directory) = paths("accepted-orphan");
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool.clone(),
            [0x97; 32],
        )
        .unwrap();
        let (file, handle) = mounted.create_file(ROOT_NODE, b"orphan", 0o644).unwrap();
        mounted.write(file.node, handle, 0, b"still-open").unwrap();
        mounted.fsync().unwrap();
        mounted.unlink(ROOT_NODE, b"orphan").unwrap();
        let removed = mounted.fsync().unwrap();
        mounted.reset_engine_counters().unwrap();
        mounted
            .write(file.node, handle, 0, b"changed-open")
            .unwrap();
        mounted.truncate(file.node, 7).unwrap();
        mounted.truncate(file.node, 9).unwrap();
        let orphan_only = mounted.fsync().unwrap();
        let repeated = mounted.fsync().unwrap();
        assert_eq!(orphan_only, removed);
        assert_eq!(repeated, removed);
        assert_eq!(
            mounted.read(file.node, handle, 0, 32).unwrap(),
            b"changed\0\0"
        );
        let counters = mounted.engine_counters().unwrap();
        assert_eq!(counters.transactions_started, 0);
        assert_eq!(counters.transactions_committed, 0);
        assert_eq!(counters.transactions_rolled_back, 0);
        assert_eq!(counters.publication_commits, 0);
        mounted.release(handle).unwrap();
        mounted.shutdown().unwrap();
        drop(mounted);

        let mut reopened = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x97; 32],
        )
        .unwrap();
        assert!(matches!(
            reopened.lookup_child(ROOT_NODE, b"orphan"),
            Err(MountedError::NotFound)
        ));
        reopened.shutdown().unwrap();
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn open_orphan_retains_dirty_bytes_through_delete_checkpoint_until_release() {
        for write_before_unlink in [true, false] {
            let label = if write_before_unlink {
                "orphan-write-before-unlink"
            } else {
                "orphan-unlink-before-write"
            };
            let (store, spool, directory) = paths(label);
            let mut mounted = MountedWorkspace::open(
                &store,
                "main",
                IntegrityMode::TrustedLocalDev,
                spool.clone(),
                [0x97; 32],
            )
            .unwrap();
            let (file, handle) = mounted.create_file(ROOT_NODE, b"orphan", 0o644).unwrap();
            mounted.write(file.node, handle, 0, b"version-one").unwrap();
            mounted.fsync().unwrap();
            if write_before_unlink {
                mounted.write(file.node, handle, 0, b"version-two").unwrap();
                mounted.unlink(ROOT_NODE, b"orphan").unwrap();
            } else {
                mounted.unlink(ROOT_NODE, b"orphan").unwrap();
                mounted.write(file.node, handle, 0, b"version-two").unwrap();
            }
            let removed = mounted.fsync().unwrap();
            assert_eq!(
                mounted.read(file.node, handle, 0, 32).unwrap(),
                b"version-two"
            );
            let retained = mounted.counters().unwrap();
            assert!(retained.dirty_ranges > 0);
            assert!(retained.spool_live_bytes > 0);
            mounted.reset_engine_counters().unwrap();
            assert_eq!(mounted.fsync().unwrap(), removed);
            let engine = mounted.engine_counters().unwrap();
            assert_eq!(engine.transactions_started, 0);
            assert_eq!(engine.transactions_committed, 0);
            assert_eq!(engine.transactions_rolled_back, 0);
            assert_eq!(engine.publication_commits, 0);
            mounted.release(handle).unwrap();
            mounted.forget(file.node, 1);
            let released = mounted.counters().unwrap();
            assert_eq!(released.dirty_ranges, 0);
            assert_eq!(released.spool_live_bytes, 0);
            assert_eq!(released.spool_physical_bytes, 0);
            assert_eq!(released.pending_nodes, 0);
            assert_eq!(released.dirty_nodes, 0);
            assert!(!mounted.nodes.contains_key(&file.node));
            mounted.shutdown().unwrap();
            drop(mounted);

            let mut reopened = MountedWorkspace::open(
                &store,
                "main",
                IntegrityMode::TrustedLocalDev,
                spool,
                [0x97; 32],
            )
            .unwrap();
            assert!(matches!(
                reopened.lookup_child(ROOT_NODE, b"orphan"),
                Err(MountedError::NotFound)
            ));
            reopened.shutdown().unwrap();
            drop(reopened);
            std::fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn resource_limit_failures_leave_namespace_content_and_spool_atomic() {
        let (store, spool, directory) = paths("resource-preflight");
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x9b; 32],
        )
        .unwrap();
        let (file, handle) = mounted.create_file(ROOT_NODE, b"file", 0o644).unwrap();
        mounted.write(file.node, handle, 0, b"keep").unwrap();
        mounted.fsync().unwrap();
        mounted.release(handle).unwrap();

        assert!(matches!(
            mounted.truncate(file.node, MAX_LOGICAL_FILE_BYTES + 1),
            Err(MountedError::NoSpace)
        ));
        assert_eq!(mounted.getattr(file.node).unwrap().size, 4);
        assert_eq!(mounted.counters().unwrap().dirty_nodes, 0);

        let next_handle = mounted.next_handle;
        mounted.next_handle = u64::MAX;
        assert!(matches!(
            mounted.open_file(file.node, true),
            Err(MountedError::TooManyOpenFiles)
        ));
        assert!(matches!(
            mounted.create_file(ROOT_NODE, b"partial-create", 0o644),
            Err(MountedError::TooManyOpenFiles)
        ));
        mounted.next_handle = next_handle;
        assert!(matches!(
            mounted.lookup_child(ROOT_NODE, b"partial-create"),
            Err(MountedError::NotFound)
        ));
        let handle = mounted.open_file(file.node, false).unwrap();
        assert_eq!(mounted.read(file.node, handle, 0, 16).unwrap(), b"keep");

        mounted.spool.live = MAX_LIVE_SPOOL_BYTES;
        assert!(matches!(
            mounted.write(file.node, handle, 0, b"x"),
            Err(MountedError::NoSpace)
        ));
        assert_eq!(mounted.spool.physical(), 0);
        mounted.spool.live = 0;
        assert_eq!(mounted.read(file.node, handle, 0, 16).unwrap(), b"keep");
        mounted.release(handle).unwrap();

        mounted
            .dirty_nodes
            .extend((0..MAX_DIRTY_NODES).map(|index| MountedNodeId(u64::MAX - index as u64)));
        assert!(matches!(
            mounted.chmod(file.node, 0o600),
            Err(MountedError::ResourceExhausted)
        ));
        assert_eq!(mounted.getattr(file.node).unwrap().mode, 0o644);
        assert!(matches!(
            mounted.link(file.node, ROOT_NODE, b"partial-link"),
            Err(MountedError::ResourceExhausted)
        ));
        assert!(matches!(
            mounted.rename(ROOT_NODE, b"file", ROOT_NODE, b"partial-rename", false),
            Err(MountedError::ResourceExhausted)
        ));
        assert!(matches!(
            mounted.mknod_file(ROOT_NODE, b"partial-node", 0o644),
            Err(MountedError::ResourceExhausted)
        ));
        mounted.dirty_nodes.clear();
        assert!(mounted.lookup_child(ROOT_NODE, b"file").is_ok());
        for absent in [
            b"partial-link".as_slice(),
            b"partial-rename",
            b"partial-node",
        ] {
            assert!(matches!(
                mounted.lookup_child(ROOT_NODE, absent),
                Err(MountedError::NotFound)
            ));
        }

        mounted.truncate(file.node, MAX_LOGICAL_FILE_BYTES).unwrap();
        let second = mounted
            .mknod_file(ROOT_NODE, b"workspace-limit", 0o644)
            .unwrap();
        let remaining = MAX_LOGICAL_WORKSPACE_BYTES - MAX_LOGICAL_FILE_BYTES;
        assert!(matches!(
            mounted.truncate(second.node, remaining + 1),
            Err(MountedError::NoSpace)
        ));
        assert_eq!(mounted.getattr(second.node).unwrap().size, 0);
        mounted.truncate(file.node, 4).unwrap();
        mounted.unlink(ROOT_NODE, b"workspace-limit").unwrap();

        mounted.shutdown().unwrap();
        drop(mounted);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn capacity_tracks_logical_spool_node_and_dirty_limits() {
        let (store, spool, directory) = paths("capacity");
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x9d; 32],
        )
        .unwrap();
        let initial = mounted.capacity().unwrap();
        assert_eq!(initial.total_bytes, MAX_LOGICAL_WORKSPACE_BYTES);
        assert_eq!(initial.free_bytes, MAX_LIVE_SPOOL_BYTES);
        assert_eq!(initial.total_files, MAX_MOUNTED_NODES as u64);
        assert_eq!(initial.free_files, MAX_DIRTY_NODES as u64);

        mounted.logical_workspace_bytes = MAX_LOGICAL_WORKSPACE_BYTES;
        assert_eq!(mounted.capacity().unwrap().free_bytes, 0);
        mounted.logical_workspace_bytes = 0;
        mounted.spool.live = MAX_LIVE_SPOOL_BYTES;
        assert_eq!(mounted.capacity().unwrap().free_bytes, 0);
        mounted.spool.live = 0;

        mounted
            .dirty_nodes
            .extend((0..MAX_DIRTY_NODES).map(|index| MountedNodeId(u64::MAX - index as u64)));
        assert_eq!(mounted.capacity().unwrap().free_files, 0);
        mounted.dirty_nodes.clear();
        mounted.directory_changes = MAX_DIRECTORY_CHANGES;
        assert_eq!(mounted.capacity().unwrap().free_files, 0);
        mounted.directory_changes = 0;

        mounted.shutdown().unwrap();
        drop(mounted);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn maximum_admitted_dirty_population_is_checkpointable_inside_q() {
        let (store, spool, directory) = paths("maximum-checkpoint");
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x9e; 32],
        )
        .unwrap();
        for index in 0..MAX_DIRTY_NODES - 1 {
            mounted
                .mknod_file(ROOT_NODE, format!("file-{index:04}").as_bytes(), 0o644)
                .unwrap();
        }
        assert_eq!(
            mounted.counters().unwrap().dirty_nodes,
            MAX_DIRTY_NODES as u64
        );
        assert!(matches!(
            mounted.mknod_file(ROOT_NODE, b"one-too-many", 0o644),
            Err(MountedError::ResourceExhausted)
        ));
        mounted.fsync().unwrap();
        let counters = mounted.counters().unwrap();
        assert_eq!(counters.dirty_nodes, 0);
        assert_eq!(counters.operation_q_current_bytes, 0);
        assert_eq!(
            counters.operation_q_high_water_bytes,
            MAX_OPERATION_Q_BYTES as u64
        );
        mounted.shutdown().unwrap();
        drop(mounted);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn retained_roots_fork_and_rollback_reopen_exact_versions() {
        let (store, spool, directory) = paths("history");
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool.clone(),
            [0x95; 32],
        )
        .unwrap();
        let (file, handle) = mounted.create_file(ROOT_NODE, b"versioned", 0o644).unwrap();
        mounted.write(file.node, handle, 0, b"version-one").unwrap();
        let first = mounted.fsync().unwrap();
        mounted.write(file.node, handle, 0, b"version-two").unwrap();
        let second = mounted.fsync().unwrap();
        mounted.release(handle).unwrap();
        let branch = mounted.fork_ref("branch").unwrap();
        assert_eq!(branch.root, second.root);

        mounted.rollback(first.root).unwrap();
        assert_eq!(mounted.lifecycle(), MountedLifecycle::Closed);
        assert!(matches!(
            mounted.lookup_child(ROOT_NODE, b"versioned"),
            Err(MountedError::StaleHandle)
        ));
        drop(mounted);

        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool.clone(),
            [0x95; 32],
        )
        .unwrap();
        let file = mounted.lookup_child(ROOT_NODE, b"versioned").unwrap();
        let handle = mounted.open_file(file.node, false).unwrap();
        assert_eq!(
            mounted.read(file.node, handle, 0, 64).unwrap(),
            b"version-one"
        );
        mounted.release(handle).unwrap();

        mounted.rollback(second.root).unwrap();
        assert_eq!(mounted.lifecycle(), MountedLifecycle::Closed);
        drop(mounted);

        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x95; 32],
        )
        .unwrap();
        let file = mounted.lookup_child(ROOT_NODE, b"versioned").unwrap();
        let handle = mounted.open_file(file.node, false).unwrap();
        assert_eq!(
            mounted.read(file.node, handle, 0, 64).unwrap(),
            b"version-two"
        );
        mounted.release(handle).unwrap();
        mounted.shutdown().unwrap();
        drop(mounted);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rollback_conflict_leaves_stale_mount_non_writable() {
        let (store, spool, directory) = paths("rollback-conflict");
        let target = {
            let mut mounted = MountedWorkspace::open(
                &store,
                "main",
                IntegrityMode::TrustedLocalDev,
                spool.clone(),
                [0x95; 32],
            )
            .unwrap();
            let (file, handle) = mounted.create_file(ROOT_NODE, b"file", 0o644).unwrap();
            mounted.write(file.node, handle, 0, b"version-one").unwrap();
            let target = mounted.fsync().unwrap().root;
            mounted.write(file.node, handle, 0, b"version-two").unwrap();
            mounted.fsync().unwrap();
            mounted.release(handle).unwrap();
            mounted.shutdown().unwrap();
            target
        };
        let mut winner = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool.clone(),
            [0x95; 32],
        )
        .unwrap();
        let mut loser = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            directory.join("loser.spool"),
            [0x95; 32],
        )
        .unwrap();
        winner.mknod_file(ROOT_NODE, b"winner", 0o644).unwrap();
        winner.fsync().unwrap();
        assert!(matches!(
            loser.rollback(target),
            Err(MountedError::Conflict)
        ));
        assert_eq!(loser.lifecycle(), MountedLifecycle::Conflict);
        assert!(matches!(
            loser.mknod_file(ROOT_NODE, b"late", 0o644),
            Err(MountedError::Indeterminate)
        ));
        drop(loser);
        winner.shutdown().unwrap();
        drop(winner);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rollback_ambiguity_leaves_mount_incomplete_and_non_writable() {
        let (store, spool, directory) = paths("rollback-ambiguity");
        let target = {
            let mut mounted = MountedWorkspace::open(
                &store,
                "main",
                IntegrityMode::TrustedLocalDev,
                spool.clone(),
                [0x95; 32],
            )
            .unwrap();
            let (file, handle) = mounted.create_file(ROOT_NODE, b"file", 0o644).unwrap();
            mounted.write(file.node, handle, 0, b"version-one").unwrap();
            let target = mounted.fsync().unwrap().root;
            mounted.write(file.node, handle, 0, b"version-two").unwrap();
            mounted.fsync().unwrap();
            mounted.release(handle).unwrap();
            mounted.shutdown().unwrap();
            target
        };
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x95; 32],
        )
        .unwrap();
        mounted.close_store_connection().unwrap();
        assert!(matches!(
            mounted.rollback(target),
            Err(MountedError::Indeterminate)
        ));
        assert_eq!(mounted.lifecycle(), MountedLifecycle::Incomplete);
        assert!(matches!(
            mounted.mknod_file(ROOT_NODE, b"late", 0o644),
            Err(MountedError::Indeterminate)
        ));
        drop(mounted);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn shutdown_closes_mutation_admission_before_and_after_checkpoint() {
        let (store, spool, directory) = paths("closed");
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x98; 32],
        )
        .unwrap();
        mounted.mknod_file(ROOT_NODE, b"accepted", 0o644).unwrap();
        let first = mounted.shutdown().unwrap();
        assert_eq!(mounted.lifecycle(), MountedLifecycle::Closed);
        assert!(matches!(
            mounted.mknod_file(ROOT_NODE, b"late", 0o644),
            Err(MountedError::StaleHandle)
        ));
        assert_eq!(mounted.shutdown().unwrap(), first);
        drop(mounted);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn paused_budget_allows_one_dirty_graceful_shutdown_checkpoint() {
        let (store, spool, directory) = paths("paused-dirty-shutdown");
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool.clone(),
            [0x98; 32],
        )
        .unwrap();
        let (file, handle) = mounted.create_file(ROOT_NODE, b"dirty", 0o644).unwrap();
        mounted
            .write(file.node, handle, 0, b"graceful-dirty-bytes")
            .unwrap();
        mounted.release(handle).unwrap();
        let budget = mounted.byte_budget();
        budget.pause_and_wait().unwrap();
        mounted.shutdown().unwrap();
        budget.close_and_wait().unwrap();
        let counters = mounted.counters().unwrap();
        assert_eq!(counters.checkpoints, 1);
        assert_eq!(counters.operation_q_current_bytes, 0);
        assert_eq!(counters.dirty_nodes, 0);
        assert_eq!(counters.dirty_ranges, 0);
        assert_eq!(counters.spool_live_bytes, 0);
        drop(mounted);

        let mut reopened = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x98; 32],
        )
        .unwrap();
        let file = reopened.lookup_child(ROOT_NODE, b"dirty").unwrap();
        let handle = reopened.open_file(file.node, false).unwrap();
        assert_eq!(
            reopened.read(file.node, handle, 0, 64).unwrap(),
            b"graceful-dirty-bytes"
        );
        reopened.release(handle).unwrap();
        reopened.shutdown().unwrap();
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn publication_conflict_leaves_mount_non_writable() {
        let (store, spool, directory) = paths("conflict");
        let mut winner = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool.clone(),
            [0x99; 32],
        )
        .unwrap();
        let mut loser = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            directory.join("loser.spool"),
            [0x9a; 32],
        )
        .unwrap();
        winner.mknod_file(ROOT_NODE, b"winner", 0o644).unwrap();
        winner.fsync().unwrap();
        loser.mknod_file(ROOT_NODE, b"loser", 0o644).unwrap();
        loser.reset_engine_counters().unwrap();
        assert!(matches!(loser.fsync(), Err(MountedError::Conflict)));
        assert_eq!(loser.lifecycle(), MountedLifecycle::Conflict);
        assert!(matches!(
            loser.mknod_file(ROOT_NODE, b"late", 0o644),
            Err(MountedError::Indeterminate)
        ));
        let counters = loser.engine_counters().unwrap();
        assert_eq!(counters.transactions_committed, 0);
        assert_eq!(counters.publication_commits, 0);
        drop(loser);
        winner.shutdown().unwrap();
        drop(winner);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn post_commit_spool_cleanup_failure_is_incomplete_not_retryable() {
        let (store, spool, directory) = paths("committed-cleanup");
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool.clone(),
            [0x9d; 32],
        )
        .unwrap();
        mounted.reset_engine_counters().unwrap();
        let (file, handle) = mounted.create_file(ROOT_NODE, b"committed", 0o644).unwrap();
        mounted
            .write(file.node, handle, 0, b"committed-bytes")
            .unwrap();
        mounted.spool.path = directory.clone();
        assert!(matches!(
            mounted.fsync(),
            Err(MountedError::CommittedCleanup)
        ));
        assert_eq!(mounted.lifecycle(), MountedLifecycle::Incomplete);
        let counters = mounted.engine_counters().unwrap();
        assert_eq!(counters.transactions_committed, 1);
        assert_eq!(counters.publication_commits, 1);
        assert!(matches!(
            mounted.write(file.node, handle, 0, b"late"),
            Err(MountedError::Indeterminate)
        ));
        mounted.spool.path = spool.clone();
        drop(mounted);

        let mut reopened = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x9d; 32],
        )
        .unwrap();
        let file = reopened.lookup_child(ROOT_NODE, b"committed").unwrap();
        let handle = reopened.open_file(file.node, false).unwrap();
        assert_eq!(
            reopened.read(file.node, handle, 0, 32).unwrap(),
            b"committed-bytes"
        );
        reopened.release(handle).unwrap();
        reopened.shutdown().unwrap();
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn pre_commit_spool_failure_rolls_back_and_preserves_retryable_dirty_state() {
        let (store, spool, directory) = paths("precommit-spool");
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool.clone(),
            [0x9f; 32],
        )
        .unwrap();
        let (file, handle) = mounted.create_file(ROOT_NODE, b"retry", 0o644).unwrap();
        mounted.write(file.node, handle, 0, b"retry-bytes").unwrap();
        mounted.reset_engine_counters().unwrap();
        drop(mounted.spool.file.take());
        assert!(matches!(mounted.fsync(), Err(MountedError::Corrupt)));
        assert_eq!(mounted.lifecycle(), MountedLifecycle::Live);
        let counters = mounted.engine_counters().unwrap();
        assert_eq!(counters.transactions_started, 1);
        assert_eq!(counters.transactions_committed, 0);
        assert_eq!(counters.transactions_rolled_back, 1);
        assert_eq!(counters.publication_commits, 0);
        assert!(mounted.counters().unwrap().dirty_nodes > 0);
        mounted.spool.file = Some(
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(&spool)
                .unwrap(),
        );
        mounted.fsync().unwrap();
        mounted.release(handle).unwrap();
        mounted.shutdown().unwrap();
        drop(mounted);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn lost_commit_acknowledgement_reconciles_candidate_and_clears_dirty_state() {
        let (store, spool, directory) = paths("lost-ack");
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool.clone(),
            [0xa2; 32],
        )
        .unwrap();
        let (file, handle) = mounted
            .create_file(ROOT_NODE, b"reconciled", 0o644)
            .unwrap();
        mounted
            .write(file.node, handle, 0, b"reconciled-bytes")
            .unwrap();
        mounted.reset_engine_counters().unwrap();
        mounted.engine.inject_lost_commit_acknowledgement();
        let accepted = mounted.fsync().unwrap();
        assert_eq!(mounted.lifecycle(), MountedLifecycle::Live);
        assert_eq!(mounted.counters().unwrap().dirty_nodes, 0);
        let counters = mounted.engine_counters().unwrap();
        assert_eq!(counters.transactions_committed, 1);
        assert_eq!(counters.publication_commits, 1);
        assert!(counters.reconciliation_statements > 0);
        mounted.release(handle).unwrap();
        mounted.shutdown().unwrap();
        drop(mounted);

        let mut reopened = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0xa2; 32],
        )
        .unwrap();
        assert_eq!(reopened.accepted(), &accepted);
        let file = reopened.lookup_child(ROOT_NODE, b"reconciled").unwrap();
        let handle = reopened.open_file(file.node, false).unwrap();
        assert_eq!(
            reopened.read(file.node, handle, 0, 32).unwrap(),
            b"reconciled-bytes"
        );
        reopened.release(handle).unwrap();
        reopened.shutdown().unwrap();
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn forced_drop_discards_unacknowledged_dirty_spool_on_exact_reopen() {
        let (store, spool, directory) = paths("forced-drop");
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool.clone(),
            [0xa0; 32],
        )
        .unwrap();
        let (file, handle) = mounted
            .create_file(ROOT_NODE, b"unacknowledged", 0o644)
            .unwrap();
        mounted.write(file.node, handle, 0, b"discard-me").unwrap();
        assert!(spool.exists());
        drop(mounted);

        let mut reopened = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool.clone(),
            [0xa0; 32],
        )
        .unwrap();
        assert!(!spool.exists());
        assert!(matches!(
            reopened.lookup_child(ROOT_NODE, b"unacknowledged"),
            Err(MountedError::NotFound)
        ));
        reopened.shutdown().unwrap();
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn pending_directory_cursor_is_complete_and_resumable() {
        let (store, spool, directory) = paths("readdir");
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x93; 32],
        )
        .unwrap();
        let dir = mounted.mkdir(ROOT_NODE, b"dir", 0o755).unwrap();
        for index in 0..300 {
            mounted
                .mknod_file(dir.node, format!("file-{index:03}").as_bytes(), 0o644)
                .unwrap();
        }
        let handle = mounted.open_directory(dir.node).unwrap();
        let mut offset = 0;
        let mut names = Vec::new();
        loop {
            let entries = mounted.readdir(handle, offset, 19).unwrap();
            if entries.is_empty() {
                break;
            }
            offset = entries.last().unwrap().next_offset;
            names.extend(entries.into_iter().map(|entry| entry.name));
        }
        assert_eq!(names.len(), 302);
        assert_eq!(names[0], b".");
        assert_eq!(names[1], b"..");
        assert!(names[2..].windows(2).all(|pair| pair[0] < pair[1]));
        mounted.release(handle).unwrap();
        drop(mounted);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn readdir_pending_entry_reclaims_nodes_without_changing_inode_identity() {
        let (store, spool, directory) = paths("readdir-reclaim");
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x9c; 32],
        )
        .unwrap();
        let file = mounted.mknod_file(ROOT_NODE, b"file", 0o644).unwrap();
        mounted.fsync().unwrap();
        mounted.forget(file.node, 1);
        let handle = mounted.open_directory(ROOT_NODE).unwrap();
        assert_eq!(mounted.readdir(handle, 0, 2).unwrap().len(), 2);
        let pending = mounted.readdir_next(handle, 2).unwrap().unwrap();
        let emitted = pending.node;
        assert_eq!(pending.name, b"file");
        mounted.discard_readdir_pending(handle).unwrap();
        mounted.reclaim_readdir_nodes(&[emitted]);
        assert!(!mounted.nodes.contains_key(&emitted));
        let replayed = mounted.readdir_next(handle, 2).unwrap().unwrap();
        assert_eq!(replayed.node, emitted);
        mounted
            .commit_readdir(handle, replayed.next_offset)
            .unwrap();
        mounted.reclaim_readdir_nodes(&[emitted]);
        let looked_up = mounted.lookup_child(ROOT_NODE, b"file").unwrap();
        assert_eq!(looked_up.node, emitted);
        mounted.release(handle).unwrap();
        mounted.shutdown().unwrap();
        drop(mounted);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn cold_lookup_and_returned_attrs_stay_inside_the_sql_gate() {
        let (store, spool, directory) = paths("cold-lookup-gate");
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool.clone(),
            [0x9c; 32],
        )
        .unwrap();
        mounted.mknod_file(ROOT_NODE, b"file", 0o640).unwrap();
        mounted.fsync().unwrap();
        mounted.shutdown().unwrap();
        drop(mounted);

        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x9c; 32],
        )
        .unwrap();
        mounted.reset_engine_counters().unwrap();
        let file = mounted.lookup_child(ROOT_NODE, b"file").unwrap();
        assert_eq!(file.mode, 0o640);
        let counters = mounted.engine_counters().unwrap();
        eprintln!("cold_lookup_primary_sql_statements={}", counters.statements);
        assert!(
            counters.statements <= MAX_COLD_LOOKUP_PRIMARY_STATEMENTS,
            "cold lookup used {} primary SQL statements",
            counters.statements
        );
        mounted.shutdown().unwrap();
        drop(mounted);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
