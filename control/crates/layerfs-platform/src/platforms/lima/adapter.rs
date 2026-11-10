//! `lima`'s adapter.
use super::{create, daemon_forward, guest, remove, start, status, stop};
use crate::adapter::Adapter;
use crate::error::MachineError;
use crate::platform::HostOs;
use crate::probe::{macos_missing, tool_version};
use crate::reach::{dial, Loopback};
use crate::setup::guest::Guest;
use crate::setup::Status;
use crate::spec::{Bundle, Forward, Running, Spec, Tools};

/// The address a lima guest reaches the host's loopback at (#535 §7.3): the
/// gateway of Lima's default user-mode network, which the guest's
/// `/etc/hosts` names `host.lima.internal`. The address, not the name: a
/// Sandbox daemon runs in the Sandbox image's root, which has no such
/// hosts entry (G7 live receipt, `receipts/g7-live/`).
const HOST_ALIAS: &str = "192.168.5.2";

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
    fn provider_state(&self, spec: &Spec) -> Result<crate::setup::State, MachineError> {
        super::observe::observe(spec).map(|(state, _)| state)
    }
    fn status(&self, spec: &Spec) -> Result<Status, MachineError> {
        status(spec)
    }
    fn reach(&self, spec: &Spec, url: &str) -> Result<String, MachineError> {
        dial(spec, url, Loopback::Alias(HOST_ALIAS))
    }
    fn forward(&self, spec: &Spec, guest_port: u16) -> Result<Forward, MachineError> {
        daemon_forward(spec, guest_port)
    }
}
