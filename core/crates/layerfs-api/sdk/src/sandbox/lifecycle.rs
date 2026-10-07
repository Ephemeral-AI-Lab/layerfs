//! Concrete once-only resource and authenticated startup composition.
use super::{
    api::hello, ManagedSandbox, SandboxCause, SandboxCreate, SandboxFailure, SandboxPhase,
};
use crate::control::Control;
use layerfs_bridge::{
    control::{ControlError, DaemonPhase, HelloRequest},
    native::{initiate_observed, public_key, ChannelWork},
};
use layerfs_sandbox::backend::docker::Docker;
use std::{fs::File, net::TcpStream};
pub(super) fn create(
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
        let stream = TcpStream::connect_timeout(&endpoint, request.startup_wait)
            .map_err(SandboxCause::Io)?;
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
        // Bridge owns cloned native directions; clear policy through that actual owner.
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
            cause,
        })),
    }
}
