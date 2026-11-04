//! Workspace calls (#373). `workspace.mount(sandbox, branch)` mounts the new
//! Workspace in the running Sandbox the call names, and nowhere else: there
//! is no placement (owner 2026-10-23), so a mount never starts a Sandbox and
//! takes no project lock; `sandbox.create` is the only start. A Workspace is
//! `{ sandbox, path }`; status, Commit and unmount go to its Sandbox's
//! daemon. Nothing is retried: a refused or failed step is the call's answer.
//! A Sandbox whose daemon ended itself when idle is refused `NoSuchSandbox`
//! by a mount and reclaimed by its `sandbox.close`. Each Workspace is
//! recorded in its Sandbox's folder, `workspace/<id>.json`, with the name
//! its mount gave it, if any (unique in the Sandbox; the other calls take
//! it in place of the path, [`by_name`]).
use crate::common::names::WorkspaceName;
use crate::common::outcome::{failed, Outcome};
use crate::daemon::values;
use crate::internal::workspace;
use crate::sandbox::registry::{lock, random, sandbox, sandbox_live, text, Mount};
use crate::sandbox::serve::{operation, WORKSPACES};
use crate::Unmounted;
use layerfs_bridge::control::{CommitControl, SdkRefusal, SdkValue};
use layerfs_history::{BranchId, CommitId, WorkspaceId};

/// A mount's `NoSuchSandbox`: `refusal` (its detail kept) with the next
/// step in its message, which agents read.
fn no_sandbox(mut refusal: SdkRefusal) -> SdkRefusal {
    refusal.message.push_str(
        ". Create a Sandbox with `sandbox.create()` and mount there: `workspace.mount(sandbox, branch)` with the `sandbox` it returns",
    );
    refusal
}

/// `workspace.mount(sandbox, branch, { name })`: a new Workspace of
/// `branch` in Sandbox `sandbox`, which must be running and hold fewer than
/// [`WORKSPACES`]; its `sandbox`, `path`, `name`, `image`, `platform` and
/// `runtime`. Refused `NoSuchSandbox` (no such Sandbox, or it closed
/// itself), `Exists` (a Workspace of that name is mounted there) and
/// `Capacity` (full), each naming the calls to make instead. Only the Sandbox's own
/// guard is taken; its record and count are checked under it.
pub(crate) fn mount(args: &SdkValue) -> Outcome {
    mount_with(args, &mut |branch| {
        crate::store::service::branch_id(args, branch)
    })
}

/// [`mount`], its Branch's id found by `branch_of` (in this process, or
/// for a Sandbox reached directly through the Store's own route, #535 G4).
pub(crate) fn mount_with(
    args: &SdkValue,
    branch_of: &mut dyn FnMut(&str) -> Result<BranchId, SdkRefusal>,
) -> Outcome {
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
    if branch == "main" {
        // main is the project's line of Layers, not a Branch (#535 terminal pass)
        return Err(crate::common::outcome::refused(
            "InvalidInput",
            "main is the project's published line, not a Branch, and is not mounted: fork a Branch from it with `branch.fork(project, name, \"main\")` and mount that",
        ));
    }
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
    let name = match args.get_text("name") {
        Some(text) => Some(WorkspaceName::parse(text)?),
        None => None,
    };
    // a name in use is refused before the daemon is reached
    let recorded = crate::sandbox::files::workspaces(&directory)?;
    free(sandbox, &recorded, name.as_ref())?;
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
    // checked again under the guard, where it holds
    free(sandbox, &workspaces, name.as_ref())?;
    if workspaces.len() >= WORKSPACES as usize {
        return Err(crate::common::outcome::refused(
            "Capacity",
            format!(
                "Sandbox {sandbox} already holds {WORKSPACES} Workspaces. Create a new Sandbox with `sandbox.create()` and mount there, or list this one's Workspaces with `sandbox.list_workspaces(\"{sandbox}\")` and unmount the ones you are done with (`workspace.unmount(ws)`)"
            ),
        ));
    }
    let id = WorkspaceId::from_authority(random()?).map_err(failed)?;
    // The Branch is found by name; the daemon binds its id.
    let branch_id = branch_of(branch)?;
    let mounted = {
        let mut control = crate::common::trace::control(&state.controls)?;
        workspace(&mut control).mount(id, branch_id)
    };
    let mounted = mounted.map_err(failed)?;
    let mut value = values::mounted(&mounted, branch);
    if let SdkValue::Map(fields) = &mut value {
        fields.push(("sandbox".to_owned(), SdkValue::text(state.id.as_str())));
        fields.push(("container".to_owned(), SdkValue::text(state.name.as_str())));
        // a forwarded Sandbox's runtime is its machine's: its record says
        let image = match &state.runtime {
            Some(runtime) => crate::sandbox::runtime::image(runtime),
            None => saved.get_text("image").unwrap_or_default().to_owned(),
        };
        fields.push(("image".to_owned(), SdkValue::text(image)));
        // what its record names (#529)
        fields.push(("platform".to_owned(), platform_of(&saved)));
        fields.push((
            "runtime".to_owned(),
            SdkValue::text(crate::sandbox::runtime::runtime_name(&saved)),
        ));
    }
    if let SdkValue::Map(fields) = &mut value {
        let shown = name.as_ref().map(|name| name.to_string());
        fields.push(("name".to_owned(), SdkValue::opt_text(shown)));
    }
    let name = name.as_ref().map(WorkspaceName::as_str);
    crate::sandbox::files::save_workspace(&state, &mounted, branch, name)?;
    lock(&state.mounts)?.insert(
        mounted.ready.directory.clone(),
        Mount {
            workspace: mounted,
            branch: branch.to_owned(),
        },
    );
    Ok(value)
}

/// Refused `Exists` when one of Sandbox `sandbox`'s Workspace records
/// carries `name` (#536 C3).
fn free(
    sandbox: &str,
    records: &[(std::path::PathBuf, SdkValue)],
    name: Option<&WorkspaceName>,
) -> Result<(), SdkRefusal> {
    let Some(name) = name else {
        return Ok(());
    };
    if (records.iter()).any(|(_, record)| record.get_text("name") == Some(name.as_str())) {
        return Err(crate::common::outcome::refused(
            "Exists",
            format!("Sandbox {sandbox} already has a Workspace named {name}: pick another name, or unmount `{sandbox}:{name}` first"),
        ));
    }
    Ok(())
}

/// `args` with a Workspace named by its name (`path` not absolute: the
/// `<sandbox>:<name>` form, #536 C3) given its path instead: the path of
/// the Workspace record in the Sandbox's folder that carries the name.
/// Refused `NotMounted` when none does.
pub(crate) fn by_name(args: SdkValue) -> Result<SdkValue, SdkRefusal> {
    let Some(name) = args.get_text("path").filter(|path| !path.starts_with('/')) else {
        return Ok(args);
    };
    let name = WorkspaceName::parse(name)?;
    let directory = crate::sandbox::files::sandbox_dir(&args)?;
    let found = (crate::sandbox::files::workspaces(&directory)?.into_iter())
        .find(|(_, record)| record.get_text("name") == Some(name.as_str()));
    let Some((_, record)) = found else {
        let sandbox = args.get_text("sandbox").unwrap_or("");
        return Err(crate::common::outcome::refused(
            "NotMounted",
            format!("{sandbox}:{name} is not mounted"),
        ));
    };
    let path = text(&record, "path")?.to_owned();
    Ok(crate::common::home::with_field(
        args,
        "path",
        SdkValue::Text(path),
    ))
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
    status_with(args, &mut |branch, _| {
        crate::store::service::head(args, branch).ok()
    })
}

/// Reads a Branch's head (Branch id and name), or `None` when it could not.
pub(crate) type HeadOf<'a> = dyn FnMut(BranchId, Option<&str>) -> Option<Option<CommitId>> + 'a;

/// [`status`], the Branch's head read by `head_of` (given the Branch's id
/// and name), or `None` when it could not be read.
pub(crate) fn status_with(args: &SdkValue, head_of: &mut HeadOf<'_>) -> Outcome {
    let state = sandbox(args)?;
    let token = state.token(text(args, "path")?)?;
    let mut control = crate::common::trace::control(&state.controls)?;
    let status = workspace(&mut control).status(token).map_err(operation)?;
    drop(control);
    let branch = lock(&state.mounts)?
        .get(text(args, "path")?)
        .map(|mount| mount.branch.clone());
    let head = head_of(status.binding.branch.id, branch.as_deref());
    let mut value = values::status(&status, head);
    if let SdkValue::Map(fields) = &mut value {
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

/// How one `workspace.unmount` call ends the Workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ending {
    /// The daemon's normal unmount; `force` aborts a live connection instead.
    Normal { force: bool },
    /// Cancel only the holders of this Workspace, then a normal unmount.
    Kill,
}

/// Reads the `force` and `kill` arguments: `kill` accepts only `"workspace"`,
/// it cannot be combined with `force`, and a missing `kill` leaves the
/// existing force path alone. The caller's side (`workspace::calls`) uses it
/// to cancel holders where the runtime is; this side uses it to refuse an
/// invalid combination before any effect.
pub(crate) fn ending(args: &SdkValue) -> Result<Ending, SdkRefusal> {
    let force = args.get_bool("force");
    match args.get_text("kill") {
        None => Ok(Ending::Normal { force }),
        Some("workspace") if !force => Ok(Ending::Kill),
        Some("workspace") => Err(crate::common::outcome::refused(
            "InvalidInput",
            "force and kill cannot be combined: kill: \"workspace\" cancels only this Workspace's holders, then unmounts it normally",
        )),
        Some(other) => Err(crate::common::outcome::refused(
            "InvalidInput",
            format!(
                "kill must be \"workspace\", not {other:?}: only a Workspace's own holders are killed, never another Workspace or the Sandbox"
            ),
        )),
    }
}

/// `workspace.unmount(ws, { discard?, force?, kill? })`: the daemon's answer.
/// `kill: "workspace"` first cancels only this Workspace's foreground execs,
/// then unmounts normally; it never stops or deletes the Sandbox and never
/// touches another Workspace.
pub(crate) fn unmount(args: &SdkValue) -> Outcome {
    let ending = ending(args)?;
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
        if ending == (Ending::Normal { force: true }) {
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
            let _sandbox = crate::sandbox::record::lock_in(&state.directory, &state.id)?;
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
