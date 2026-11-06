//! `firecracker`'s adapter.
use super::{create, machine_guest, remove, start, status, stop};
use crate::adapter::{refused, Adapter};
use crate::error::{Code, MachineError};
use crate::platform::HostOs;
use crate::probe::{macos_missing, nested_missing, tool_version};
use crate::reach::{dial, Loopback};
use crate::setup::forward::refuse;
use crate::setup::guest::Guest;
use crate::setup::Status;
use crate::spec::{Bundle, Forward, Running, Spec, Tools};

/// Why a Sandbox daemon's port is not forwarded: a daemon listens on the
/// microVM's loopback (`layerfs-sandbox-backend`'s local listener takes only a
/// loopback address), and the worker's forward is an `ssh -L` to the Lima
/// host, which reaches the microVM only at its tap address, where only the
/// worker listens (`worker_bind` 0.0.0.0). Reaching the microVM's loopback
/// needs a second hop LayerFS does not have.
const FORWARD: &str = "its Sandbox daemons listen on the microVM's loopback, which a forward through the Lima host cannot reach (it reaches only the microVM's tap address); its Workspace calls go through the worker";

/// The Lima host's own host address (`host.lima.internal`, the gateway
/// of Lima's default user-mode network), which a microVM reaches through
/// the Lima host's masquerade; Lima maps it to this host's loopback.
const LIMA_HOST: &str = "192.168.5.2";

pub(crate) struct Firecracker;
pub(crate) static ADAPTER: Firecracker = Firecracker;

impl Adapter for Firecracker {
    fn probe(&self, tools: &Tools, host: HostOs, missing: &mut Vec<String>) -> Option<String> {
        missing.extend(macos_missing(host));
        missing.extend(nested_missing(host));
        tool_version(tools, "limactl", &["--version"], missing)
    }
    fn guest(&self, spec: &Spec) -> Result<Guest, MachineError> {
        machine_guest(spec)
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
    /// The guest's tap address, `172.30.<n>.1` (`scripts.rs`), is the nested
    /// Lima host, not the caller's host. The Lima host masquerades the
    /// microVMs' traffic (`BOOT_VM`), so a loopback URL becomes the Lima
    /// host's own host address, [`LIMA_HOST`] (G7 receipt). A server
    /// elsewhere is dialled as given.
    fn reach(&self, spec: &Spec, url: &str) -> Result<String, MachineError> {
        dial(spec, url, Loopback::Alias(LIMA_HOST))
    }
    fn forward(&self, spec: &Spec, _guest_port: u16) -> Result<Forward, MachineError> {
        Err(refuse(spec, Code::Unsupported, FORWARD))
    }
}
