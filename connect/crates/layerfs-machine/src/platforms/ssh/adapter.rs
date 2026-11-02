//! `ssh`'s adapter: `create` connects and `remove` disconnects.
use super::{connect, disconnect, identity, machine_guest, start, status, stop};
use crate::adapter::Adapter;
use crate::error::MachineError;
use crate::platform::HostOs;
use crate::probe::first_line;
use crate::setup::guest::Guest;
use crate::setup::{tool, Status};
use crate::spec::{Bundle, Running, Spec, Tools};

pub(crate) struct Ssh;
pub(crate) static ADAPTER: Ssh = Ssh;

impl Adapter for Ssh {
    fn probe(&self, tools: &Tools, _host: HostOs, missing: &mut Vec<String>) -> Option<String> {
        let Some(ssh) = tool::lookup(tools, "ssh") else {
            missing.push("tool:ssh".into());
            return None;
        };
        first_line(tool::command(&ssh, ["-V"]), true)
    }
    fn guest(&self, spec: &Spec) -> Result<Guest, MachineError> {
        machine_guest(spec)
    }
    fn create(&self, spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
        connect(spec, bundle)
    }
    fn remove(&self, spec: &Spec) -> Result<(), MachineError> {
        disconnect(spec)
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
        identity(spec)
    }
}
