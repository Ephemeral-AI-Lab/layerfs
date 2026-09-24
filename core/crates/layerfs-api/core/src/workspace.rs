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
    /// Bounded data-callback byte totals, labeled by direction and quantity.
    pub projection_bytes: Vec<(String, u64)>,
    /// Request-size histogram buckets, labeled `<direction>:<size range>`.
    pub projection_histogram: Vec<(String, u64)>,
    /// Upstream host Service calls this Workspace issued.
    pub upstream_calls: u64,
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

    /// One declared data-callback byte total, or `None` for a total this build
    /// does not report.
    pub fn projection_bytes(&self, label: &str) -> Option<u64> {
        self.projection_bytes
            .iter()
            .find(|(name, _)| name == label)
            .map(|(_, total)| *total)
    }

    /// One request-size histogram bucket, or `None` for a bucket this build does
    /// not report.
    pub fn projection_size(&self, label: &str) -> Option<u64> {
        self.projection_histogram
            .iter()
            .find(|(name, _)| name == label)
            .map(|(_, count)| *count)
    }
}
