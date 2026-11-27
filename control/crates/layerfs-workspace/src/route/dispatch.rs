//! The host Workspace operations, here or in a machine's worker
//! (`sandbox.host`): mount, Commit, unmount, exec, its cancel and the status
//! read; every
//! other operation is a Sandbox operation (`sandbox::dispatch`). Moved from
//! `sandbox/dispatch.rs` so Sandbox code never imports Workspace code
//! (layerfs-project#106). It matches on the one table of host operations
//! (`host::Call`, R1).
use crate::machine::serve as workspace;
use layerfs_home::outcome::Outcome;
use layerfs_sandbox::exec;
use layerfs_sandbox::route::machine::{Call, Host};

/// Runs one host operation once: a Workspace operation here, any other in
/// `sandbox::dispatch`, which checks a Commit `message` first.
pub fn call(host: Host, call: Call) -> Outcome {
    match call {
        Call::Mount { .. } | Call::Commit { .. } | Call::Unmount { .. } | Call::Exec { .. } => {
            layerfs_sandbox::route::machine::dispatch::checked(&host.message)?;
        }
        Call::Status { .. } | Call::ExecCancel { .. } => {}
        _ => {
            return layerfs_sandbox::route::machine::dispatch::call(
                host,
                call,
                &crate::machine::drain::Drain,
            )
        }
    }
    let path = match call.path() {
        Some(_) => workspace::by_name(&host, call.path())?,
        None => None,
    };
    layerfs_home::trace::step("by_name");
    match call {
        Call::Mount { branch, name } => workspace::mount(&host, branch.as_deref(), name.as_deref()),
        Call::Commit {
            force,
            ref expected,
            ..
        } => {
            let force = force.unwrap_or(false);
            workspace::commit(&host, path.as_deref(), force, expected.as_deref())
        }
        Call::Unmount { discard, force, .. } => workspace::unmount(
            &host,
            path.as_deref(),
            discard.unwrap_or(false),
            force.unwrap_or(false),
        ),
        Call::Status { ref under, .. } => {
            workspace::status(&host, path.as_deref(), under.as_deref())
        }
        Call::Exec {
            directory,
            command,
            cwd,
            env,
            stdin,
            exec_id,
            workspace,
            tag,
            ..
        } => crate::exec::run::exec(
            &host,
            &crate::exec::run::Exec {
                path: path.as_deref(),
                directory: directory.as_deref(),
                command: command.as_deref(),
                cwd: cwd.as_deref(),
                env: &env,
                stdin: stdin.as_deref(),
                exec_id: exec_id.as_deref(),
                workspace: workspace.as_deref(),
                tag: tag.as_deref(),
            },
        ),
        Call::ExecCancel {
            exec_id,
            grace_ms,
            workspace,
            ..
        } => exec::cancel::call(
            &host,
            path.as_deref(),
            (exec_id.as_deref(), grace_ms),
            workspace.as_deref(),
        ),
        _ => layerfs_sandbox::route::machine::dispatch::call(
            host,
            call,
            &crate::machine::drain::Drain,
        ),
    }
}
