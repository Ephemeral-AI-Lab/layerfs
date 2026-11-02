//! `native`: the host itself. Nothing is made, started or stopped; its
//! Sandbox backend runs in the caller's process. On Windows it holds only
//! records and its Stores are in the WSL worker, which is not designed yet.
use crate::error::{Code, MachineError};
use crate::kind::HostOs;
use crate::probe::native_missing;
use crate::setup::{State, Status};
use crate::spec::{Running, Spec};

/// Only reached where `native`'s route is a worker (Windows).
pub(crate) fn start(spec: &Spec) -> Result<Option<Running>, MachineError> {
    Err(MachineError::new(
        Code::Unsupported,
        format!(
            "the native machine {} on this host runs its Stores in a WSL2 worker, which is not designed yet",
            spec.name
        ),
    ))
}

/// The host runs whenever the caller does; `down_reason` names what a Linux
/// host lacks for Sandboxes. There is no data disk of its own to report.
pub(crate) fn status() -> Status {
    let missing = native_missing(HostOs::current());
    Status {
        state: State::Running,
        free_disk: None,
        tool_version: None,
        down_reason: (!missing.is_empty()).then(|| format!("missing: {}", missing.join(", "))),
    }
}
