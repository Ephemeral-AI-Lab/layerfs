//! `lima`'s adapter.
use super::{create, daemon_forward, guest, remove, start, status, stop};
use crate::platform::adapter::Adapter;
use crate::platform::error::MachineError;
use crate::setup::guest::Guest;
use crate::setup::probe::{macos_missing, tool_version};
use crate::setup::spec::{Bundle, Forward, NameLimit, Running, Spec, Tools};
use crate::setup::Status;
use crate::table::profile::HostOs;
use crate::table::reach::Loopback;

/// What a lima guest calls this host (#535 §7.3): the guest's `/etc/hosts`
/// names `host.lima.internal`, the gateway of Lima's default user-mode
/// network (192.168.5.2, G7 live receipt). A Sandbox resolves it with its
/// machine's hosts file, bound into its root at start.
const HOST_NAME: &str = "host.lima.internal names this host there";

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
    fn name_limit(&self) -> Option<NameLimit> {
        super::name_limit::name_limit()
    }
    fn data_volume(&self, spec: &Spec) -> Option<String> {
        Some(super::status::disk_name(&spec.vm_name))
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
        super::status::observe(spec).map(|(state, _)| state)
    }
    fn status(&self, spec: &Spec) -> Result<Status, MachineError> {
        status(spec)
    }
    fn loopback(&self, _spec: &Spec) -> Loopback {
        Loopback::Apart(HOST_NAME.to_owned())
    }
    fn forward(&self, spec: &Spec, guest_port: u16) -> Result<Forward, MachineError> {
        daemon_forward(spec, guest_port)
    }
}
