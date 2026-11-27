//! `host`'s adapter: Sandboxes on the host's own OS; nothing is made,
//! started or stopped.
use super::{start, status};
use crate::platform::adapter::{refused, Adapter};
use crate::platform::error::MachineError;
use crate::table::profile::HostOs;
use crate::setup::probe::native_missing;
use crate::table::reach::Loopback;
use crate::setup::forward::shared;
use crate::setup::guest::Guest;
use crate::setup::Status;
use crate::setup::spec::{Bundle, Forward, NameLimit, Running, Spec, Tools};

pub(crate) struct Host;
pub(crate) static ADAPTER: Host = Host;

impl Adapter for Host {
    fn probe(&self, _tools: &Tools, host: HostOs, missing: &mut Vec<String>) -> Option<String> {
        missing.extend(native_missing(host));
        None
    }
    fn guest(&self, spec: &Spec) -> Result<Guest, MachineError> {
        Err(refused(spec, "runs in-process; its files are the host's"))
    }
    fn name_limit(&self) -> Option<NameLimit> {
        None
    }
    fn data_volume(&self, _spec: &Spec) -> Option<String> {
        None
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
    fn stop(&self, _spec: &Spec, _force: bool) -> Result<(), MachineError> {
        Ok(())
    }
    fn provider_state(&self, _spec: &Spec) -> Result<crate::setup::State, MachineError> {
        Ok(crate::setup::State::Running)
    }
    fn status(&self, _spec: &Spec) -> Result<Status, MachineError> {
        status()
    }
    fn loopback(&self, _spec: &Spec) -> Loopback {
        Loopback::Shared
    }
    /// The daemon is on this host's loopback: its port is used as it is.
    fn forward(&self, _spec: &Spec, guest_port: u16) -> Result<Forward, MachineError> {
        Ok(shared(guest_port))
    }
}
