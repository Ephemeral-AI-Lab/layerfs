//! One shared TCP/Noise/Hello pipeline, retaining progress through every failure.
use super::create::{hello, SandboxCause, SandboxPhase};
use crate::internal::Control;
use layerfs_bridge::{
    control::{ControlError, DaemonPhase, DaemonStatus, HelloRequest},
    duplex::{initiate_observed, ChannelWork},
};
use std::{
    net::{SocketAddr, TcpStream},
    time::Duration,
};

pub(super) struct Startup {
    pub phase: SandboxPhase,
    pub control: Option<Control>,
    pub handshake: ChannelWork,
    pub observed: Option<DaemonStatus>,
}
impl Startup {
    pub fn new() -> Self {
        Self {
            phase: SandboxPhase::Validate,
            control: None,
            handshake: ChannelWork::default(),
            observed: None,
        }
    }
    pub fn connect(
        &mut self,
        endpoint: SocketAddr,
        host: Option<&str>,
        wait: Duration,
        secret: &[u8; 32],
        peer: [u8; 32],
        existing: bool,
    ) -> Result<DaemonStatus, SandboxCause> {
        self.phase = SandboxPhase::Connect;
        let address = super::create::connect_address(endpoint, host).map_err(SandboxCause::Io)?;
        let stream = TcpStream::connect_timeout(&address, wait).map_err(SandboxCause::Io)?;
        stream
            .set_read_timeout(Some(wait))
            .map_err(SandboxCause::Io)?;
        stream
            .set_write_timeout(Some(wait))
            .map_err(SandboxCause::Io)?;
        self.phase = SandboxPhase::Authenticate;
        let start = initiate_observed(stream, secret, peer);
        self.handshake = start.work;
        self.control = Some(Control::new(start.result.map_err(SandboxCause::Channel)?));
        self.phase = SandboxPhase::Policy;
        self.control
            .as_ref()
            .expect("authenticated")
            .clear_io_waits()
            .map_err(SandboxCause::Channel)?;
        self.phase = SandboxPhase::Hello;
        let status = hello(
            self.control.as_mut().expect("authenticated"),
            HelloRequest {
                expected_instance: None,
                wait_for_store: existing,
            },
        )
        .map_err(SandboxCause::Control)?;
        self.observed = Some(status.clone());
        let expected = if existing {
            DaemonPhase::ControlReady
        } else {
            DaemonPhase::InstallPending
        };
        if status.phase != expected {
            return Err(SandboxCause::Setup(ControlError("original startup phase")));
        }
        Ok(status)
    }
}
/// Installation and its Ready observation are identical for every runtime.
pub(super) fn install(
    control: &mut Control,
    startup: &mut DaemonStatus,
    installed: &mut Option<crate::Installed>,
    phase: &mut SandboxPhase,
    store: &layerfs_persistence::SealedStore,
    manifest: &layerfs_bridge::provision::StoreManifest,
) -> Result<(), SandboxCause> {
    *phase = SandboxPhase::Install;
    *installed = Some(
        control
            .install_file(store, manifest)
            .map_err(SandboxCause::Install)?,
    );
    *phase = SandboxPhase::Hello;
    let status = hello(
        control,
        HelloRequest {
            expected_instance: Some(startup.instance),
            wait_for_store: true,
        },
    )
    .map_err(SandboxCause::Control)?;
    *startup = status;
    if startup.phase != DaemonPhase::ControlReady {
        return Err(SandboxCause::Setup(ControlError(
            "original installed startup phase",
        )));
    }
    Ok(())
}
