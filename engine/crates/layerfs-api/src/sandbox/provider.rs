//! Runtime startup uses the same authenticated connection and Store installation as Docker.
use super::{connection::Startup, SandboxApi, SandboxCause, SandboxPhase};
use crate::internal::Control;
use layerfs_bridge::{
    control::{ControlError, DaemonStatus},
    daemon_setup::DaemonSetup,
    duplex::{public_key, ChannelWork},
    provision::StoreManifest,
};
use layerfs_sandbox::{
    provider::{LaunchRequest, ProviderLaunchOptions},
    CommandIdentity, Runtime, Sandbox,
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
    pub installed: Option<crate::Installed>,
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
    pub installed: Option<crate::Installed>,
    pub(crate) control: Control,
    /// Command identity admitted for this Sandbox deployment.
    pub command_identity: CommandIdentity,
}
impl ProviderSandbox {
    /// Explicit startup observation/wait on the same incarnation and Control owner.
    pub(crate) fn status(
        &mut self,
        wait_for_store: bool,
    ) -> Result<DaemonStatus, Box<crate::OperationFailure>> {
        super::create::hello(
            &mut self.control,
            layerfs_bridge::control::HelloRequest {
                expected_instance: Some(self.startup.instance),
                wait_for_store,
            },
        )
    }
}
impl SandboxApi {
    /// Uses the deployment's selected runtime.
    pub fn with_runtime(runtime: Arc<Runtime>) -> Self {
        Self { runtime }
    }
    /// Creates, privately configures, starts and authenticates once, preserving partial ownership on failure.
    pub fn create(
        &self,
        request: ProviderCreate,
        project: &crate::SealedProject,
    ) -> Result<ProviderSandbox, Box<ProviderFailure>> {
        self.create_provider_file(request, &project.store, &project.manifest)
    }
    /// Launches once and authenticates to Hello without Store installation (internal proof only).
    pub(crate) fn start(
        &self,
        request: ProviderCreate,
    ) -> Result<ProviderSandbox, Box<ProviderFailure>> {
        let mut progress = Startup::new();
        let mut launched = None;
        let result = (|| {
            if let Some(directory) =
                self.runtime
                    .local_directory(&request.launch.name)
                    .map_err(|cause| {
                        SandboxCause::Runtime(layerfs_sandbox::WireFailure::local(cause))
                    })?
            {
                if std::path::Path::new(&request.setup.overlay)
                    != directory.join("overlay/overlay.sqlite")
                    || std::path::Path::new(&request.setup.mounts) != directory.join("workspaces")
                    || std::path::Path::new(&request.setup.store).starts_with(&directory)
                    || request.setup.listen != "127.0.0.1:0"
                {
                    return Err(SandboxCause::Setup(ControlError(
                        "provider local resource ownership",
                    )));
                }
            }
            let mut launch = request.launch.clone();
            launch.daemon_config = request.setup.encode().map_err(SandboxCause::Setup)?;
            if let ProviderLaunchOptions::Local {
                command_identity, ..
            } = &launch.options
            {
                if (command_identity.uid, command_identity.gid)
                    != (request.setup.command_uid, request.setup.command_gid)
                {
                    return Err(SandboxCause::Setup(ControlError(
                        "provider topology/identity",
                    )));
                }
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
            launched = Some(match self.runtime.launch(launch) {
                Ok(owner) => Box::new(owner),
                Err(mut failure) => {
                    progress.phase = match failure.phase {
                        layerfs_sandbox::provider::LaunchPhase::Validate => SandboxPhase::Validate,
                        layerfs_sandbox::provider::LaunchPhase::Create => SandboxPhase::Create,
                        layerfs_sandbox::provider::LaunchPhase::Start => SandboxPhase::Start,
                        layerfs_sandbox::provider::LaunchPhase::Listener => SandboxPhase::Listener,
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
            let connect_host = match &request.launch.options {
                ProviderLaunchOptions::Docker { connect_host, .. } => connect_host.as_deref(),
                _ => None,
            };
            progress.connect(
                endpoint,
                connect_host,
                request.launch.startup_timeout,
                &request.controller_private,
                request.daemon_peer,
                request.setup.existing_store.is_some(),
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
                        SandboxCause::Runtime(layerfs_sandbox::WireFailure::local(cause))
                    })?
            {
                if std::path::Path::new(&request.setup.overlay)
                    != directory.join("overlay/overlay.sqlite")
                    || std::path::Path::new(&request.setup.mounts) != directory.join("workspaces")
                    || std::path::Path::new(&request.setup.store).starts_with(&directory)
                    || request.setup.listen != "127.0.0.1:0"
                {
                    return Err(SandboxCause::Setup(ControlError(
                        "provider local resource ownership",
                    )));
                }
            }
            let mut launch = request.launch.clone();
            launch.daemon_config = request.setup.encode().map_err(SandboxCause::Setup)?;
            if let ProviderLaunchOptions::Local {
                command_identity, ..
            } = &launch.options
            {
                if (command_identity.uid, command_identity.gid)
                    != (request.setup.command_uid, request.setup.command_gid)
                {
                    return Err(SandboxCause::Setup(ControlError(
                        "provider topology/identity",
                    )));
                }
            }
            if request
                .setup
                .existing_store
                .as_ref()
                .is_some_and(|m| m.locator != request.setup.store)
                || launch.startup_timeout.is_zero()
                || public_key(&request.controller_private).map_err(SandboxCause::Channel)?
                    != request.setup.control_peer
                || public_key(&request.setup.private_key).map_err(SandboxCause::Channel)?
                    != request.daemon_peer
            {
                return Err(SandboxCause::Setup(ControlError(
                    "provider topology/identity",
                )));
            }
            if let Some(selected) = &request.setup.existing_store {
                let mut expected = manifest.clone();
                expected.daemon_sqlite = selected.daemon_sqlite.clone();
                if *selected != expected {
                    return Err(SandboxCause::Setup(ControlError(
                        "selected existing Project",
                    )));
                }
            }
            launched = Some(match self.runtime.launch(launch) {
                Ok(owner) => Box::new(owner),
                Err(mut failure) => {
                    progress.phase = match failure.phase {
                        layerfs_sandbox::provider::LaunchPhase::Validate => SandboxPhase::Validate,
                        layerfs_sandbox::provider::LaunchPhase::Create => SandboxPhase::Create,
                        layerfs_sandbox::provider::LaunchPhase::Start => SandboxPhase::Start,
                        layerfs_sandbox::provider::LaunchPhase::Listener => SandboxPhase::Listener,
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
            let connect_host = match &request.launch.options {
                ProviderLaunchOptions::Docker { connect_host, .. } => connect_host.as_deref(),
                _ => None,
            };
            let mut status = progress.connect(
                endpoint,
                connect_host,
                request.launch.startup_timeout,
                &request.controller_private,
                request.daemon_peer,
                request.setup.existing_store.is_some(),
            )?;
            if request.setup.existing_store.is_none() {
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
