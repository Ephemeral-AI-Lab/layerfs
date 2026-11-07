//! Workspace calls on a forwarded Sandbox (#535 G4) in this process: mount,
//! Commit, unmount and status straight to its daemon, and the directory of
//! a Workspace an exec runs in (moved from `sandbox/direct.rs`,
//! layerfs-project#106).
use crate::direct::host;
use crate::host::{Call, Message};
use crate::record::{SandboxRecord, WorkspaceRecord};
use crate::workspace::serve;
use crate::{dispatch, files, unmount_guard};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_history::{BranchId, CommitId};
use layerfs_home::events::Events;
use layerfs_home::helpers;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_store::call::store_call;
use layerfs_store::calls::{Page, StoreOp};
use layerfs_store::lease;
use layerfs_store::resolve::{self, Target};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Runs Workspace operation `call` (mount, Commit, unmount, the status
/// read, `sandbox.list_workspaces`) with `message` on forwarded Sandbox
/// `record` in this process, under the Store's lease.
pub fn call(
    home: &Path,
    events: &mut Events<'_>,
    record: &SandboxRecord,
    call: Call,
    message: Message,
) -> Outcome {
    let home = home.to_path_buf();
    let target = resolve::target(&home, record.project().unwrap_or(""))?;
    let _lease = lease::hold(&home, &target.store)?;
    let host = host(&home, record, &target, message);
    match call {
        Call::Mount { branch, name } => serve::mount(&host, branch.as_deref(), name.as_deref()),
        Call::Commit { ref path, force } => {
            let path = serve::by_name(&host, path.as_deref())?;
            serve::commit(&host, path.as_deref(), force.unwrap_or(false))
        }
        Call::Unmount {
            ref path,
            discard,
            force,
        } => {
            let path = serve::by_name(&host, path.as_deref())?;
            let (discard, force) = (discard.unwrap_or(false), force.unwrap_or(false));
            serve::unmount(&host, path.as_deref(), discard, force)
        }
        Call::Status { ref path } => {
            let path = serve::by_name(&host, path.as_deref())?;
            serve::status_with(&host, path.as_deref(), &mut |id, name| {
                let (found, head) = branch_of(&home, events, &target, name?).ok()?;
                (found == id).then_some(head)
            })
        }
        Call::ListWorkspaces { .. } => dispatch::call(host, call),
        _ => Err(refused(
            "InvalidInput",
            format!(
                "{} is not sent to a Sandbox's daemon directly",
                call.op().name()
            ),
        )),
    }
}

/// The id and head of Branch `name` of `target`'s project (a status's),
/// read through the Store's own route (in this process, or its machine's
/// worker); a PostgreSQL Store's on this process's held read session.
fn branch_of(
    home: &Path,
    events: &mut Events<'_>,
    target: &Target,
    name: &str,
) -> Result<(BranchId, Option<CommitId>)> {
    if let Some(found) = layerfs_store::call::held_branch(home, target, name)? {
        return Ok(found);
    }
    let op = StoreOp::ListBranches {
        name: Some(name),
        page: Page::default(),
    };
    let listed = store_call(home, events, target, &op)?;
    let found = match listed.get("items") {
        Some(SdkValue::List(items)) => items.first().cloned(),
        _ => None,
    };
    let id = found
        .as_ref()
        .and_then(|item| item.get_text("id"))
        .and_then(layerfs_bridge::sdk_values::unhex)
        .and_then(|bytes| BranchId::from_slice(&bytes).ok())
        .ok_or_else(|| refused("NoSuchBranch", format!("Branch {name} does not exist")))?;
    let head = found
        .as_ref()
        .and_then(|item| item.get_text("head"))
        .and_then(layerfs_bridge::sdk_values::unhex)
        .and_then(|bytes| CommitId::from_slice(&bytes).ok());
    Ok((id, head))
}

/// The directory of Workspace `at` (its path, or its name) of forwarded
/// Sandbox `record`, as its record in the home folder names it, for an
/// exec or a cancel its worker runs, with that record: refused
/// `NotMounted` when no record names it, and while an unmount holds it.
pub fn exec_directory(
    home: &Path,
    record: &SandboxRecord,
    at: &str,
) -> Result<(String, WorkspaceRecord)> {
    let dir = record.dir(home);
    let by_path = at.starts_with('/');
    let found = (files::workspaces(&dir)?.into_iter()).find(|(_, workspace)| match by_path {
        true => workspace.path() == Some(at),
        false => workspace.name() == Some(at),
    });
    let Some((_, workspace)) = found else {
        return Err(refused(
            "NotMounted",
            format!("{}:{at} is not mounted", record.name()),
        ));
    };
    let path = helpers::required(workspace.path(), "path")?.to_owned();
    unmount_guard::check_in(&dir, &path)?;
    Ok((path, workspace))
}
