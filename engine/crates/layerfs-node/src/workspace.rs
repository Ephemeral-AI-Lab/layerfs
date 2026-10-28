//! Workspace calls (#373). `workspace.mount(sandbox, branch)` mounts the new
//! Workspace in the running Sandbox the call names, and nowhere else: there
//! is no placement (owner 2026-10-23), so a mount never starts a Sandbox and
//! takes no project lock; `sandbox.create` is the only start. A Workspace is
//! `{ sandbox, path }`; status, Commit and unmount go to its Sandbox's
//! daemon. Nothing is retried: a refused or failed step is the call's answer.
//! A Sandbox whose daemon ended itself when idle is refused `NoSuchSandbox`
//! by a mount and reclaimed by the next `sandbox.create` or
//! `runtime.sandbox.reclaim`.
use crate::job::{failed, Outcome};
use crate::registry::{
    lock, naming, random, sandbox, sandbox_live, sandbox_refreshed, text, Mount,
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

/// `workspace.unmount(ws, { discard?, force? })`: `Busy` while a background
/// process this binding started still runs there, then the daemon's answer.
pub(crate) fn unmount(args: &SdkValue) -> Outcome {
    let state = sandbox_refreshed(args)?;
    let _guard = crate::discovery::guard(&state.directory)?;
    let path = text(args, "path")?;
    let token = state.token(path)?;
    // The runtime owner knows its own processes; the daemon's kernel probe
    // still answers Busy for any other holder.
    let running = crate::exec::running_in(&state, path)?;
    if !running.is_empty() {
        return Err(SdkRefusal {
            code: "Busy".to_owned(),
            message: format!(
                "{}:{path} is busy: {} still running",
                state.id,
                running.join(", ")
            ),
            detail: SdkValue::map([(
                "processes",
                SdkValue::List(running.into_iter().map(SdkValue::Text).collect()),
            )]),
        });
    }
    let value = {
        let mut control = crate::trace::control(&state.controls)?;
        let mut api = workspace(&mut control);
        // Forced: one abort write ends the connection, also after a normal
        // unmount stalled at Join; uncommitted changes are dropped.
        if args.get_bool("force") {
            match api.unmount(token, true).map_err(operation)? {
                Some(outcome) => values::force_unmounted(&outcome),
                None => values::unmounted(&Unmounted::Closed)?,
            }
        } else {
            let discard = args.get_bool("discard");
            values::unmounted(&api.unmount_checked(token, discard).map_err(operation)?)?
        }
    };
    std::fs::remove_file(crate::discovery::workspace_file(&state, token)).map_err(failed)?;
    lock(&state.mounts)?.remove(path);
    Ok(value)
}
