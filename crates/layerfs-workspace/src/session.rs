use crate::{WorkspacePlacement, WorkspaceProjection, WorkspaceState};
use layerfs_storage::{BranchId, CommitId, HeadMoved, RefOutcome, StorageError};
use std::ffi::OsString;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorkspaceSessionId([u8; 16]);

impl WorkspaceSessionId {
    pub(crate) fn new() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let mut input = Vec::new();
        input.extend_from_slice(
            &SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
                .to_be_bytes(),
        );
        input.extend_from_slice(&std::process::id().to_be_bytes());
        input.extend_from_slice(&SERIAL.fetch_add(1, Ordering::Relaxed).to_be_bytes());
        let digest = layerfs_content::ObjectId::for_bytes(&input).to_bytes();
        Self(digest[..16].try_into().expect("fixed digest"))
    }
}

impl fmt::Display for WorkspaceSessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("w:")?;
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl std::str::FromStr for WorkspaceSessionId {
    type Err = WorkspaceError;

    fn from_str(value: &str) -> WorkspaceResult<Self> {
        Ok(Self(parse_id(value, "w:")?))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateWorkspaceSession {
    pub branch_id: BranchId,
    pub placement: WorkspacePlacement,
    pub projection: Option<WorkspaceProjection>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EndWorkspaceMode {
    Clean,
    Discard,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceSession {
    pub id: WorkspaceSessionId,
    pub branch_id: BranchId,
    pub pinned_head: CommitId,
    pub placement: WorkspacePlacement,
    pub projection: WorkspaceProjection,
    pub state: WorkspaceState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceSummary {
    pub id: WorkspaceSessionId,
    pub branch_id: BranchId,
    pub pinned_head: CommitId,
    pub state: WorkspaceState,
    pub dirty: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceDetail {
    pub session: WorkspaceSession,
    pub mutation_generation: u64,
    pub executions: Vec<ExecutionSummary>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceDiff {
    pub session_id: WorkspaceSessionId,
    pub dirty: bool,
    pub mutation_generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NonEmpty<T>(T);

impl NonEmpty<Vec<OsString>> {
    pub fn new(value: Vec<OsString>) -> WorkspaceResult<Self> {
        if value.is_empty() {
            Err(WorkspaceError::InvalidExecution)
        } else {
            Ok(Self(value))
        }
    }

    pub fn as_slice(&self) -> &[OsString] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExecutionId(pub(crate) [u8; 16]);

impl fmt::Display for ExecutionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("x:")?;
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl std::str::FromStr for ExecutionId {
    type Err = WorkspaceError;

    fn from_str(value: &str) -> WorkspaceResult<Self> {
        Ok(Self(parse_id(value, "x:")?))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputChunk {
    pub sequence: u64,
    pub stream: OutputStream,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionReceipt {
    pub execution_id: ExecutionId,
    pub exit_code: Option<i32>,
    pub elapsed_ns: u64,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
    pub stopped: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecutionEvent {
    Started(WorkspaceExecution),
    Stdout(OutputChunk),
    Stderr(OutputChunk),
    Exited(ExecutionReceipt),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceExecution {
    pub id: ExecutionId,
    pub session_id: WorkspaceSessionId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionSummary {
    pub id: ExecutionId,
    pub running: bool,
    pub receipt: Option<ExecutionReceipt>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkspaceCommitResult {
    Created {
        previous_head: CommitId,
        commit_id: CommitId,
    },
    UpToDate {
        head: CommitId,
    },
    HeadMoved {
        expected: CommitId,
        actual: CommitId,
    },
}

impl WorkspaceCommitResult {
    pub(crate) fn from_outcome(previous_head: CommitId, outcome: RefOutcome<CommitId>) -> Self {
        match outcome {
            RefOutcome::Created(commit_id) | RefOutcome::FastForwarded(commit_id) => {
                Self::Created {
                    previous_head,
                    commit_id,
                }
            }
            RefOutcome::UpToDate(head) => Self::UpToDate { head },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceEndResult {
    pub session_id: WorkspaceSessionId,
    pub discarded: bool,
}

#[derive(Debug)]
pub enum WorkspaceError {
    NotFound,
    WorkspaceBusy,
    WorkspaceDirty,
    ReadOnly,
    InvalidPlacement,
    InvalidExecution,
    Storage(StorageError),
    Io(std::io::Error),
}

pub type WorkspaceResult<T> = std::result::Result<T, WorkspaceError>;

impl WorkspaceError {
    pub(crate) fn from_commit(error: StorageError) -> WorkspaceResult<WorkspaceCommitResult> {
        match error {
            StorageError::CommitHeadMoved(HeadMoved {
                expected: Some(expected),
                actual: Some(actual),
            }) => Ok(WorkspaceCommitResult::HeadMoved { expected, actual }),
            StorageError::InvalidInput("workspace busy") => Err(Self::WorkspaceBusy),
            error => Err(Self::Storage(error)),
        }
    }
}

impl From<StorageError> for WorkspaceError {
    fn from(value: StorageError) -> Self {
        Self::Storage(value)
    }
}

impl From<std::io::Error> for WorkspaceError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for WorkspaceError {}

fn parse_id(value: &str, prefix: &str) -> WorkspaceResult<[u8; 16]> {
    let value = value
        .strip_prefix(prefix)
        .ok_or(WorkspaceError::InvalidExecution)?;
    if value.len() != 32 {
        return Err(WorkspaceError::InvalidExecution);
    }
    let mut bytes = [0; 16];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = (hex(pair[0])? << 4) | hex(pair[1])?;
    }
    Ok(bytes)
}

fn hex(value: u8) -> WorkspaceResult<u8> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(WorkspaceError::InvalidExecution),
    }
}
