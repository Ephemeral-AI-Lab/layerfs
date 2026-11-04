//! Workspace calls (#373). `workspace.mount(sandbox, branch)` mounts the new
//! Workspace in the running Sandbox the call names, and nowhere else: there
//! is no placement (owner 2026-10-23), so a mount never starts a Sandbox and
//! takes no project lock; `sandbox.create` is the only start. A Workspace is
//! `{ sandbox, path }`; status, Commit and unmount go to its Sandbox's
//! daemon. Nothing is retried: a refused or failed step is the call's answer.
//! A Sandbox whose daemon ended itself when idle is refused `NoSuchSandbox`
//! by a mount and reclaimed by the next `sandbox.create` or
//! `runtime.sandbox.reclaim`.
use crate::job::{failed, refused, Outcome};
use crate::registry::{
    lock, naming, random, sandbox, sandbox_live, sandbox_refreshed, text, Mount, SandboxState,
};
use crate::sandbox::{operation, WORKSPACES};
use layerfs_api::internal::workspace;
use layerfs_api::sandbox::{values, Unmounted};
use layerfs_bridge::control::{CommitControl, SdkRefusal, SdkValue};
use layerfs_history::WorkspaceId;
use std::path::Path;

/// The Workspaces Sandbox directory `directory` records.
fn recorded(directory: &Path) -> Result<usize, SdkRefusal> {
    let mut count = 0;
    for item in std::fs::read_dir(directory).map_err(failed)? {
        let name = item.map_err(failed)?.file_name();
        let name = name.to_string_lossy();
        count += usize::from(name.starts_with("workspace-") && name.ends_with(".record"));
    }
    Ok(count)
}

/// A mount's `NoSuchSandbox`: `refusal` (its detail kept) with the next
/// step in its message, which agents read.
fn no_sandbox(mut refusal: SdkRefusal) -> SdkRefusal {
    refusal.message.push_str(
        ". Create a Sandbox with `sandbox.create()` and mount there: `workspace.mount(sandbox, branch)` with the `sandbox` it returns",
    );
    refusal
}

/// `workspace.mount(sandbox, branch)`: a new Workspace of `branch` in
/// Sandbox `sandbox`, which must be running and hold fewer than
/// [`WORKSPACES`]; its `sandbox`, `path` and `image`. Refused
/// `NoSuchSandbox` (no such Sandbox, or it closed itself) and `Capacity`
/// (full), each naming the calls to make instead. Only the Sandbox's own
/// guard is taken; its record and count are checked under it.
pub(crate) fn mount(args: &SdkValue) -> Outcome {
    let branch = text(args, "branch")?;
    let sandbox = match args.get_text("sandbox") {
        Some(id) if !id.is_empty() => id,
        _ => {
            return Err(crate::job::refused(
                "InvalidInput",
                "workspace.mount names its Sandbox: workspace.mount(sandbox, branch), with a Sandbox from `sandbox.create()` or `project.list_sandboxes()`",
            ))
        }
    };
    let directory = crate::discovery::sandbox_dir(args, sandbox)?;
    let record = directory.join("sandbox.record");
    let missing = || {
        no_sandbox(crate::job::refused(
            "NoSuchSandbox",
            format!("the project has no Sandbox {sandbox}"),
        ))
    };
    if !record.exists() {
        return Err(missing());
    }
    let state =
        sandbox_live(&naming(args, sandbox)).map_err(|refusal| match refusal.code.as_str() {
            "NoSuchSandbox" => no_sandbox(refusal),
            _ => refusal,
        })?;
    let _guard = crate::discovery::guard(&state.directory)?;
    // under the Sandbox's guard: a close may have removed it, or mounts filled it
    if !record.exists() {
        return Err(missing());
    }
    if recorded(&directory)? >= WORKSPACES as usize {
        return Err(crate::job::refused(
            "Capacity",
            format!(
                "Sandbox {sandbox} already holds {WORKSPACES} Workspaces. Create a new Sandbox with `sandbox.create()` and mount there, or list this one's Workspaces with `sandbox.list_workspaces(\"{sandbox}\")` and unmount the ones you are done with (`workspace.unmount(ws)`)"
            ),
        ));
    }
    let id = WorkspaceId::from_authority(random()?).map_err(failed)?;
    // The Branch is found by name in this process; the daemon binds its id.
    let branch_id = crate::direct::branch_id(args, branch)?;
    let mounted = {
        let mut control = crate::trace::control(&state.controls)?;
        workspace(&mut control).mount(id, branch_id)
    };
    let mounted = mounted.map_err(failed)?;
    let mut value = values::mounted(&mounted, branch);
    if let SdkValue::Map(fields) = &mut value {
        fields.push(("sandbox".to_owned(), SdkValue::text(state.id.as_str())));
        fields.push(("container".to_owned(), SdkValue::text(state.name.as_str())));
        let image = state.runtime.request().image.as_str();
        fields.push(("image".to_owned(), SdkValue::text(image)));
    }
    crate::discovery::save_workspace(&state, &mounted, branch)?;
    lock(&state.mounts)?.insert(
        mounted.ready.directory.clone(),
        Mount {
            workspace: mounted,
            branch: branch.to_owned(),
        },
    );
    Ok(value)
}

/// One recorded Sandbox of the project: running (`Ok(true)`), or found
/// gone or stopped (`Ok(false)`, with its container and whether it stopped).
/// Its container is inspected once; no daemon is dialed.
fn observed(record: &SdkValue, directory: &Path) -> Result<Option<(String, bool)>, SdkRefusal> {
    let cached = lock(&crate::registry::SANDBOXES)?.get(directory).cloned();
    let observed = match cached {
        Some(state) => state.runtime.endpoint().map(drop),
        None => {
            let (docker, selector, request) = crate::discovery::runtime(record)?;
            layerfs_sandbox::backend::docker::Sandbox::reopen(docker, selector, request).map(drop)
        }
    };
    match observed {
        Ok(()) => Ok(None),
        Err(failure) if failure.container_gone() || failure.container_stopped() => Ok(Some((
            text(record, "container")?.to_owned(),
            failure.container_stopped(),
        ))),
        Err(failure) => Err(failed(failure)),
    }
}

/// The project's recorded Sandboxes, lowest number first, with each record.
fn records(args: &SdkValue) -> Result<Vec<(String, std::path::PathBuf, SdkValue)>, SdkRefusal> {
    let mut found = Vec::new();
    for id in crate::discovery::sandbox_ids(args)? {
        let directory = crate::discovery::sandbox_dir(args, &id)?;
        let record = directory.join("sandbox.record");
        // a number claimed by a start that has not finished is not a Sandbox yet
        if record.exists() {
            let value = crate::discovery::load(&record)?;
            found.push((id, directory, value));
        }
    }
    Ok(found)
}

/// Reclaims every recorded Sandbox of the project found gone or stopped (an
/// idle-closed one), so its number is free again. `sandbox.create` calls it
/// under the project's guard before it starts one.
pub(crate) fn reclaim_gone(args: &SdkValue) -> Result<(), SdkRefusal> {
    for (id, directory, value) in records(args)? {
        if let Some((container, stopped)) = observed(&value, &directory)? {
            crate::sandbox::reclaim(args, &id, &container, stopped)?;
        }
    }
    Ok(())
}

/// `project.list_sandboxes()`: the project's running Sandboxes, lowest
/// number first: `{ sandbox, name, image, workspaces }`, where `workspaces`
/// counts its Workspace records. It reads records and inspects each
/// container once; it starts nothing and changes nothing. A Sandbox found
/// gone or stopped (an idle-closed one) is left out and its records kept:
/// `sandbox.create` and `runtime.sandbox.reclaim` remove them.
pub(crate) fn sandboxes(args: &SdkValue) -> Outcome {
    let mut items = Vec::new();
    for (id, directory, value) in records(args)? {
        if observed(&value, &directory)?.is_some() {
            continue;
        }
        items.push(SdkValue::map([
            ("sandbox", SdkValue::text(&id)),
            (
                "name",
                SdkValue::text(value.get_text("name").unwrap_or_default()),
            ),
            (
                "image",
                SdkValue::text(value.get_text("image").unwrap_or_default()),
            ),
            ("workspaces", SdkValue::Int(recorded(&directory)? as i64)),
        ]));
    }
    Ok(SdkValue::map([
        ("kind", SdkValue::text("ok")),
        ("items", SdkValue::List(items)),
    ]))
}

/// `runtime.sandbox.reclaim()`: removes the records of the project's
/// Sandboxes whose containers are gone or stopped (and a stopped one's
/// container), each under the project's guard, so their numbers are free
/// again: `{ kind, reclaimed }`. One started again under its number
/// meanwhile is left alone. Each step is attempted once.
pub(crate) fn reclaim(args: &SdkValue) -> Outcome {
    let mut reclaimed = Vec::new();
    for (id, directory, value) in records(args)? {
        let Some((container, stopped)) = observed(&value, &directory)? else {
            continue;
        };
        let _placing = crate::discovery::guard(&crate::discovery::project_dir(args)?)?;
        match crate::sandbox::reclaim(args, &id, &container, stopped) {
            Ok(_) => reclaimed.push(SdkValue::text(&id)),
            // started again under the number meanwhile, or already reclaimed
            Err(refusal) if matches!(refusal.code.as_str(), "Busy" | "NoSuchSandbox") => {}
            Err(refusal) => return Err(refusal),
        }
    }
    Ok(SdkValue::map([
        ("kind", SdkValue::text("ok")),
        ("reclaimed", SdkValue::List(reclaimed)),
    ]))
}

/// `backend.daemon.workspace.status(ws)`: the daemon's Status, which reads no
/// Store, and the Branch head from one newest-first History read in this
/// process.
pub(crate) fn status(args: &SdkValue) -> Outcome {
    let state = sandbox(args)?;
    let token = state.token(text(args, "path")?)?;
    let mut control = crate::trace::control(&state.controls)?;
    let status = workspace(&mut control).status(token).map_err(operation)?;
    drop(control);
    let head = crate::direct::head(args, status.binding.branch.id).ok();
    let mut value = values::status(&status, head);
    if let SdkValue::Map(fields) = &mut value {
        let branch = lock(&state.mounts)?
            .get(text(args, "path")?)
            .map(|mount| mount.branch.clone());
        fields.push(("branch".to_owned(), SdkValue::opt_text(branch)));
    }
    Ok(value)
}

/// `workspace.commit(ws, { message?, force? })`.
pub(crate) fn commit(args: &SdkValue) -> Outcome {
    let state = sandbox(args)?;
    let token = state.token(text(args, "path")?)?;
    let reply = workspace(&mut *crate::trace::control(&state.controls)?)
        .commit_with(CommitControl {
            token,
            force: args.get_bool("force"),
            message: args.get_text("message").map(str::to_owned),
        })
        .map_err(operation)?;
    values::commit(&reply)
}

/// A `Busy` refusal naming the processes that still run in the Workspace.
fn busy(state: &SandboxState, path: &str, processes: Vec<String>) -> SdkRefusal {
    SdkRefusal {
        code: "Busy".to_owned(),
        message: format!(
            "{}:{path} is busy: {} still running",
            state.id,
            processes.join(", ")
        ),
        detail: SdkValue::map([(
            "processes",
            SdkValue::List(processes.into_iter().map(SdkValue::Text).collect()),
        )]),
    }
}

/// How one `workspace.unmount` call ends the Workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ending {
    /// The daemon's normal unmount; `force` aborts a live connection instead.
    Normal { force: bool },
    /// Cancel only the holders of this Workspace, then a normal unmount.
    Kill,
}

/// Reads the `force` and `kill` arguments: `kill` accepts only `"workspace"`,
/// it cannot be combined with `force`, and a missing `kill` leaves the
/// existing force path alone.
fn ending(args: &SdkValue) -> Result<Ending, SdkRefusal> {
    let force = args.get_bool("force");
    match args.get_text("kill") {
        None => Ok(Ending::Normal { force }),
        Some("workspace") if !force => Ok(Ending::Kill),
        Some("workspace") => Err(refused(
            "InvalidInput",
            "force and kill cannot be combined: kill: \"workspace\" cancels only this Workspace's holders, then unmounts it normally",
        )),
        Some(other) => Err(refused(
            "InvalidInput",
            format!("kill must be \"workspace\", not {other:?}: only a Workspace's own holders are killed, never another Workspace or the Sandbox"),
        )),
    }
}

/// Every process id still holding the Workspace at `path`: the background
/// processes this binding started there and the foreground execs whose
/// `/tmp` record names it. Nothing from another Workspace is included.
fn holders(state: &SandboxState, path: &str) -> Result<Vec<String>, SdkRefusal> {
    Ok(holder_ids(
        crate::exec::running_in(state, path)?,
        crate::exec::foreground_in(state, path)?,
    ))
}

/// The holder ids of one Workspace: its background processes and foreground
/// execs, in one sorted list with no duplicate.
fn holder_ids(background: Vec<String>, foreground: Vec<crate::exec::Foreground>) -> Vec<String> {
    let mut ids = background;
    ids.extend(foreground.into_iter().map(|record| record.id));
    ids.sort();
    ids.dedup();
    ids
}

/// The end of a kill-scoped unmount, once no holder is left: the daemon's
/// normal unmount, which never stops or deletes the Sandbox and never aborts
/// the Workspace's connection.
fn unmount_normally(args: &SdkValue, state: &SandboxState) -> Outcome {
    let path = text(args, "path")?;
    let token = state.token(path)?;
    let discard = args.get_bool("discard");
    let value = {
        let mut control = crate::trace::control(&state.controls)?;
        let mut api = workspace(&mut control);
        match api.unmount_checked(token, discard).map_err(operation) {
            Ok(unmounted) => values::unmounted(&unmounted)?,
            // The daemon saw a holder the runtime owner may know as a
            // foreground exec: name its id so the caller can cancel it.
            Err(refusal) if refusal.code == "Busy" => {
                match crate::exec::foreground_in(state, path) {
                    Ok(records) if !records.is_empty() => {
                        return Err(busy(
                            state,
                            path,
                            records.into_iter().map(|record| record.id).collect(),
                        ))
                    }
                    _ => return Err(refusal),
                }
            }
            Err(refusal) => return Err(refusal),
        }
    };
    std::fs::remove_file(crate::discovery::workspace_file(state, token)).map_err(failed)?;
    lock(&state.mounts)?.remove(path);
    Ok(value)
}

/// A `Busy` refusal naming holders whose process group reaches outside the
/// Workspace, so a workspace-scoped kill left them alone.
fn scope_conflict(
    state: &SandboxState,
    path: &str,
    conflicts: Vec<(String, Vec<String>)>,
) -> SdkRefusal {
    let ids = conflicts
        .iter()
        .map(|(id, _)| SdkValue::text(id.as_str()))
        .collect::<Vec<_>>();
    let outside = conflicts
        .iter()
        .flat_map(|(_, paths)| paths.iter())
        .map(|path| SdkValue::text(path.as_str()))
        .collect::<Vec<_>>();
    SdkRefusal {
        code: "Busy".to_owned(),
        message: format!(
            "{}:{path} has a holder whose process group reaches outside the Workspace; nothing was killed",
            state.id
        ),
        detail: SdkValue::map([
            ("processes", SdkValue::List(ids)),
            ("scope_conflict", SdkValue::List(outside)),
        ]),
    }
}

/// `workspace.unmount(ws, { discard?, force?, kill? })`: `Busy` while a
/// background process this binding started, or a foreground exec, still runs
/// there, then the daemon's answer. `kill: "workspace"` first cancels those
/// holders (TERM, a bounded wait, then KILL only if still there), then
/// unmounts normally; if any holder survives it answers `Busy` with the ids.
/// It never stops or deletes the Sandbox and never touches another Workspace.
pub(crate) fn unmount(args: &SdkValue) -> Outcome {
    let ending = ending(args)?;
    let state = sandbox_refreshed(args)?;
    let _guard = crate::discovery::guard(&state.directory)?;
    let path = text(args, "path")?;
    let token = state.token(path)?;
    if ending == Ending::Kill {
        // Cancel only this Workspace's holders, then look again: a process
        // that would not stop is named rather than detached.
        let mut remaining = holders(&state, path)?;
        if !remaining.is_empty() {
            let mut conflicts = Vec::new();
            for id in &remaining {
                match crate::exec::terminate(&state, path, id)? {
                    crate::exec::Terminated::Outside(outside) => {
                        conflicts.push((id.clone(), outside))
                    }
                    crate::exec::Terminated::Gone | crate::exec::Terminated::Survived => {}
                }
            }
            // Never kill a group that reaches outside this Workspace: leave it
            // and name it, so the caller can decide. No silent collateral.
            if !conflicts.is_empty() {
                return Err(scope_conflict(&state, path, conflicts));
            }
            remaining = holders(&state, path)?;
        }
        if !remaining.is_empty() {
            return Err(busy(&state, path, remaining));
        }
        return unmount_normally(args, &state);
    }
    let force = ending == Ending::Normal { force: true };
    // The runtime owner knows its own processes; the daemon's kernel probe
    // still answers Busy for any other holder.
    let running = crate::exec::running_in(&state, path)?;
    if !running.is_empty() {
        return Err(busy(&state, path, running));
    }
    if force {
        // A foreground exec that is still running owns this Workspace. Forcing
        // now would detach it as Retained(Detach); make the caller cancel the
        // listed ids and unmount normally instead. A scan that fails leaves
        // the recovery path (force) as it was.
        if let Ok(records) = crate::exec::foreground_in(&state, path) {
            let ids = records
                .into_iter()
                .map(|record| record.id)
                .collect::<Vec<_>>();
            if !ids.is_empty() {
                return Err(busy(&state, path, ids));
            }
        }
    }
    let value = if force {
        let mut control = crate::trace::control(&state.controls)?;
        let mut api = workspace(&mut control);
        // One abort write ends the connection, also after a normal unmount
        // stalled at Join; uncommitted changes are dropped.
        match api.unmount(token, true).map_err(operation)? {
            Some(outcome) => values::force_unmounted(&outcome),
            None => values::unmounted(&Unmounted::Closed)?,
        }
    } else {
        return unmount_normally(args, &state);
    };
    std::fs::remove_file(crate::discovery::workspace_file(&state, token)).map_err(failed)?;
    lock(&state.mounts)?.remove(path);
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(fields: &[(&str, SdkValue)]) -> SdkValue {
        let mut map = SdkValue::Map(Vec::new());
        if let SdkValue::Map(entries) = &mut map {
            entries.extend(
                fields
                    .iter()
                    .map(|(name, value)| ((*name).to_owned(), value.clone())),
            );
        }
        map
    }

    fn foreground(id: &str, path: &str) -> crate::exec::Foreground {
        crate::exec::Foreground {
            id: id.to_owned(),
            path: path.to_owned(),
            pid: 1,
            start: "1".to_owned(),
        }
    }

    #[test]
    fn kill_accepts_only_the_workspace_scope() {
        assert_eq!(
            ending(&args(&[("kill", SdkValue::text("workspace"))])).unwrap(),
            Ending::Kill
        );
        for other in ["sandbox", "workspaces", "WORKSPACE", "", "all"] {
            let refusal = ending(&args(&[("kill", SdkValue::text(other))])).unwrap_err();
            assert_eq!(refusal.code, "InvalidInput", "{other:?}");
            assert!(
                refusal.message.contains("workspace"),
                "{other:?}: {}",
                refusal.message
            );
        }
    }

    #[test]
    fn kill_and_force_are_mutually_exclusive() {
        let refusal = ending(&args(&[
            ("force", SdkValue::Bool(true)),
            ("kill", SdkValue::text("workspace")),
        ]))
        .unwrap_err();
        assert_eq!(refusal.code, "InvalidInput");
        assert!(
            refusal.message.contains("force and kill"),
            "{}",
            refusal.message
        );
    }

    #[test]
    fn missing_or_absent_kill_keeps_the_normal_force_path() {
        assert_eq!(ending(&args(&[])).unwrap(), Ending::Normal { force: false });
        assert_eq!(
            ending(&args(&[("force", SdkValue::Bool(true))])).unwrap(),
            Ending::Normal { force: true }
        );
    }

    #[test]
    fn holder_ids_name_only_the_given_workspace_holders_once() {
        let ids = holder_ids(
            vec!["proc_1_aaaaaaaaaaaaaaaaaaaaaaaa".to_owned()],
            vec![
                foreground("proc_1_bbbbbbbbbbbbbbbbbbbbbbbb", "/workspaces/1"),
                foreground("proc_1_aaaaaaaaaaaaaaaaaaaaaaaa", "/workspaces/1"),
            ],
        );
        assert_eq!(
            ids,
            vec![
                "proc_1_aaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
                "proc_1_bbbbbbbbbbbbbbbbbbbbbbbb".to_owned(),
            ]
        );
        // A Workspace's holders never include a command from another path:
        // the selection is done by `running_in` / `foreground_in`, before the
        // merge, and this only merges what they returned.
        assert!(ids.iter().all(|id| id.starts_with("proc_1_")));
    }
}
