//! `apple-container`'s adapter.
use super::{create, guest, remove, start, status, stop};
use crate::adapter::{refused, Adapter};
use crate::error::{Code, MachineError};
use crate::platform::HostOs;
use crate::probe::{macos_missing, tool_version};
use crate::reach::{dial, Loopback};
use crate::setup::forward::refuse;
use crate::setup::guest::Guest;
use crate::setup::Status;
use crate::spec::{Bundle, Forward, Running, Spec, Tools};
use crate::support::APPLE_CONTAINER_ALIAS;

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
    fn status(&self, spec: &Spec) -> Result<Status, MachineError> {
        status(spec)
    }
    fn host_identity(&self, spec: &Spec) -> Result<String, MachineError> {
        Err(refused(spec, "has no ssh host"))
    }
    fn reach(&self, spec: &Spec, url: &str) -> Result<String, MachineError> {
        dial(spec, url, Loopback::Refused(APPLE_CONTAINER_ALIAS))
    }
    fn forward(&self, spec: &Spec, _guest_port: u16) -> Result<Forward, MachineError> {
        Err(refuse(spec, Code::Unsupported, FORWARD))
    }
}
