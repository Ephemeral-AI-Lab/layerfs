use layerfs_core::content::rope::{
    build, read_all_bounded, read_plan, read_range_with_plan, replace, state as rope_state,
    FileStateRoot, ObjectRead, ReadPlan, RopeCounters,
};
use layerfs_core::inode::{
    inode_table_lookup, visit_inode_table_entries, InodeId, InodeKind, InodeRecordV1,
    InodeTableCounters, InodeTableRoot,
};
use layerfs_core::metadata::{
    metadata_lookup, metadata_tree_entries, visit_metadata_entries, MetadataEntryV1, MetadataKey,
    MetadataTreeBuilder, PortableMetadataV1,
};
use layerfs_core::namespace::{
    directory_lookup, directory_page_after, empty_directory, DirectoryStateRoot, NamespaceCounters,
    NamespaceRootV1, SymlinkStateV1,
};
use layerfs_core::namespace_codec::{decode_inode_record, decode_namespace_root, decode_symlink};
use layerfs_core::{CanonicalName, CanonicalPath, CoreError, ObjectId};
use layerfs_workspace::{
    BeginOperation, CommitResult, EngineCounters, IntegrityMode, OperationRecordRef,
    WorkingCandidate, WorkingCandidateWrite, WorkingError, WorkingStore,
};
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
    pub generation: u64,
    pub before: ObjectId,
    pub after: ObjectId,
    pub counters: layerfs_core::logical::LogicalCounters,
    pub operation_q_terminal_bytes: u64,
    pub operation_q_high_water_bytes: u64,
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

impl From<WorkingError> for MountedError {
    fn from(value: WorkingError) -> Self {
        if value.is_no_space() {
            Self::NoSpace
        } else if value.is_read_only() {
            Self::ReadOnly
        } else {
            match value {
                WorkingError::Core(error) => error.into(),
                WorkingError::Io(error) => Self::Io(error),
                WorkingError::Storage(_) | WorkingError::InvalidReceipt => Self::Corrupt,
            }
        }
    }
}
