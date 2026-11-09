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
use crate::daemon::values;
use crate::host::Host;
use crate::internal::workspace;
use crate::lifecycle::{operation, WORKSPACES};
use crate::record::{DaemonRecord, WorkspaceRecord};
use crate::registry::{sandbox, sandbox_live, Mount};
use crate::OperationCause;
use crate::Unmounted;
use layerfs_bridge::control::{CommitControl, Reply, SdkRefusal, SdkValue};
use layerfs_history::{BranchId, CommitId, WorkspaceId};
use layerfs_home::helpers::{lock, random, required};
use layerfs_home::names::WorkspaceName;
use layerfs_home::outcome::{failed, Outcome};

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
/// guard is taken; its record and count are checked under it. The daemon
/// finds the Branch by its project's name and its own on the Workspace's
/// Store reader (#64): this process reads no Store, on every route, and a
/// missing project or Branch is the daemon's `NoSuchProject` or
/// `NoSuchBranch`, as the Store service's lookup refuses it.
pub fn mount(host: &Host, branch: Option<&str>, name: Option<&str>) -> Outcome {
    let branch = required(branch, "branch")?;
    let sandbox = match host.sandbox.as_deref() {
        Some(id) if !id.is_empty() => id,
        _ => {
            return Err(layerfs_home::outcome::refused(
                "InvalidInput",
                "workspace.mount names its Sandbox: workspace.mount(sandbox, branch), with a Sandbox from `sandbox.create()` or `project.list_sandboxes()`",
            ))
        }
    };
    if branch == "main" {
        // main is the project's line of Layers, not a Branch (#535 terminal pass)
        return Err(layerfs_home::outcome::refused(
            "InvalidInput",
            "main is the project's published line, not a Branch, and is not mounted: fork a Branch from it with `branch.fork(project, name, \"main\")` and mount that",
        ));
    }
    let directory = crate::files::sandbox_dir(host)?;
    let missing = || {
        no_sandbox(layerfs_home::outcome::refused(
            "NoSuchSandbox",
            format!("no Sandbox {sandbox} runs on this machine"),
        ))
    };
    if crate::files::started(&directory)?.is_none() {
        return Err(missing());
    }
    let name = match name {
        Some(text) => Some(WorkspaceName::parse(text)?),
        None => None,
    };
    // a name in use is refused before the daemon is reached
    let recorded = crate::files::workspaces(&directory)?;
    free(sandbox, &recorded, name.as_ref())?;
    let state = sandbox_live(host).map_err(|refusal| match refusal.code.as_str() {
        "NoSuchSandbox" => no_sandbox(refusal),
        _ => refusal,
    })?;
    let _guard = layerfs_store::project::discovery::guard(&state.directory)?;
    // under the Sandbox's guard: a close may have removed it, or mounts filled it
    let Some(saved) = crate::files::started(&directory)? else {
        return Err(missing());
    };
    let workspaces = crate::files::workspaces(&directory)?;
    // checked again under the guard, where it holds
    free(sandbox, &workspaces, name.as_ref())?;
    if workspaces.len() >= WORKSPACES as usize {
        return Err(layerfs_home::outcome::refused(
            "Capacity",
            format!(
                "Sandbox {sandbox} already holds {WORKSPACES} Workspaces. Create a new Sandbox with `sandbox.create()` and mount there, or list this one's Workspaces with `sandbox.list_workspaces(\"{sandbox}\")` and unmount the ones you are done with (`workspace.unmount(ws)`)"
            ),
        ));
    }
    let project = host.project.as_str();
    let id = WorkspaceId::from_authority(random()?).map_err(failed)?;
    let mounted = {
        let mut control = crate::daemon::trace::control(&state.controls)?;
        workspace(&mut control).mount_named(id, (project, branch))
    };
    let mounted = mounted.map_err(|failure| match (&failure.bound, &failure.failure.cause) {
        // the daemon's lookup refusal, nothing bound: the call's answer
        (None, OperationCause::Unexpected(Reply::SdkRefused(refusal))) => refusal.clone(),
        _ => failed(failure),
    })?;
    let mut value = values::mounted(&mounted, branch);
    if let SdkValue::Map(fields) = &mut value {
        fields.push(("sandbox".to_owned(), SdkValue::text(state.id.as_str())));
        fields.push(("container".to_owned(), SdkValue::text(state.name.as_str())));
        // a forwarded Sandbox's runtime is its machine's: its record says
        let image = match &state.runtime {
            Some(runtime) => crate::runtime::image(runtime),
            None => saved.image().unwrap_or_default().to_owned(),
        };
        fields.push(("image".to_owned(), SdkValue::text(image)));
        // what its record names (#529)
        fields.push(("platform".to_owned(), platform_of(&saved)));
        fields.push((
            "runtime".to_owned(),
            SdkValue::text(crate::runtime::runtime_name(&saved)),
        ));
    }
    if let SdkValue::Map(fields) = &mut value {
        let shown = name.as_ref().map(|name| name.to_string());
        fields.push(("name".to_owned(), SdkValue::opt_text(shown)));
    }
    let name = name.as_ref().map(WorkspaceName::as_str);
    crate::files::save_workspace(&state, &mounted, branch, name)?;
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
    records: &[(std::path::PathBuf, WorkspaceRecord)],
    name: Option<&WorkspaceName>,
) -> Result<(), SdkRefusal> {
    let Some(name) = name else {
        return Ok(());
    };
    if (records.iter()).any(|(_, record)| record.name() == Some(name.as_str())) {
        return Err(layerfs_home::outcome::refused(
            "Exists",
            format!("Sandbox {sandbox} already has a Workspace named {name}: pick another name, or unmount `{sandbox}:{name}` first"),
        ));
    }
    Ok(())
}

/// The Workspace `path` names, by its name when it is not absolute (the
/// `<sandbox>:<name>` form, #536 C3): the path of the Workspace record in
/// the Sandbox's folder that carries the name. Refused `NotMounted` when
/// none does.
pub fn by_name(host: &Host, path: Option<&str>) -> Result<Option<String>, SdkRefusal> {
    let Some(name) = path.filter(|path| !path.starts_with('/')) else {
        return Ok(path.map(str::to_owned));
    };
    let name = WorkspaceName::parse(name)?;
    let directory = crate::files::sandbox_dir(host)?;
    let found = (crate::files::workspaces(&directory)?.into_iter())
        .find(|(_, record)| record.name() == Some(name.as_str()));
    let Some((_, record)) = found else {
        let sandbox = host.sandbox.as_deref().unwrap_or("");
        return Err(layerfs_home::outcome::refused(
            "NotMounted",
            format!("{sandbox}:{name} is not mounted"),
        ));
    };
    let path = required(record.path(), "path")?.to_owned();
    Ok(Some(path))
}

/// A record's `platform` (#529), or null for a record from before it, which
/// the agent reads as the platform its runtime and home imply.
fn platform_of(record: &DaemonRecord) -> SdkValue {
    SdkValue::opt_text(record.platform().map(str::to_owned))
}

/// `backend.daemon.workspace.status(ws)` of the Workspace at `path`: the
/// daemon's Status, which reads no Store, and the Branch head from one
/// newest-first History read in this process.
pub fn status(host: &Host, path: Option<&str>) -> Outcome {
    let open = host.open_args();
    status_with(host, path, &mut |branch, _| {
        layerfs_store::service::head(&open, branch).ok()
    })
}

/// Reads a Branch's head (Branch id and name), or `None` when it could not.
pub type HeadOf<'a> = dyn FnMut(BranchId, Option<&str>) -> Option<Option<CommitId>> + 'a;

/// [`status`], the Branch's head read by `head_of` (given the Branch's id
/// and name), or `None` when it could not be read.
pub fn status_with(host: &Host, path: Option<&str>, head_of: &mut HeadOf<'_>) -> Outcome {
    let state = sandbox(host)?;
    let token = state.token(required(path, "path")?)?;
    let mut control = crate::daemon::trace::control(&state.controls)?;
    let status = workspace(&mut control).status(token).map_err(operation)?;
    drop(control);
    let branch = lock(&state.mounts)?
        .get(required(path, "path")?)
        .map(|mount| mount.branch.clone());
    let head = head_of(status.binding.branch.id, branch.as_deref());
    let mut value = values::status(&status, head);
    if let SdkValue::Map(fields) = &mut value {
        fields.push(("branch".to_owned(), SdkValue::opt_text(branch)));
    }
    Ok(value)
}

/// `workspace.commit(ws, { message?, force? })` of the Workspace at `path`.
pub fn commit(host: &Host, path: Option<&str>, force: bool) -> Outcome {
    let state = sandbox(host)?;
    let token = state.token(required(path, "path")?)?;
    let reply = workspace(&mut *crate::daemon::trace::control(&state.controls)?)
        .commit_with(CommitControl {
            token,
            force,
            message: host.message.text().map(str::to_owned),
        })
        .map_err(operation)?;
    values::commit(&reply)
}

/// How long unmount's Workspace status probe waits for the daemon. After
/// slot reclamation the ordinary path succeeds; a truly exhausted daemon
/// returns `Capacity` in bounded time instead of blocking behind the guard.
const UNMOUNT_STATUS_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// `workspace.unmount(ws, { discard?, force? })` of the Workspace at
/// `path`: the daemon's answer. It is refused `Busy` while a command of the
/// Workspace runs; `force` cancels them first (D-10). A stale command
/// record is removed and logged. Its folder (`workspace/<id>/`) goes with
/// it.
pub fn unmount(host: &Host, path: Option<&str>, discard: bool, force: bool) -> Outcome {
    let state = sandbox(host)?;
    let path = required(path, "path")?;
    if let Some(value) = crate::unmount_guard::find(&state, path)? {
        if !crate::unmount_guard::retry(&value) {
            return crate::unmount_guard::repeat(&value);
        }
        // A recorded forced detach refused `EBUSY`: fall through and ask the
        // daemon to re-attempt that detach once the holder is gone.
    }
    let guard = layerfs_store::project::discovery::guard(&state.directory)?;
    // Another process may have registered while this call waited for the
    // short admission section. Check again before sending anything.
    let retrying = match crate::unmount_guard::find(&state, path)? {
        Some(value) if !crate::unmount_guard::retry(&value) => {
            return crate::unmount_guard::repeat(&value);
        }
        Some(_) => true,
        None => false,
    };
    // A retryable Retained Workspace's live mount table no longer answers:
    // read its token from the workspace record (as `sandbox.close` does)
    // instead of re-running `unmount_guard::check`, which would re-report
    // the Retained custody before the retry ever reaches the daemon (#538).
    let token = if retrying {
        crate::unmount_guard::token(&state, path)?
    } else {
        state.token(path)?
    };
    // its running commands (D-10): `Busy`, or with `force` cancelled first
    let wsid = layerfs_bridge::sdk_values::hex(&token.workspace.to_bytes());
    let unreached = &mut crate::exec::cancel::unreached;
    crate::exec::cancel::settle(&state.directory, &state.id, Some(&wsid), force, unreached)?;
    // One short deadline, not the full admission wait: a Workspace unmount
    // must not hold the folder guard while a refusing daemon keeps the
    // ordinary 60 s control wait running (#540).
    crate::files::refresh_within(&state, UNMOUNT_STATUS_WAIT)?;
    // the daemon's kernel probe answers Busy for a process still holding it
    let attempt = crate::unmount_guard::begin(&state, token, path)?;
    drop(guard);
    // Only this Workspace's lease remains held. Other Workspaces and
    // processes can take Sandbox ownership while the daemon helper waits.
    let mut sent = false;
    let mut retryable = false;
    let outcome = (|| {
        let mut control = crate::daemon::trace::control(&state.controls)?;
        sent = true;
        let mut api = workspace(&mut control);
        // Forced: one abort write ends the connection, also after a normal
        // unmount stalled at Join; uncommitted changes are dropped.
        if force {
            match api.unmount(token, true) {
                Ok(Some(outcome)) => Ok(values::force_unmounted(&outcome)),
                Ok(None) => values::unmounted(&Unmounted::Closed),
                Err(error) => {
                    retryable = crate::unmount_guard::retryable(&error);
                    Err(operation(error))
                }
            }
        } else {
            match api.unmount_checked(token, discard) {
                Ok(answer) => values::unmounted(&answer),
                Err(error) => {
                    retryable = crate::unmount_guard::retryable(&error);
                    Err(operation(error))
                }
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
        crate::unmount_guard::cancel(&attempt)
    } else {
        crate::unmount_guard::finish(&attempt, path, &outcome, retryable)
    };
    if let Err(error) = publication {
        return crate::unmount_guard::cleanup_failure(outcome, error);
    }
    if outcome.is_ok() {
        let cleanup = (|| {
            let _guard = layerfs_store::project::discovery::guard(&state.directory)?;
            let _sandbox = crate::record::lock_in(&state.directory, &state.id)?;
            match std::fs::remove_file(crate::files::workspace_file(&state, token)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(failed(error)),
            }
            // its folder, with any command record left in it
            let folder = crate::exec::record::workspace_dir(&state.directory, &wsid);
            match std::fs::remove_dir_all(folder) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                    return Err(failed(error))
                }
                _ => {}
            }
            lock(&state.mounts)?.remove(path);
            Ok(())
        })();
        if let Err(error) = cleanup {
            return crate::unmount_guard::cleanup_failure(outcome, error);
        }
    }
    outcome
}
