//! The host Workspace operations, here or in a machine's worker
//! (`sandbox.host`): mount, Commit, unmount, exec and the status read; every
//! other operation is a Sandbox operation (`sandbox::dispatch`). Moved from
//! `sandbox/dispatch.rs` so Sandbox code never imports Workspace code
//! (layerfs-project#106). It matches on the one table of host operations
//! (`host::Call`, R1).
use crate::exec;
use crate::host::{Call, Host};
use crate::workspace::serve as workspace;
use layerfs_home::outcome::Outcome;

/// Runs one host operation once: a Workspace operation here, any other in
/// `sandbox::dispatch`, which checks a Commit `message` first.
pub fn call(host: Host, call: Call) -> Outcome {
    match call {
        Call::Mount { .. } | Call::Commit { .. } | Call::Unmount { .. } | Call::Exec { .. } => {
            crate::dispatch::checked(&host.message)?;
        }
        Call::Status { .. } => {}
        _ => return crate::dispatch::call(host, call),
    }
    let path = match call.path() {
        Some(_) => workspace::by_name(&host, call.path())?,
        None => None,
    };
    match call {
        Call::Mount { branch, name } => workspace::mount(&host, branch.as_deref(), name.as_deref()),
        Call::Commit { force, .. } => {
            workspace::commit(&host, path.as_deref(), force.unwrap_or(false))
        }
        Call::Unmount { discard, force, .. } => workspace::unmount(
            &host,
            path.as_deref(),
            discard.unwrap_or(false),
            force.unwrap_or(false),
        ),
        Call::Status { .. } => workspace::status(&host, path.as_deref()),
        Call::Exec {
            directory,
            command,
            spool,
            ..
        } => exec::exec(
            &host,
            path.as_deref(),
            (directory.as_deref(), command.as_deref()),
            spool,
        ),
        _ => crate::dispatch::call(host, call),
    }
}
