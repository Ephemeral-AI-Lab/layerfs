//! Original deployment inputs and failure custody; private keys stay out of diagnostics.
use crate::{control::Control, OperationFailure};
use layerfs_bridge::{
    control::{ControlError, DaemonStatus},
    daemon_setup::DaemonSetup,
    native::{ChannelError, ChannelWork},
};
use layerfs_sandbox::backend::docker::{
    ListenerFailure, Sandbox, SandboxCreateFailure, SandboxRequest, UploadFailure,
};
use std::{fmt, io, path::PathBuf, time::Duration};
/// One actual stopped-container deployment, private archive and authenticated startup.
#[derive(Clone)]
pub struct SandboxCreate {
    /// Exact immutable image and borrowed shared named volume.
    pub deployment: SandboxRequest,
    /// Explicit private daemon resources and Disposable install/open selection.
    pub setup: DaemonSetup,
    /// Sealed Linux executable supplied by the caller; streamed once, never read as Store data.
    pub executable: PathBuf,
    /// Controller Noise secret; redacted from Debug and never sent in env/argv.
    pub controller_private: [u8; 32],
    /// Expected actual daemon identity, checked against private deployment config.
    pub daemon_peer: [u8; 32],
    /// Explicit listener event wait; does not apply to ordinary commands.
    pub startup_wait: Duration,
}
impl fmt::Debug for SandboxCreate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SandboxCreate")
            .field("deployment", &self.deployment)
            .field("setup", &self.setup)
            .field("executable", &self.executable)
            .field("controller_private", &"[redacted]")
            .field("daemon_peer", &self.daemon_peer)
            .field("startup_wait", &self.startup_wait)
            .finish()
    }
}
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
    /// One expected-peer native authentication.
    Authenticate,
    /// Clears only explicit handshake waits after authentication; owner remains retained on failure.
    Policy,
    /// One correlated Hello through the retained Control owner.
    Hello,
}
/// Exact original failure, independent of owned resource disposition.
#[derive(Debug)]
pub enum SandboxCause {
    /// Local filesystem/socket error.
    Io(io::Error),
    /// Local config or correlated startup validation.
    Setup(ControlError),
    /// Actual original runtime Create including partial decoded ID.
    Create(Box<SandboxCreateFailure>),
    /// Actual incremental archive extraction failure and work.
    Upload(Box<UploadFailure>),
    /// Original listener wait/progress/fence failure.
    Listener(Box<ListenerFailure>),
    /// Original Engine transfer, no replay.
    Runtime(Box<layerfs_sandbox::WireFailure>),
    /// Original authentication failure; handshake work remains in the receipt.
    Channel(ChannelError),
    /// Original correlated daemon control exchange/refusal.
    Control(Box<OperationFailure>),
}
/// Caller retains exact acknowledged owner, connection and original deployment inputs.
pub struct SandboxFailure {
    /// Original inputs with secrets retained privately.
    pub request: SandboxCreate,
    /// Original reached boundary.
    pub phase: SandboxPhase,
    /// Exact acknowledged owned container; no automatically adopted partial ID.
    pub sandbox: Option<Sandbox>,
    /// Original authenticated control owner, if established.
    pub control: Option<Control>,
    /// Actual original handshake work, including failure.
    pub handshake: ChannelWork,
    /// Original correlated Hello, even if its phase failed selection validation.
    pub observed: Option<DaemonStatus>,
    /// Exact original cause.
    pub cause: SandboxCause,
}
impl fmt::Debug for SandboxFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SandboxFailure")
            .field("request", &self.request)
            .field("phase", &self.phase)
            .field("sandbox", &self.sandbox)
            .field("has_control", &self.control.is_some())
            .field("handshake", &self.handshake)
            .field("observed", &self.observed)
            .field("cause", &self.cause)
            .finish()
    }
}
