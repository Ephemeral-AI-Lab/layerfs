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
