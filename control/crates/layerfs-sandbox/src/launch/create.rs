//! Ordinary Sandbox startup, original deployment inputs and failure custody.
use crate::Runtime;
use crate::{internal::Control, OperationFailure};
use layerfs_protocol::daemon::{DaemonStatus, HelloRequest, Reply, Request};
use layerfs_protocol::duplex::ChannelError;
use layerfs_protocol::provision::install::ControlError;
use std::{fmt, io, sync::Arc};

/// Concrete boundary reached by one original lifecycle operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SandboxPhase {
    /// Local validation, no container effect.
    Validate,
    /// Caller executable open/metadata.
    Executable,
    /// Volume observation and original Create.
    Create,
    /// Original stopped-container archive extraction.
    Upload,
    /// Original container Start.
    Start,
    /// Actual original stdout listener event wait.
    Listener,
    /// Exact container endpoint observation.
    Endpoint,
    /// One TCP connection attempt.
    Connect,
    /// One expected-peer duplex authentication.
    Authenticate,
    /// Clears only explicit handshake waits after authentication; owner remains retained on failure.
    Policy,
    /// One correlated Hello through the retained Control owner.
    Hello,
    /// One original sealed Project installation.
    Install,
    /// One explicit whole-Sandbox stop attempt.
    Stop,
    /// One delete attempt after acknowledged stop.
    Delete,
}

/// Exact original failure, independent of owned resource disposition.
#[derive(Debug)]
pub enum SandboxCause {
    /// Local filesystem/socket error.
    Io(io::Error),
    /// Local config or correlated startup validation.
    Setup(ControlError),
    /// Original Engine transfer, no replay.
    Runtime(Box<crate::WireFailure>),
    /// Original provider launch cause; partial runtime ownership is retained separately.
    Provider(Box<crate::LaunchFailure>),
    /// Original authentication failure; handshake work remains in the receipt.
    Channel(ChannelError),
    /// Original correlated daemon control exchange/refusal.
    Control(Box<OperationFailure>),
    /// Original streaming installation failure and exact publication knowledge.
    Install(Box<layerfs_project::store::InstallFailure>),
}

impl SandboxPhase {
    /// Where a start stopped, in plain words.
    pub fn words(self) -> &'static str {
        match self {
            Self::Validate => "while its launch was checked",
            Self::Executable => "while its daemon program was opened",
            Self::Create => "while its daemon was created",
            Self::Upload => "while its daemon's files were copied in",
            Self::Start => "while its daemon was started",
            Self::Listener => "before its daemon said it was listening",
            Self::Endpoint => "while its daemon's address was read",
            Self::Connect => "while this side connected to its daemon",
            Self::Authenticate => "while its daemon's identity was checked",
            Self::Policy => "after its daemon's identity was checked",
            Self::Hello => "at its daemon's first answer",
            Self::Install => "while the Store was installed into it",
            Self::Stop => "while it was stopped",
            Self::Delete => "while it was deleted",
        }
    }
}

/// In plain words, never `Debug` text.
impl fmt::Display for SandboxCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use crate::RuntimeError;
        use layerfs_project::store::InstallError;
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Setup(error) => write!(f, "{error}"),
            Self::Runtime(failure) => write!(f, "{failure}"),
            Self::Provider(failure) => match &failure.cause {
                RuntimeError::Io(error) => write!(f, "{error}"),
                RuntimeError::Protocol(said) => f.write_str(said),
                RuntimeError::Json(said) => f.write_str(said),
                RuntimeError::Unsupported(what) => write!(f, "not supported here: {what}"),
                RuntimeError::Unavailable(why) => f.write_str(why),
            },
            Self::Channel(error) => write!(f, "{error}"),
            Self::Control(failure) => write!(f, "{failure}"),
            Self::Install(failure) => match &failure.error {
                InstallError::Io(error) => write!(f, "the Store's file: {error}"),
                InstallError::Channel(error) => write!(f, "{error}"),
                InstallError::Protocol(error) => write!(f, "{error}"),
                InstallError::Remote(_) => f.write_str("its daemon refused the Store"),
            },
        }
    }
}

/// Actual local Engine and authenticated daemon lifecycle organization.
#[derive(Clone)]
pub struct SandboxApi {
    pub runtime: Arc<Runtime>,
}

pub fn hello(
    control: &mut Control,
    request: HelloRequest,
) -> Result<DaemonStatus, Box<OperationFailure>> {
    let request = Request::Hello(request);
    match crate::daemon::failure::exchange(control, request.clone())? {
        Reply::Hello(status) => Ok(status),
        reply => Err(OperationFailure::unexpected(request, reply)),
    }
}
