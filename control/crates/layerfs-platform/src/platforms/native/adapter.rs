//! `native`'s adapter: the host itself; nothing is made, started or stopped.
use super::{start, status};
use crate::adapter::{refused, Adapter};
use crate::error::MachineError;
use crate::platform::HostOs;
use crate::probe::native_missing;
use crate::reach::{dial, Loopback};
use crate::setup::forward::shared;
use crate::setup::guest::Guest;
use crate::setup::Status;
use crate::spec::{Bundle, Forward, Running, Spec, Tools};

pub(crate) struct Native;
pub(crate) static ADAPTER: Native = Native;

impl Adapter for Native {
    fn probe(&self, _tools: &Tools, host: HostOs, missing: &mut Vec<String>) -> Option<String> {
        missing.extend(native_missing(host));
        None
    }
    fn guest(&self, spec: &Spec) -> Result<Guest, MachineError> {
        Err(refused(spec, "runs in-process; its files are the host's"))
    }
    fn create(&self, spec: &Spec, _bundle: &Bundle) -> Result<Running, MachineError> {
        Err(refused(spec, "is built in and never created"))
    }
    fn remove(&self, spec: &Spec) -> Result<(), MachineError> {
        Err(refused(spec, "is built in and never removed"))
    }
    fn start(&self, spec: &Spec, _bundle: &Bundle) -> Result<Option<Running>, MachineError> {
        start(spec)
    }
    fn stop(&self, _spec: &Spec) -> Result<(), MachineError> {
        Ok(())
    }
    fn provider_state(&self, _spec: &Spec) -> Result<crate::setup::State, MachineError> {
        Ok(crate::setup::State::Running)
    }
    fn status(&self, _spec: &Spec) -> Result<Status, MachineError> {
        Ok(status())
    }
    fn reach(&self, spec: &Spec, url: &str) -> Result<String, MachineError> {
        dial(spec, url, Loopback::Shared)
    }
    /// The daemon is on this host's loopback: its port is used as it is.
    fn forward(&self, _spec: &Spec, guest_port: u16) -> Result<Forward, MachineError> {
        Ok(shared(guest_port))
    }
}
