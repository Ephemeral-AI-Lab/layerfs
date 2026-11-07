//! Runtime startup: one launch, one authenticated connection and the Store's installation, the same for every backend.
use super::{connection::Startup, SandboxApi, SandboxCause, SandboxPhase};
use crate::internal::Control;
use layerfs_bridge::{
    control::{ControlError, DaemonStatus},
    daemon_setup::DaemonSetup,
    duplex::{public_key, ChannelWork},
    provision::StoreManifest,
};
use layerfs_sandbox_backend::{
    Bind, CommandIdentity, LaunchRequest, ProviderLaunchOptions, Runtime, Sandbox,
};
use std::{fmt, sync::Arc};

/// Runtime launch and private daemon authentication inputs.
#[derive(Clone)]
pub struct ProviderCreate {
    /// Runtime launch; configuration is encoded from setup after validation.
    pub launch: LaunchRequest,
    /// Explicit Disposable daemon topology and Store selection.
    pub setup: DaemonSetup,
    /// Controller secret; never printed or sent in argv/environment.
    pub controller_private: [u8; 32],
    /// Expected daemon public identity.
    pub daemon_peer: [u8; 32],
}
impl fmt::Debug for ProviderCreate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderCreate")
            .field("launch", &self.launch)
            .field("setup", &self.setup)
            .field("daemon_peer", &self.daemon_peer)
            .finish_non_exhaustive()
    }
}
/// Exact startup failure and all acknowledged ownership, without automatic cleanup.
pub struct ProviderFailure {
    /// Original private request.
    pub request: ProviderCreate,
    /// Last attempted stage.
    pub phase: SandboxPhase,
    /// Acknowledged runtime, including a partially launched instance.
    pub runtime: Option<Box<Sandbox>>,
    /// Authenticated channel when established.
    pub control: Option<Control>,
    /// Actual handshake work.
    pub handshake: ChannelWork,
    /// Last correlated startup observation.
    pub observed: Option<DaemonStatus>,
    /// Installation acknowledgement even when the following Hello fails.
    pub installed: Option<layerfs_project::store::Installed>,
    /// Original deciding cause.
    pub cause: SandboxCause,
}
impl fmt::Debug for ProviderFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderFailure")
            .field("phase", &self.phase)
            .field("runtime", &self.runtime.as_ref().map(|r| r.identity()))
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
impl fmt::Display for ProviderFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sandbox {:?}: {:?}", self.phase, self.cause)
    }
}
impl std::error::Error for ProviderFailure {}
/// Runtime owner and its original authenticated daemon channel.
pub struct ProviderSandbox {
    /// Original private launch and daemon configuration inputs.
    pub request: ProviderCreate,
    /// Explicit runtime lifecycle owner.
    pub runtime: Box<Sandbox>,
    /// Original correlated startup observation.
    pub startup: DaemonStatus,
    /// Actual handshake work.
    pub handshake: ChannelWork,
    /// Store installation acknowledgement, if installed by this create.
    pub installed: Option<layerfs_project::store::Installed>,
    pub control: Control,
    /// Command identity admitted for this Sandbox deployment.
    pub command_identity: CommandIdentity,
}
impl SandboxApi {
    /// Uses the deployment's selected runtime.
    pub fn with_runtime(runtime: Arc<Runtime>) -> Self {
        Self { runtime }
    }
    /// Creates a new daemon on the PostgreSQL Store its setup selects
    /// (#506): no Store file, bind or install; it starts ready: a native
    /// (macOS) daemon process, or a namespaces root with no bind (#535 §7.4).
    pub fn create_provider_postgres(
        &self,
        request: ProviderCreate,
    ) -> Result<ProviderSandbox, Box<ProviderFailure>> {
        if !matches!(
            request.setup.store,
            layerfs_bridge::DaemonStore::Postgres { .. }
        ) {
            return Err(Box::new(ProviderFailure {
                request,
                phase: SandboxPhase::Validate,
                runtime: None,
                control: None,
                handshake: ChannelWork::default(),
                observed: None,
                installed: None,
                cause: SandboxCause::Setup(ControlError("PostgreSQL Store with no volume")),
            }));
        }
        self.start(request)
    }
    /// Launches once and authenticates to Hello without Store installation (internal proof only).
    pub fn start(&self, request: ProviderCreate) -> Result<ProviderSandbox, Box<ProviderFailure>> {
        let mut progress = Startup::new();
        let mut launched = None;
        let result = (|| {
            if let Some(directory) =
                self.runtime
                    .local_directory(&request.launch.name)
                    .map_err(|cause| {
                        SandboxCause::Runtime(layerfs_sandbox_backend::WireFailure::local(cause))
                    })?
            {
                if std::path::Path::new(&request.setup.overlay)
                    != directory.join("overlay/overlay.sqlite")
                    || std::path::Path::new(&request.setup.mounts) != directory.join("workspaces")
                    || matches!(
                        &request.setup.store,
                        layerfs_bridge::DaemonStore::Sqlite { locator, .. }
                            if std::path::Path::new(locator).starts_with(&directory)
                    )
                    || request.setup.listen != "127.0.0.1:0"
                {
                    return Err(SandboxCause::Setup(ControlError(
                        "provider local resource ownership",
                    )));
                }
            }
            let mut launch = request.launch.clone();
            launch.daemon_config = request.setup.encode().map_err(SandboxCause::Setup)?;
            let (ProviderLaunchOptions::Local {
                command_identity, ..
            }
            | ProviderLaunchOptions::Namespaces {
                command_identity, ..
            }) = &launch.options;
            if (command_identity.uid, command_identity.gid)
                != (request.setup.command_uid, request.setup.command_gid)
            {
                return Err(SandboxCause::Setup(ControlError(
                    "provider topology/identity",
                )));
            }
            if launch.startup_timeout.is_zero()
                || public_key(&request.controller_private).map_err(SandboxCause::Channel)?
                    != request.setup.control_peer
                || public_key(&request.setup.private_key).map_err(SandboxCause::Channel)?
                    != request.daemon_peer
            {
                return Err(SandboxCause::Setup(ControlError(
                    "provider topology/identity",
                )));
            }
            let binds = binds(&launch, &request.setup.store)?;
            launched = Some(match self.runtime.launch_with_binds(launch, binds) {
                Ok(owner) => Box::new(owner),
                Err(mut failure) => {
                    progress.phase = match failure.phase {
                        layerfs_sandbox_backend::LaunchPhase::Validate => SandboxPhase::Validate,
                        layerfs_sandbox_backend::LaunchPhase::Create => SandboxPhase::Create,
                        layerfs_sandbox_backend::LaunchPhase::Start => SandboxPhase::Start,
                        layerfs_sandbox_backend::LaunchPhase::Listener => SandboxPhase::Listener,
                    };
                    launched = failure.instance.take();
                    return Err(SandboxCause::Provider(failure));
                }
            });
            progress.phase = SandboxPhase::Endpoint;
            let endpoint = launched
                .as_ref()
                .expect("launched")
                .endpoint()
                .map_err(SandboxCause::Runtime)?;
            progress.connect(
                endpoint,
                request.launch.startup_timeout,
                &request.controller_private,
                request.daemon_peer,
                request.setup.opens_existing(),
            )
        })();
        match result {
            Ok(startup) => Ok(ProviderSandbox {
                request: request.clone(),
                runtime: launched.expect("launched"),
                startup,
                installed: None,
                handshake: progress.handshake,
                control: progress.control.expect("authenticated"),
                command_identity: CommandIdentity {
                    uid: request.setup.command_uid,
                    gid: request.setup.command_gid,
                },
            }),
            Err(cause) => Err(Box::new(ProviderFailure {
                request,
                phase: progress.phase,
                runtime: launched,
                control: progress.control,
                handshake: progress.handshake,
                observed: progress.observed,
                installed: None,
                cause,
            })),
        }
    }
    /// Launches once, authenticates, selects or installs the Store, then observes Ready.
    pub fn create_provider_file(
        &self,
        request: ProviderCreate,
        store: &layerfs_persistence::SealedStore,
        manifest: &StoreManifest,
    ) -> Result<ProviderSandbox, Box<ProviderFailure>> {
        let mut progress = Startup::new();
        let mut launched = None;
        let mut installed = None;
        let result = (|| {
            if let Some(directory) =
                self.runtime
                    .local_directory(&request.launch.name)
                    .map_err(|cause| {
                        SandboxCause::Runtime(layerfs_sandbox_backend::WireFailure::local(cause))
                    })?
            {
                if std::path::Path::new(&request.setup.overlay)
                    != directory.join("overlay/overlay.sqlite")
                    || std::path::Path::new(&request.setup.mounts) != directory.join("workspaces")
                    || matches!(
                        &request.setup.store,
                        layerfs_bridge::DaemonStore::Sqlite { locator, .. }
                            if std::path::Path::new(locator).starts_with(&directory)
                    )
                    || request.setup.listen != "127.0.0.1:0"
                {
                    return Err(SandboxCause::Setup(ControlError(
                        "provider local resource ownership",
                    )));
                }
            }
            let mut launch = request.launch.clone();
            launch.daemon_config = request.setup.encode().map_err(SandboxCause::Setup)?;
            let (ProviderLaunchOptions::Local {
                command_identity, ..
            }
            | ProviderLaunchOptions::Namespaces {
                command_identity, ..
            }) = &launch.options;
            if (command_identity.uid, command_identity.gid)
                != (request.setup.command_uid, request.setup.command_gid)
            {
                return Err(SandboxCause::Setup(ControlError(
                    "provider topology/identity",
                )));
            }
            if launch.startup_timeout.is_zero()
                || public_key(&request.controller_private).map_err(SandboxCause::Channel)?
                    != request.setup.control_peer
                || public_key(&request.setup.private_key).map_err(SandboxCause::Channel)?
                    != request.daemon_peer
            {
                return Err(SandboxCause::Setup(ControlError(
                    "provider topology/identity",
                )));
            }
            if let layerfs_bridge::DaemonStore::Sqlite {
                install: Some(selected),
                ..
            } = &request.setup.store
            {
                let mut expected = manifest.clone();
                expected.daemon_sqlite = selected.daemon_sqlite.clone();
                if *selected != expected {
                    return Err(SandboxCause::Setup(ControlError(
                        "selected existing Project",
                    )));
                }
            }
            let binds = binds(&launch, &request.setup.store)?;
            launched = Some(match self.runtime.launch_with_binds(launch, binds) {
                Ok(owner) => Box::new(owner),
                Err(mut failure) => {
                    progress.phase = match failure.phase {
                        layerfs_sandbox_backend::LaunchPhase::Validate => SandboxPhase::Validate,
                        layerfs_sandbox_backend::LaunchPhase::Create => SandboxPhase::Create,
                        layerfs_sandbox_backend::LaunchPhase::Start => SandboxPhase::Start,
                        layerfs_sandbox_backend::LaunchPhase::Listener => SandboxPhase::Listener,
                    };
                    launched = failure.instance.take();
                    return Err(SandboxCause::Provider(failure));
                }
            });
            progress.phase = SandboxPhase::Endpoint;
            let endpoint = launched
                .as_ref()
                .expect("launched")
                .endpoint()
                .map_err(SandboxCause::Runtime)?;
            let mut status = progress.connect(
                endpoint,
                request.launch.startup_timeout,
                &request.controller_private,
                request.daemon_peer,
                request.setup.opens_existing(),
            )?;
            if !request.setup.opens_existing() {
                let mut phase = SandboxPhase::Install;
                let result = super::connection::install(
                    progress.control.as_mut().expect("authenticated"),
                    &mut status,
                    &mut installed,
                    &mut phase,
                    store,
                    manifest,
                );
                progress.phase = phase;
                progress.observed = Some(status.clone());
                result?;
            }
            Ok(status)
        })();
        match result {
            Ok(startup) => Ok(ProviderSandbox {
                request: request.clone(),
                runtime: launched.expect("launched"),
                startup,
                installed,
                handshake: progress.handshake,
                control: progress.control.expect("authenticated"),
                command_identity: CommandIdentity {
                    uid: request.setup.command_uid,
                    gid: request.setup.command_gid,
                },
            }),
            Err(cause) => Err(Box::new(ProviderFailure {
                request,
                phase: progress.phase,
                runtime: launched,
                control: progress.control,
                handshake: progress.handshake,
                observed: progress.observed,
                installed,
                cause,
            })),
        }
    }
}

/// The binds a namespaces daemon's private root takes (#535 §6.3): its
/// sqlite Store file's directory, or none for a PostgreSQL Store, whose
/// daemon dials the server. Other backends take none.
fn binds(
    launch: &LaunchRequest,
    store: &layerfs_bridge::DaemonStore,
) -> Result<Vec<Bind>, SandboxCause> {
    match (&launch.options, store) {
        (
            ProviderLaunchOptions::Namespaces { .. },
            layerfs_bridge::DaemonStore::Sqlite { locator, .. },
        ) => Bind::store(std::path::Path::new(locator))
            .map(|bind| vec![bind])
            .map_err(|cause| {
                SandboxCause::Runtime(layerfs_sandbox_backend::WireFailure::local(cause))
            }),
        _ => Ok(Vec::new()),
    }
}
