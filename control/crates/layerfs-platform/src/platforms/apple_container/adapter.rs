//! `apple-container`'s adapter.
use super::observe::gateway;
use super::{create, guest, remove, start, status, stop};
use crate::adapter::Adapter;
use crate::error::{Code, MachineError};
use crate::platform::HostOs;
use crate::probe::{macos_missing, tool_version};
use crate::reach::{dial, Loopback};
use crate::setup::forward::refuse;
use crate::setup::guest::Guest;
use crate::setup::Status;
use crate::spec::{Bundle, Forward, Running, Spec, Tools};

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
    fn create(&self, spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
        create(spec, bundle)
    }
    fn remove(&self, spec: &Spec) -> Result<(), MachineError> {
        remove(spec)
    }
    fn start(&self, spec: &Spec, bundle: &Bundle) -> Result<Option<Running>, MachineError> {
        start(spec, bundle).map(Some)
    }
    fn stop(&self, spec: &Spec) -> Result<(), MachineError> {
        stop(spec)
    }
    fn provider_state(&self, spec: &Spec) -> Result<crate::setup::State, MachineError> {
        super::observe::observe(spec)
    }
    fn status(&self, spec: &Spec) -> Result<Status, MachineError> {
        status(spec)
    }
    /// A loopback URL becomes the container network's gateway (G7 receipt):
    /// the one its last start recorded (#66), else read from the machine
    /// (a record from before #66, or a start that could not read it). A
    /// recorded gateway that went stale is dialled as recorded and fails
    /// there, explicitly; nothing looks it up again.
    fn reach(&self, spec: &Spec, url: &str) -> Result<String, MachineError> {
        match spec.state.gateway {
            Some(recorded) => dial(spec, url, Loopback::Alias(&recorded.to_string())),
            None => dial(spec, url, Loopback::Lookup(&|| gateway(spec))),
        }
    }
    fn forward(&self, spec: &Spec, _guest_port: u16) -> Result<Forward, MachineError> {
        Err(refuse(spec, Code::Unsupported, FORWARD))
    }
}
