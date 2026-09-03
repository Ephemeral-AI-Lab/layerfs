use crate::WorkspaceState;
use layerfs_layerstack_store::{BranchId, CommitId, EntityName, LayerStackId, StoreError};
use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContainerId(pub String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkspacePlacement {
    Host {
        root: PathBuf,
    },
    Container {
        container_id: ContainerId,
        root: PathBuf,
    },
}

impl WorkspacePlacement {
    pub(crate) fn root(&self) -> &PathBuf {
        match self {
            Self::Host { root } | Self::Container { root, .. } => root,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceProjection {
    Fuse,
    Materialize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorkspaceId([u8; 16]);

impl WorkspaceId {
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

    #[cfg(unix)]
    pub(crate) fn bytes(self) -> [u8; 16] {
        self.0
    }
}

impl fmt::Display for WorkspaceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("w:")?;
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl std::str::FromStr for WorkspaceId {
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
    pub id: WorkspaceId,
    pub branch_id: BranchId,
    pub layer_stack_id: LayerStackId,
    pub layer_stack_name: EntityName,
    pub branch_name: EntityName,
    pub pinned_head: Option<CommitId>,
    pub placement: WorkspacePlacement,
    pub projection: WorkspaceProjection,
    pub state: WorkspaceState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceSummary {
    pub id: WorkspaceId,
    pub branch_id: BranchId,
    pub layer_stack_id: LayerStackId,
    pub layer_stack_name: EntityName,
    pub branch_name: EntityName,
    pub pinned_head: Option<CommitId>,
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
    pub session_id: WorkspaceId,
    pub dirty: bool,
    pub mutation_generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceFileRangeEdit {
    pub workspace_id: WorkspaceId,
    pub path: String,
    pub start: u64,
    pub delete_len: u64,
    pub replacement: WorkspaceFileReplacement,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkspaceFileReplacement {
    Inline(Vec<u8>),
    Zero(u64),
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

impl ExecutionId {
    #[cfg(unix)]
    pub(crate) fn bytes(self) -> [u8; 16] {
        self.0
    }
}

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
pub struct DaemonTiming {
    pub accept_bind_ns: u64,
    pub decode_ns: u64,
    pub spawn_ns: u64,
    pub runtime_ns: u64,
    pub drain_ns: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ExecutionTransport {
    #[default]
    Host,
    Daemon,
    DockerEngineFallback,
    DockerCliFallback,
    DockerCliInteractive,
}

impl ExecutionTransport {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Daemon => "daemon",
            Self::DockerEngineFallback => "docker-engine",
            Self::DockerCliFallback => "docker-cli",
            Self::DockerCliInteractive => "docker-cli-interactive",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionReceipt {
    pub execution_id: ExecutionId,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub elapsed_ns: u64,
    pub total_wall_ns: u64,
    pub spawn_ns: u64,
    pub supervisor_queue_ns: u64,
    pub runtime_ns: u64,
    pub drain_ns: u64,
    pub terminal_publication_ns: u64,
    pub unattributed_ns: u64,
    pub transport: ExecutionTransport,
    pub daemon_timing: Option<DaemonTiming>,
    pub docker_engine_calls: u64,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
    pub stopped: bool,
}

impl ExecutionReceipt {
    pub fn timing_balanced(&self) -> bool {
        let phases = self
            .spawn_ns
            .checked_add(self.supervisor_queue_ns)
            .and_then(|value| value.checked_add(self.runtime_ns))
            .and_then(|value| value.checked_add(self.drain_ns))
            .and_then(|value| value.checked_add(self.terminal_publication_ns))
            .and_then(|value| value.checked_add(self.unattributed_ns));
        self.elapsed_ns == self.total_wall_ns && phases == Some(self.total_wall_ns)
    }
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
    pub session_id: WorkspaceId,
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
        previous_head: Option<CommitId>,
        commit_id: CommitId,
    },
    UpToDate {
        head: Option<CommitId>,
    },
    CreatedPresentationFailed {
        previous_head: Option<CommitId>,
        commit_id: CommitId,
    },
    UpToDatePresentationFailed {
        head: Option<CommitId>,
    },
    Busy,
    HeadMoved {
        expected: Option<CommitId>,
        actual: Option<CommitId>,
    },
}

impl WorkspaceCommitResult {
    pub(crate) fn from_outcome(
        outcome: layerfs_layerstack_store::CommitOutcome,
        previous_head: Option<CommitId>,
    ) -> Self {
        match outcome {
            layerfs_layerstack_store::CommitOutcome::Committed { commit_id, .. } => Self::Created {
                previous_head,
                commit_id,
            },
            layerfs_layerstack_store::CommitOutcome::UpToDate { .. } => Self::UpToDate {
                head: previous_head,
            },
        }
    }

    pub(crate) fn presentation_failed(self) -> Self {
        match self {
            Self::Created {
                previous_head,
                commit_id,
            } => Self::CreatedPresentationFailed {
                previous_head,
                commit_id,
            },
            Self::UpToDate { head } => Self::UpToDatePresentationFailed { head },
            result => result,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceEndResult {
    pub session_id: WorkspaceId,
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
    InfrastructureLost,
    OutputFailed,
    Storage(StoreError),
    Io(std::io::Error),
}

pub type WorkspaceResult<T> = std::result::Result<T, WorkspaceError>;

impl WorkspaceError {
    pub(crate) fn from_commit(error: StoreError) -> WorkspaceResult<WorkspaceCommitResult> {
        match error {
            StoreError::CommitHeadMoved { expected, actual } => {
                Ok(WorkspaceCommitResult::HeadMoved { expected, actual })
            }
            StoreError::InvalidInput("workspace busy") => Ok(WorkspaceCommitResult::Busy),
            error => Err(Self::Storage(error)),
        }
    }
}

impl From<StoreError> for WorkspaceError {
    fn from(value: StoreError) -> Self {
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
