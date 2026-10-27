//! Ordinary Sandbox startup, original deployment inputs and failure custody.
use crate::{internal::Control, OperationFailure};
use layerfs_bridge::{
    control::{ControlError, DaemonPhase, DaemonStatus, HelloRequest, Reply, Request},
    daemon_setup::DaemonSetup,
    duplex::{initiate_observed, public_key, ChannelError, ChannelWork},
};
use layerfs_sandbox::backend::docker::{
    Docker, ListenerFailure, Sandbox, SandboxCreateFailure, SandboxRequest, UploadFailure,
};
use std::{fmt, fs::File, io, net::TcpStream, path::PathBuf, time::Duration};
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
    /// Host to dial for the daemon's loopback-published port, when this
    /// process does not share the Docker host's loopback (a client in a
    /// container: `host.docker.internal`). `None` dials the observed
    /// 127.0.0.1 endpoint. The publish and its check are unchanged.
    pub connect_host: Option<String>,
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
            .field("connect_host", &self.connect_host)
            .finish()
    }
}
/// The address to dial for a daemon `endpoint` observed on the Docker host's
/// loopback: the endpoint itself, or the port on `host` (one resolution, its
/// first address; nothing is tried twice).
pub fn connect_address(
    endpoint: std::net::SocketAddr,
    host: Option<&str>,
) -> io::Result<std::net::SocketAddr> {
    let Some(host) = host else {
        return Ok(endpoint);
    };
    std::net::ToSocketAddrs::to_socket_addrs(&(host, endpoint.port()))?
        .next()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("daemon host {host} has no address"),
            )
        })
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
    /// Original streaming installation failure and exact publication knowledge.
    Install(Box<crate::InstallFailure>),
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
    /// Installation acknowledgement, retained if the following Ready observation fails.
    pub installed: Option<crate::Installed>,
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
impl fmt::Display for SandboxFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sandbox {:?}: {:?}", self.phase, self.cause)
    }
}
impl std::error::Error for SandboxFailure {}
/// Actual local Engine and authenticated daemon lifecycle organization.
#[derive(Clone, Debug)]
pub struct SandboxApi {
    pub(crate) docker: Docker,
}
/// Exact acknowledged container and authenticated control owner.
pub struct ManagedSandbox {
    /// Owned runtime container; explicit stop/delete only.
    pub runtime: Sandbox,
    /// Existing sequential control owner; correlation and failed state are never reset.
    pub(crate) control: Control,
    /// Original acknowledged installation, when this create installed the Project.
    pub installed: Option<crate::Installed>,
    pub(crate) request: SandboxCreate,
    /// Original correlated startup status; this is not kernel Workspace Ready.
    pub startup: DaemonStatus,
    /// Actual original duplex handshake work.
    pub handshake: ChannelWork,
    pub(crate) command_identity: layerfs_sandbox::CommandIdentity,
}
impl SandboxApi {
    /// Uses one explicit local Engine endpoint and metadata wait policy.
    pub fn new(docker: Docker) -> Self {
        Self { docker }
    }
    /// Creates, privately configures, starts and authenticates once, preserving partial ownership on failure.
    pub fn create(
        &self,
        request: SandboxCreate,
        project: &crate::SealedProject,
    ) -> Result<ManagedSandbox, Box<SandboxFailure>> {
        self.create_file(request, &project.store, &project.manifest)
    }
    /// Creates a new daemon using exact persisted sealed-file and manifest facts.
    pub fn create_file(
        &self,
        request: SandboxCreate,
        store: &layerfs_persistence::SealedStore,
        manifest: &layerfs_bridge::provision::StoreManifest,
    ) -> Result<ManagedSandbox, Box<SandboxFailure>> {
        let existing = request.setup.existing_store.is_some();
        if let Some(selected) = &request.setup.existing_store {
            let mut expected = manifest.clone();
            expected.daemon_sqlite = selected.daemon_sqlite.clone();
            if *selected != expected {
                return Err(Box::new(SandboxFailure {
                    request,
                    phase: SandboxPhase::Validate,
                    sandbox: None,
                    control: None,
                    handshake: ChannelWork::default(),
                    observed: None,
                    installed: None,
                    cause: SandboxCause::Setup(ControlError("selected existing Project")),
                }));
            }
        }
        let mut owner = create(&self.docker, request)?;
        if existing {
            // The original startup already selected and waited for this Store.
            // Installing again would change the existing one-attempt lifecycle.
            return Ok(owner);
        }
        let installed = owner
            .control
            .install_file(store, manifest)
            .map_err(|cause| {
                Box::new(SandboxFailure {
                    request: owner.request.clone(),
                    phase: SandboxPhase::Install,
                    sandbox: None,
                    control: None,
                    handshake: owner.handshake,
                    observed: Some(owner.startup.clone()),
                    installed: None,
                    cause: SandboxCause::Install(cause),
                })
            });
        let installed = match installed {
            Ok(installed) => installed,
            Err(mut failure) => {
                failure.sandbox = Some(owner.runtime);
                failure.control = Some(owner.control);
                return Err(failure);
            }
        };
        owner.installed = Some(installed);
        match hello(
            &mut owner.control,
            HelloRequest {
                expected_instance: Some(owner.startup.instance),
                wait_for_store: true,
            },
        ) {
            Ok(status) if status.phase == DaemonPhase::ControlReady => {
                owner.startup = status;
                Ok(owner)
            }
            Ok(status) => Err(Box::new(SandboxFailure {
                request: owner.request,
                phase: SandboxPhase::Hello,
                sandbox: Some(owner.runtime),
                control: Some(owner.control),
                handshake: owner.handshake,
                observed: Some(status),
                installed: owner.installed,
                cause: SandboxCause::Setup(ControlError("original installed startup phase")),
            })),
            Err(cause) => Err(Box::new(SandboxFailure {
                request: owner.request,
                phase: SandboxPhase::Hello,
                sandbox: Some(owner.runtime),
                control: Some(owner.control),
                handshake: owner.handshake,
                observed: Some(owner.startup),
                installed: owner.installed,
                cause: SandboxCause::Control(cause),
            })),
        }
    }
}
impl ManagedSandbox {
    /// Explicit startup observation/wait on the same incarnation and Control owner.
    /// A later observation never resolves an earlier unknown operation.
    pub(crate) fn status(
        &mut self,
        wait_for_store: bool,
    ) -> Result<DaemonStatus, Box<OperationFailure>> {
        hello(
            &mut self.control,
            HelloRequest {
                expected_instance: Some(self.startup.instance),
                wait_for_store,
            },
        )
    }
}
pub(super) fn hello(
    control: &mut Control,
    request: HelloRequest,
) -> Result<DaemonStatus, Box<OperationFailure>> {
    let request = Request::Hello(request);
    match crate::base::failure::exchange(control, request.clone())? {
        Reply::Hello(status) => Ok(status),
        reply => Err(OperationFailure::unexpected(request, reply)),
    }
}
pub(crate) fn create(
    docker: &Docker,
    request: SandboxCreate,
) -> Result<ManagedSandbox, Box<SandboxFailure>> {
    let mut phase = SandboxPhase::Validate;
    let mut sandbox = None;
    let mut control = None;
    let mut handshake = ChannelWork::default();
    let mut observed = None;
    let result = (|| {
        let config = request.setup.encode().map_err(SandboxCause::Setup)?;
        if request.setup.listen != format!("0.0.0.0:{}", request.deployment.port)
            || request.setup.store != "/layerfs-store/global/store.sqlite"
            || request.setup.overlay != "/layerfs-local/overlay/overlay.sqlite"
            || request.setup.mounts != "/workspaces"
            || request.startup_wait.is_zero()
            || public_key(&request.controller_private).map_err(SandboxCause::Channel)?
                != request.setup.control_peer
            || public_key(&request.setup.private_key).map_err(SandboxCause::Channel)?
                != request.daemon_peer
        {
            return Err(SandboxCause::Setup(ControlError(
                "concrete deployment topology/identity",
            )));
        }
        phase = SandboxPhase::Executable;
        let mut file = File::open(&request.executable).map_err(SandboxCause::Io)?;
        phase = SandboxPhase::Create;
        sandbox = Some(
            docker
                .create_sandbox(request.deployment.clone())
                .map_err(SandboxCause::Create)?,
        );
        let owner = sandbox.as_mut().expect("acknowledged container");
        phase = SandboxPhase::Upload;
        owner
            .upload(&mut file, &config)
            .map_err(SandboxCause::Upload)?;
        phase = SandboxPhase::Start;
        owner.start().map_err(SandboxCause::Runtime)?;
        phase = SandboxPhase::Listener;
        owner
            .wait_listener(request.startup_wait)
            .map_err(SandboxCause::Listener)?;
        phase = SandboxPhase::Endpoint;
        let endpoint = owner.endpoint().map_err(SandboxCause::Runtime)?;
        phase = SandboxPhase::Connect;
        let address =
            connect_address(endpoint, request.connect_host.as_deref()).map_err(SandboxCause::Io)?;
        let stream =
            TcpStream::connect_timeout(&address, request.startup_wait).map_err(SandboxCause::Io)?;
        stream
            .set_read_timeout(Some(request.startup_wait))
            .map_err(SandboxCause::Io)?;
        stream
            .set_write_timeout(Some(request.startup_wait))
            .map_err(SandboxCause::Io)?;
        phase = SandboxPhase::Authenticate;
        let start = initiate_observed(stream, &request.controller_private, request.daemon_peer);
        handshake = start.work;
        let connection = start.result.map_err(SandboxCause::Channel)?;
        // The startup handshake waits are cleared before control/product operations.
        // Bridge owns cloned duplex directions; clear policy through that actual owner.
        control = Some(Control::new(connection));
        phase = SandboxPhase::Policy;
        control
            .as_ref()
            .expect("authenticated")
            .clear_io_waits()
            .map_err(SandboxCause::Channel)?;
        phase = SandboxPhase::Hello;
        let status = hello(
            control.as_mut().expect("authenticated"),
            HelloRequest {
                expected_instance: None,
                wait_for_store: request.setup.existing_store.is_some(),
            },
        )
        .map_err(SandboxCause::Control)?;
        let expected = if request.setup.existing_store.is_some() {
            DaemonPhase::ControlReady
        } else {
            DaemonPhase::InstallPending
        };
        observed = Some(status.clone());
        if status.phase != expected {
            return Err(SandboxCause::Setup(ControlError("original startup phase")));
        }
        Ok(status)
    })();
    match result {
        Ok(startup) => Ok(ManagedSandbox {
            request: request.clone(),
            installed: None,
            runtime: sandbox.take().expect("owned"),
            control: control.take().expect("authenticated"),
            startup,
            handshake,
            command_identity: layerfs_sandbox::CommandIdentity {
                uid: request.setup.command_uid,
                gid: request.setup.command_gid,
            },
        }),
        Err(cause) => Err(Box::new(SandboxFailure {
            request,
            phase,
            sandbox,
            control,
            handshake,
            observed,
            installed: None,
            cause,
        })),
    }
}
