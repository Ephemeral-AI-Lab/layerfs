use crate::{backing::budget::Charge, runtime::state::OperationGuard};
use layerfs_bridge::contract::{Failure, Request, Response, Root, Source};
use std::{io::Write, path::PathBuf, sync::Arc, time::Instant};

pub const DEFAULT_MEMORY_BUDGET_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_READ_BYTES: usize = 128 * 1024;
pub const MAX_DIRECTORY_ENTRIES: usize = 128;
pub type HandleId = u64;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceScope {
    Local,
    Projection,
}
pub type OperationDelivery = Arc<
    dyn Fn(&Request, &mut dyn Source, &mut dyn Write, Instant) -> Result<Response, Failure>
        + Send
        + Sync,
>;

#[derive(Clone, Debug)]
pub struct WorkspaceConfig {
    pub root: PathBuf,
    pub max_count: usize,
    pub memory_budget_bytes: usize,
    pub disk_budget_bytes: Option<u64>,
}
#[derive(Clone, Debug)]
pub enum Base {
    Root(Root),
    Branch([u8; 17]),
}
#[derive(Clone, Debug)]
pub struct AttachOptions {
    pub access: WorkspaceAccess,
    pub id: String,
    pub incarnation: Root,
    pub store: u32,
    pub base: Base,
    pub owner_uid: u32,
    pub owner_gid: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    File,
    Directory,
    Symlink,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeAttributes {
    pub serial: u64,
    pub kind: NodeKind,
    pub size: u64,
    pub references: u64,
    pub mode: u32,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: u32,
    pub uid: u32,
    pub gid: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceError {
    InvalidInput,
    Capacity,
    Busy,
    Closed,
    NotFound,
    NotDirectory,
    IsDirectory,
    WrongKind,
    BadHandle,
    ReadOnly,
    Denied,
    Unsupported,
    Deadline,
    Io,
    Service(Failure),
    Backing(BackingFailure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackingPhase {
    Acquire,
    Create,
    Allocate,
    Write,
    Input,
    Read,
    Verify,
    Cleanup,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackingFailure {
    pub phase: BackingPhase,
    pub payload: u64,
    pub declared_bytes: u64,
    pub completed_bytes: u64,
    pub created_segments: u32,
    pub allocated_bytes: u64,
    pub reserved_bytes: u64,
    pub cleanup_failed: bool,
    pub accounting_complete: bool,
    pub kind: std::io::ErrorKind,
}
impl std::fmt::Display for BackingFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for BackingFailure {}

#[derive(Clone, Copy, Debug)]
pub struct BackingStatus {
    pub quota_bytes: u64,
    pub allocated_bytes: u64,
    pub reserved_bytes: u64,
    pub payloads: usize,
    pub retained_payloads: usize,
    pub failed_payloads: usize,
    pub readers: usize,
    pub acquiring: bool,
    pub cleaning: bool,
    pub admission_stopped: bool,
    pub accounting_complete: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CleanupReport {
    pub payloads_released: usize,
    pub segments_released: u32,
    pub bytes_released: u64,
    pub remaining_payloads: usize,
}
impl From<Failure> for WorkspaceError {
    fn from(value: Failure) -> Self {
        Self::Service(value)
    }
}
impl From<std::io::Error> for WorkspaceError {
    fn from(_: std::io::Error) -> Self {
        Self::Io
    }
}
impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for WorkspaceError {}

#[derive(Clone, Copy, Debug)]
pub struct WorkspaceStatus {
    pub generation: u64,
    pub revision: u64,
    pub dirty_inodes: usize,
    pub mounted: bool,
    pub stopping: bool,
    pub closed: bool,
    pub active_operations: usize,
    pub nodes: usize,
    pub handles: usize,
    pub projection_handles: usize,
    pub cookies: usize,
    pub accounted_bytes: usize,
}
pub struct ReadReply {
    pub(crate) bytes: Vec<u8>,
    pub(crate) _charge: Charge,
    pub(crate) _operation: OperationGuard,
}
impl AsRef<[u8]> for ReadReply {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}
#[derive(Debug)]
pub struct DirectoryEntry {
    pub serial: u64,
    pub kind: NodeKind,
    pub name: Vec<u8>,
    pub cookie: u64,
}
pub struct DirectoryPage {
    pub(crate) entries: Vec<DirectoryEntry>,
    pub(crate) _charge: Charge,
    pub(crate) _operation: OperationGuard,
}
impl DirectoryPage {
    pub fn entries(&self) -> &[DirectoryEntry] {
        &self.entries
    }
}

/// Explicit local edit capability. Linux projection writes follow a later round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceAccess {
    ReadOnly,
    LocalEdit,
}
#[derive(Clone, Debug)]
pub struct WorkspacePath(Vec<u8>);
impl WorkspacePath {
    pub fn new(bytes: &[u8]) -> Result<Self, WorkspaceError> {
        if bytes.is_empty() || bytes.len() > 4096 || bytes.split(|b| *b == b'/').count() > 256 {
            return Err(WorkspaceError::InvalidInput);
        }
        for part in bytes.split(|b| *b == b'/') {
            if part.is_empty()
                || part.len() > 255
                || part == b"."
                || part == b".."
                || part.iter().any(|b| matches!(b, 0 | b'\\'))
                || std::str::from_utf8(part).is_err()
            {
                return Err(WorkspaceError::InvalidInput);
            }
        }
        let mut owned = Vec::new();
        owned
            .try_reserve_exact(bytes.len())
            .map_err(|_| WorkspaceError::Capacity)?;
        owned.extend_from_slice(bytes);
        Ok(Self(owned))
    }
}
impl AsRef<[u8]> for WorkspacePath {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
pub struct RangeEdit {
    pub start: u64,
    pub end: u64,
    pub replacement: crate::OwnedPayload,
}
#[derive(Clone, Copy, Debug)]
pub struct MutationReceipt {
    pub incarnation: [u8; 32],
    pub generation: u64,
    pub inode: u64,
    pub revision: u64,
    pub accepted_bytes: u64,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct MetadataCleanupReport {
    pub roots_released: usize,
    pub pages_reclaimed: usize,
    pub payload_custodies_released: usize,
    pub remaining_roots: usize,
}
#[derive(Clone, Copy, Debug)]
pub struct MetadataStatus {
    pub allocated_pages: usize,
    pub reusable_pages: usize,
    pub roots: usize,
    pub external_roots: usize,
    pub allocated_bytes: u64,
    pub reserved_bytes: u64,
    pub working_bytes: usize,
    pub accounting_complete: bool,
    pub admission_stopped: bool,
}
