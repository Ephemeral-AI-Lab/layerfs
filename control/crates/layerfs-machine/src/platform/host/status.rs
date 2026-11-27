//! `host`: Sandboxes on the host's own OS. Nothing is made, started or
//! stopped; its Sandbox backend runs in the caller's process. On Windows
//! it holds only records and its Stores are in the WSL worker, which is
//! not designed yet.
use crate::platform::error::MachineError;
use crate::setup::probe::native_missing;
use crate::setup::{State, Status};
use crate::table::profile::HostOs;

/// The host runs whenever the caller does. A host that lacks what
/// Sandboxes need (`native_missing`: root, `/dev/fuse`, cgroup v2 on Linux)
/// is `Unsupported` naming it, as another platform's status is when its
/// tool is missing and as `provider_within` answers, so it never reads
/// `ready`. There is no data disk of its own to report.
pub(crate) fn status() -> Result<Status, MachineError> {
    let missing = native_missing(HostOs::current());
    if !missing.is_empty() {
        return Err(MachineError::missing(
            "the host machine cannot run Sandboxes on this host",
            missing,
        ));
    }
    Ok(Status {
        state: State::Running,
        kernel: Some(State::Running),
        worker: None,
        free_disk: None,
        tool_version: None,
        down_reason: None,
    })
}
