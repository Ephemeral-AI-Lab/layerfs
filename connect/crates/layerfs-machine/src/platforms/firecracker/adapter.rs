//! `firecracker`'s adapter.
use super::{create, machine_guest, remove, start, status, stop};
use crate::adapter::{refused, Adapter};
use crate::error::MachineError;
use crate::platform::HostOs;
use crate::probe::{macos_missing, nested_missing, tool_version};
use crate::reach::{dial, Loopback};
use crate::setup::guest::Guest;
use crate::setup::Status;
use crate::spec::{Bundle, Running, Spec, Tools};

/// A firecracker machine whose network number was never recorded has no
/// host address its guest could dial.
const NO_NETWORK: &str =
    "its network number (the n of 172.30.<n>.1, recorded at create) is not known";

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
    /// The guest reaches the Lima host at its tap's address, `172.30.<n>.1`
    /// (`scripts.rs`); the Lima host's own outbound NAT is not built and the
    /// host loopback beyond it is not verified (G7: `support.rs` refuses).
    fn reach(&self, spec: &Spec, url: &str) -> Result<String, MachineError> {
        match spec.state.network {
            Some(n) => dial(spec, url, Loopback::Alias(&format!("172.30.{n}.1"))),
            None => dial(spec, url, Loopback::Refused(NO_NETWORK)),
        }
    }
}
