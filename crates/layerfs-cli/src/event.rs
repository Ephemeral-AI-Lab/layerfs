#[derive(Debug)]
pub enum CliError {
    Parse(String),
    Context(String),
    Sdk(layerfs_sdk::SdkError),
    Storage(layerfs_sdk::StorageError),
    Workspace(layerfs_sdk::WorkspaceError),
    Io(std::io::Error),
    Operation {
        context: String,
        source: Box<CliError>,
    },
    Interrupted,
}

pub type CliResult<T> = std::result::Result<T, CliError>;

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Operation { context, source } => write!(formatter, "{context}: {source}"),
            _ => write!(formatter, "{self:?}"),
        }
    }
}

impl std::error::Error for CliError {}

impl From<layerfs_sdk::SdkError> for CliError {
    fn from(value: layerfs_sdk::SdkError) -> Self {
        Self::Sdk(value)
    }
}

impl From<layerfs_sdk::StorageError> for CliError {
    fn from(value: layerfs_sdk::StorageError) -> Self {
        Self::Storage(value)
    }
}

impl From<layerfs_sdk::WorkspaceError> for CliError {
    fn from(value: layerfs_sdk::WorkspaceError) -> Self {
        Self::Workspace(value)
    }
}

impl From<std::io::Error> for CliError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Clone, Debug)]
pub enum CommandResult {
    Empty,
    Text(String),
    Query(layerfs_sdk::QueryPage),
    Monitor(layerfs_sdk::MonitorSnapshot),
    Dedup(layerfs_sdk::DedupAnalysis),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationPhase {
    Parse,
    Execute,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProgressValue {
    pub current: u64,
    pub total: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSummary {
    pub name: String,
}

#[derive(Debug)]
pub enum CliEvent {
    Started(CommandSummary),
    Progress {
        phase: OperationPhase,
        value: ProgressValue,
    },
    Output(Vec<u8>),
    Diff(layerfs_sdk::DiffPage),
    Snapshot(layerfs_sdk::QueryPage),
    Finished(CliResult<CommandResult>),
}
