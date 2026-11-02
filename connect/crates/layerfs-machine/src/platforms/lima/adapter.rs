//! `lima`'s adapter.
use super::{create, guest, remove, start, status, stop};
use crate::adapter::{refused, Adapter};
use crate::error::MachineError;
use crate::platform::HostOs;
use crate::probe::{macos_missing, tool_version};
use crate::reach::{dial, Loopback};
use crate::setup::guest::Guest;
use crate::setup::Status;
use crate::spec::{Bundle, Running, Spec, Tools};

/// The name a lima guest reaches the host's loopback by (#535 §7.3; not yet
/// verified live, G7: `support.rs` refuses its PostgreSQL Sandboxes).
const HOST_ALIAS: &str = "host.lima.internal";

pub(crate) struct Lima;
pub(crate) static ADAPTER: Lima = Lima;

impl Adapter for Lima {
    fn probe(&self, tools: &Tools, host: HostOs, missing: &mut Vec<String>) -> Option<String> {
        missing.extend(macos_missing(host));
        tool_version(tools, "limactl", &["--version"], missing)
    }
    fn guest(&self, spec: &Spec) -> Result<Guest, MachineError> {
        guest(spec)
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
        dial(spec, url, Loopback::Alias(HOST_ALIAS))
    }
}
