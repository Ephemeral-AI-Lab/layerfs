//! Workspace calls (#373). `workspace.mount(sandbox, branch)` mounts the new
//! Workspace in the running Sandbox the call names, and nowhere else: there
//! is no placement (owner 2026-10-23), so a mount never starts a Sandbox and
//! takes no project lock; `sandbox.create` is the only start. A Workspace is
//! `{ sandbox, path }`; status, Commit and unmount go to its Sandbox's
//! daemon. Nothing is retried: a refused or failed step is the call's answer.
//! A Sandbox whose daemon ended itself when idle is refused `NoSuchSandbox`
//! by a mount and reclaimed by its `sandbox.close`. Each Workspace is
//! recorded in its Sandbox's folder, `workspace/<id>.json`.
use crate::common::outcome::{failed, Outcome};
use crate::daemon::values;
use crate::internal::workspace;
use crate::sandbox::registry::{lock, random, sandbox, sandbox_live, text, Mount};
use crate::sandbox::serve::{operation, WORKSPACES};
use crate::Unmounted;
use layerfs_bridge::control::{CommitControl, SdkRefusal, SdkValue};
use layerfs_history::WorkspaceId;

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
/// [`WORKSPACES`]; its `sandbox`, `path`, `image`, `platform` and `runtime`. Refused
/// `NoSuchSandbox` (no such Sandbox, or it closed itself) and `Capacity`
/// (full), each naming the calls to make instead. Only the Sandbox's own
/// guard is taken; its record and count are checked under it.
pub(crate) fn mount(args: &SdkValue) -> Outcome {
    let branch = text(args, "branch")?;
    let sandbox = match args.get_text("sandbox") {
        Some(id) if !id.is_empty() => id,
        _ => {
            return Err(crate::common::outcome::refused(
                "InvalidInput",
                "workspace.mount names its Sandbox: workspace.mount(sandbox, branch), with a Sandbox from `sandbox.create()` or `project.list_sandboxes()`",
            ))
        }
    };
    let directory = crate::sandbox::files::sandbox_dir(args)?;
    let missing = || {
        no_sandbox(crate::common::outcome::refused(
            "NoSuchSandbox",
            format!("no Sandbox {sandbox} runs on this machine"),
        ))
    };
    if crate::sandbox::files::started(&directory)?.is_none() {
        return Err(missing());
    }
    let state = sandbox_live(args).map_err(|refusal| match refusal.code.as_str() {
        "NoSuchSandbox" => no_sandbox(refusal),
        _ => refusal,
    })?;
    let _guard = crate::sandbox::files::guard(&state.directory)?;
    // under the Sandbox's guard: a close may have removed it, or mounts filled it
    let Some(saved) = crate::sandbox::files::started(&directory)? else {
        return Err(missing());
    };
    let workspaces = crate::sandbox::files::workspaces(&directory)?;
    if workspaces.len() >= WORKSPACES as usize {
        return Err(crate::common::outcome::refused(
            "Capacity",
            format!(
                "Sandbox {sandbox} already holds {WORKSPACES} Workspaces. Create a new Sandbox with `sandbox.create()` and mount there, or list this one's Workspaces with `sandbox.list_workspaces(\"{sandbox}\")` and unmount the ones you are done with (`workspace.unmount(ws)`)"
            ),
        ));
    }
    let id = WorkspaceId::from_authority(random()?).map_err(failed)?;
    // The Branch is found by name in this process; the daemon binds its id.
    let branch_id = crate::store::service::branch_id(args, branch)?;
    let mounted = {
        let mut control = crate::common::trace::control(&state.controls)?;
        workspace(&mut control).mount(id, branch_id)
    };
    let mounted = mounted.map_err(failed)?;
    let mut value = values::mounted(&mounted, branch);
    if let SdkValue::Map(fields) = &mut value {
        fields.push(("sandbox".to_owned(), SdkValue::text(state.id.as_str())));
        fields.push(("container".to_owned(), SdkValue::text(state.name.as_str())));
        fields.push((
            "image".to_owned(),
            SdkValue::text(crate::sandbox::runtime::image(&state.runtime)),
        ));
        // what its record names (#529)
        fields.push(("platform".to_owned(), platform_of(&saved)));
        fields.push((
            "runtime".to_owned(),
            SdkValue::text(crate::sandbox::runtime::runtime_name(&saved)),
        ));
    }
    crate::sandbox::files::save_workspace(&state, &mounted, branch, None)?;
    lock(&state.mounts)?.insert(
        mounted.ready.directory.clone(),
        Mount {
            workspace: mounted,
            branch: branch.to_owned(),
        },
    );
    Ok(value)
}

/// A record's `platform` (#529), or null for a record from before it, which
/// the agent reads as the platform its runtime and home imply.
fn platform_of(record: &SdkValue) -> SdkValue {
    SdkValue::opt_text(record.get_text("platform").map(str::to_owned))
}

/// `backend.daemon.workspace.status(ws)`: the daemon's Status, which reads no
/// Store, and the Branch head from one newest-first History read in this
/// process.
pub(crate) fn status(args: &SdkValue) -> Outcome {
    let state = sandbox(args)?;
    let token = state.token(text(args, "path")?)?;
    let mut control = crate::common::trace::control(&state.controls)?;
    let status = workspace(&mut control).status(token).map_err(operation)?;
    drop(control);
    let head = crate::store::service::head(args, status.binding.branch.id).ok();
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
    let reply = workspace(&mut *crate::common::trace::control(&state.controls)?)
        .commit_with(CommitControl {
            token,
            force: args.get_bool("force"),
            message: args.get_text("message").map(str::to_owned),
        })
        .map_err(operation)?;
    values::commit(&reply)
}

/// `workspace.unmount(ws, { discard?, force? })`: the daemon's answer.
pub(crate) fn unmount(args: &SdkValue) -> Outcome {
    let state = sandbox(args)?;
    let path = text(args, "path")?;
    if let Some(value) = crate::workspace::unmount_guard::find(&state, path)? {
        return crate::workspace::unmount_guard::repeat(&value);
    }
    let guard = crate::sandbox::files::guard(&state.directory)?;
    // Another process may have registered while this call waited for the
    // short admission section. Check again before sending anything.
    if let Some(value) = crate::workspace::unmount_guard::find(&state, path)? {
        return crate::workspace::unmount_guard::repeat(&value);
    }
    let token = state.token(path)?;
    crate::sandbox::files::refresh(&state)?;
    // the daemon's kernel probe answers Busy for a process still holding it
    let attempt = crate::workspace::unmount_guard::begin(&state, token, path)?;
    drop(guard);
    // Only this Workspace's lease remains held. Other Workspaces and
    // processes can take Sandbox ownership while the daemon helper waits.
    let mut sent = false;
    let outcome = (|| {
        let mut control = crate::common::trace::control(&state.controls)?;
        sent = true;
        let mut api = workspace(&mut control);
        // Forced: one abort write ends the connection, also after a normal
        // unmount stalled at Join; uncommitted changes are dropped.
        if args.get_bool("force") {
            match api.unmount(token, true).map_err(operation)? {
                Some(outcome) => Ok(values::force_unmounted(&outcome)),
                None => values::unmounted(&Unmounted::Closed),
            }
        } else {
            match api.unmount_checked(token, args.get_bool("discard")) {
                Ok(answer) => values::unmounted(&answer),
                Err(error) => Err(operation(error)),
            }
        }
    })();
    let outcome = match outcome {
        Err(error) if error.code == "Missing" => {
            Ok(SdkValue::map([("kind", SdkValue::text("Unmounted"))]))
        }
        other => other,
    };
    // Preserve the daemon's known outcome before any discovery-lock wait.
    // A file/cleanup failure is separate from an already successful detach.
    let publication = if !sent {
        crate::workspace::unmount_guard::cancel(&attempt)
    } else {
        crate::workspace::unmount_guard::finish(&attempt, path, &outcome)
    };
    if let Err(error) = publication {
        return crate::workspace::unmount_guard::cleanup_failure(outcome, error);
    }
    if outcome.is_ok() {
        let cleanup = (|| {
            let _guard = crate::sandbox::files::guard(&state.directory)?;
            match std::fs::remove_file(crate::sandbox::files::workspace_file(&state, token)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(failed(error)),
            }
            lock(&state.mounts)?.remove(path);
            Ok(())
        })();
        if let Err(error) = cleanup {
            return crate::workspace::unmount_guard::cleanup_failure(outcome, error);
        }
    }
    outcome
}
