//! `native`: the host itself. Nothing is made, started or stopped; its
//! Sandbox backend runs in the caller's process. On Windows it holds only
//! records and its Stores are in the WSL worker, which is not designed yet.
use crate::platform::HostOs;
use crate::probe::native_missing;
use crate::setup::{State, Status};

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
