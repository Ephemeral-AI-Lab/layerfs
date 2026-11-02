//! `microsandbox`'s adapter.
use super::{create, guest, remove, start, status, stop};
use crate::adapter::{refused, Adapter};
use crate::error::MachineError;
use crate::platform::HostOs;
use crate::probe::tool_version;
use crate::setup::guest::Guest;
use crate::setup::Status;
use crate::spec::{Bundle, Running, Spec, Tools};

pub(crate) struct Microsandbox;
pub(crate) static ADAPTER: Microsandbox = Microsandbox;

impl Adapter for Microsandbox {
    fn probe(&self, tools: &Tools, _host: HostOs, missing: &mut Vec<String>) -> Option<String> {
        tool_version(tools, "msb", &["--version"], missing)
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
}
