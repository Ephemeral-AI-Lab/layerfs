//! Workspace calls on a forwarded Sandbox (#535 G4) in this process: mount,
//! Commit, unmount and status straight to its daemon, and the directory of
//! a Workspace an exec runs in (moved from `sandbox/direct.rs`,
//! layerfs-project#106).
use crate::direct::args;
use crate::record::SandboxRecord;
use crate::workspace::serve;
use crate::{dispatch, files, unmount_guard};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_history::{BranchId, CommitId};
use layerfs_home::events::Events;
use layerfs_home::helpers;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_store::call::store_call;
use layerfs_store::lease;
use layerfs_store::resolve::{self, Target};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Runs Workspace operation `op` (`workspace.mount`, `workspace.commit`,
/// `workspace.unmount`, `backend.daemon.workspace.status`,
/// `sandbox.list_workspaces`) on forwarded Sandbox `record` in this
/// process, under the Store's lease.
pub fn call(
    home: &Path,
    events: &mut Events<'_>,
    record: &SandboxRecord,
    op: &str,
    more: Vec<(&str, SdkValue)>,
) -> Outcome {
    let home = home.to_path_buf();
    let target = resolve::target(&home, record.project().unwrap_or(""))?;
    let _lease = lease::hold(&home, &target.store)?;
    let args = args(&home, record, &target, more);
    match op {
        "workspace.mount" => serve::mount_with(&args, &mut |branch| {
            Ok(branch_of(&home, events, &target, branch)?.0)
        }),
        "workspace.commit" => serve::commit(&serve::by_name(args)?),
        "workspace.unmount" => serve::unmount(&serve::by_name(args)?),
        "backend.daemon.workspace.status" => {
            serve::status_with(&serve::by_name(args)?, &mut |id, name| {
                let (found, head) = branch_of(&home, events, &target, name?).ok()?;
                (found == id).then_some(head)
            })
        }
        "sandbox.list_workspaces" => dispatch::call(op, args),
        _ => Err(refused(
            "InvalidInput",
            format!("{op} is not sent to a Sandbox's daemon directly"),
        )),
    }
}

/// The id and head of Branch `name` of `target`'s project, read through
/// the Store's own route (in this process, or its machine's worker).
fn branch_of(
    home: &Path,
    events: &mut Events<'_>,
    target: &Target,
    name: &str,
) -> Result<(BranchId, Option<CommitId>)> {
    let listed = store_call(
        home,
        events,
        target,
        "project.list_branches",
        vec![("name".to_owned(), SdkValue::text(name))],
    )?;
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
/// exec its worker runs: refused `NotMounted` when no record names it,
/// and while an unmount holds it.
pub fn exec_directory(home: &Path, record: &SandboxRecord, at: &str) -> Result<String> {
    let dir = record.dir(home);
    let field = if at.starts_with('/') { "path" } else { "name" };
    let found = (files::workspaces(&dir)?.into_iter())
        .find(|(_, workspace)| workspace.get_text(field) == Some(at));
    let Some((_, workspace)) = found else {
        return Err(refused(
            "NotMounted",
            format!("{}:{at} is not mounted", record.name()),
        ));
    };
    let path = helpers::text(&workspace, "path")?.to_owned();
    unmount_guard::check_in(&dir, &path)?;
    Ok(path)
}
