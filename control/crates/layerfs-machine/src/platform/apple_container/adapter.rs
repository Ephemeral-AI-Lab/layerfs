//! `apple-container`'s adapter.
use super::{create, guest, remove, start, status, stop};
use crate::platform::adapter::Adapter;
use crate::platform::error::{Code, MachineError};
use crate::setup::forward::refuse;
use crate::setup::guest::Guest;
use crate::setup::probe::{macos_missing, tool_version};
use crate::setup::spec::{Bundle, Forward, NameLimit, Running, Spec, Tools};
use crate::setup::Status;
use crate::table::profile::HostOs;
use crate::table::reach::Loopback;

/// Why a Sandbox daemon's port is not forwarded: `container run --publish`
/// (`create.rs`) is the only way the CLI publishes a port, so the worker's
/// is the only one, and a later port needs a mechanism LayerFS does not have.
const FORWARD: &str = "the container CLI publishes a port only when the machine's container is created, so a Sandbox daemon's port cannot be published later; its Workspace calls go through the worker";

pub(crate) struct AppleContainer;
pub(crate) static ADAPTER: AppleContainer = AppleContainer;

impl Adapter for AppleContainer {
    fn probe(&self, tools: &Tools, host: HostOs, missing: &mut Vec<String>) -> Option<String> {
        missing.extend(macos_missing(host));
        if host == HostOs::Macos && std::env::consts::ARCH != "aarch64" {
            missing.push("host:apple-silicon".into());
        }
        tool_version(tools, "container", &["--version"], missing)
    }
    fn guest(&self, spec: &Spec) -> Result<Guest, MachineError> {
        Ok(guest(spec))
    }
    fn name_limit(&self) -> Option<NameLimit> {
        None
    }
    fn data_volume(&self, spec: &Spec) -> Option<String> {
        Some(crate::setup::vm::data_volume(spec))
    }
    fn create(&self, spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
        create(spec, bundle)
    }
    fn remove(&self, spec: &Spec) -> Result<(), MachineError> {
        remove(spec)
    }
    fn start(&self, spec: &Spec, bundle: &Bundle) -> Result<Option<Running>, MachineError> {
        start(spec, bundle).map(Some)
    }
    fn stop(&self, spec: &Spec, force: bool) -> Result<(), MachineError> {
        stop(spec, force)
    }
    fn provider_state(&self, spec: &Spec) -> Result<crate::setup::State, MachineError> {
        super::status::observe(spec)
    }
    fn status(&self, spec: &Spec) -> Result<Status, MachineError> {
        status(spec)
    }
    /// The guest has no name for this host: it reaches it at the container
    /// network's gateway (G7 receipt), the one its last start recorded
    /// (#66) when there is one, on which the server must listen (0.0.0.0,
    /// as Docker Desktop publishes a port), not on 127.0.0.1 alone.
    fn loopback(&self, spec: &Spec) -> Loopback {
        let gateway = match spec.state.gateway {
            Some(recorded) => recorded.to_string(),
            None => "the container network's gateway, 192.168.64.1 on vmnet's default network"
                .to_owned(),
        };
        Loopback::Apart(format!(
            "{gateway}, on which the server must listen, not on 127.0.0.1 alone; the machine has no name for this host"
        ))
    }
    fn forward(&self, spec: &Spec, _guest_port: u16) -> Result<Forward, MachineError> {
        Err(refuse(spec, Code::Unsupported, FORWARD))
    }
}
