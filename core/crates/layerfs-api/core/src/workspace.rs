//! Agent-facing Workspace identities and typed operation results.
use layerfs_bridge::contract::{Code, Failure, WorkspaceCommitFailureWire};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WorkspaceId(pub String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mount {
    pub id: WorkspaceId,
    /// Absolute path inside the sandbox, used as Exec's working directory.
    pub location: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecResult {
    pub exit_status: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceError {
    Failure(Failure),
    Retained { id: WorkspaceId, cause: Code },
    UncertainMount { id: WorkspaceId, cause: Failure },
    Stale,
    Commit(Box<WorkspaceCommitFailureWire>),
}
impl From<Failure> for WorkspaceError {
    fn from(value: Failure) -> Self {
        Self::Failure(value)
    }
}
impl From<Code> for WorkspaceError {
    fn from(value: Code) -> Self {
        Self::Failure(value.into())
    }
}
impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for WorkspaceError {}

/// One current observation of a Workspace, never a receipt for an earlier call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceStatus {
    pub mounted: bool,
    pub stopping: bool,
    pub closed: bool,
    pub active_operations: u64,
    pub nodes: u64,
    pub handles: u64,
    pub cookies: u64,
    pub consumer_accounted_bytes: u64,
    /// Bounded projection callback counts in `projection_labels` order.
    pub projection: Vec<(String, u64)>,
    /// Upstream host Service calls this Workspace issued.
    pub upstream_calls: u64,
    /// Replacement payload bytes accepted by published range edits.
    pub range_accepted_payload_bytes: u64,
    /// Physical suffix payload bytes copied by published range edits.
    pub range_shifted_suffix_bytes: u64,
}

impl WorkspaceStatus {
    /// One declared projection callback class count, or `None` for a class this
    /// build does not report.
    pub fn projection_count(&self, class: &str) -> Option<u64> {
        self.projection
            .iter()
            .find(|(label, _)| label == class)
            .map(|(_, count)| *count)
    }
}
