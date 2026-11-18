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
use crate::record::{DaemonRecord, SandboxRecord, WorkspaceRecord};
use crate::registry::{sandbox, sandbox_live, Mount};
use crate::OperationCause;
use crate::Unmounted;
use layerfs_bridge::control::{CommitControl, MountPlace, Reply, SdkRefusal, SdkValue};
use layerfs_history::{BranchId, CommitId, WorkspaceId};
use layerfs_home::helpers::{lock, random, required};
use layerfs_home::names::WorkspaceName;
use layerfs_home::outcome::{failed, with_next, Outcome};

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
        // This Sandbox's record names the machine it runs on; the daemon
        // that binds the mount registers both names beside its Workspace.
        let machine =
            SandboxRecord::read_in(&directory, sandbox)?.map(|record| record.machine().to_owned());
        let place = MountPlace::new(Some(state.id.as_str()), machine.as_deref());
        workspace(&mut control)
            .placed(place.sandbox.as_deref(), place.machine.as_deref())
            .mount_named(id, (project, branch))
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
/// daemon's Status, which reads no Store, its changes at or below `under`
/// from one bounded overlay read ([`changed`]), and the Branch head from
/// one newest-first History read in this process.
pub fn status(host: &Host, path: Option<&str>, under: Option<&str>) -> Outcome {
    let open = host.open_args();
    status_with(host, (path, under), &mut |branch, _| {
        layerfs_store::service::head(&open, branch).ok()
    })
}

/// The changes of the Workspace at `path` at or below `under`, from the
/// daemon's one bounded overlay read: `{ entries: [{ path, change, type
/// }], partial }`.
pub fn changed(control: &mut crate::internal::Control, path: &str, under: Option<&str>) -> Outcome {
    let mut args = vec![("path".to_owned(), SdkValue::text(path))];
    args.extend(under.map(|under| ("under".to_owned(), SdkValue::text(under))));
    let method = crate::daemon::operator::CHANGED;
    crate::daemon::operator::call(control, method, SdkValue::Map(args)).map_err(|error| match error
    {
        crate::SdkError::Refused(refusal) => refusal,
        crate::SdkError::Operation(failure) => {
            values::refusal(&failure).unwrap_or_else(|| failed(failure))
        }
        crate::SdkError::Mount(failure) => failed(failure),
    })
}

/// Reads a Branch's head (Branch id and name), or `None` when it could not.
pub type HeadOf<'a> = dyn FnMut(BranchId, Option<&str>) -> Option<Option<CommitId>> + 'a;

/// [`status`], the Branch's head read by `head_of` (given the Branch's id
/// and name), or `None` when it could not be read.
pub fn status_with(
    host: &Host,
    (path, under): (Option<&str>, Option<&str>),
    head_of: &mut HeadOf<'_>,
) -> Outcome {
    let state = sandbox(host)?;
    let token = state.token(required(path, "path")?)?;
    let mut control = crate::daemon::trace::control(&state.controls)?;
    let status = workspace(&mut control).status(token).map_err(operation)?;
    let changes = changed(&mut control, required(path, "path")?, under)?;
    drop(control);
    let branch = lock(&state.mounts)?
        .get(required(path, "path")?)
        .map(|mount| mount.branch.clone());
    let head = head_of(status.binding.branch.id, branch.as_deref());
    let mut value = values::status(&status, head);
    if let SdkValue::Map(fields) = &mut value {
        fields.push(("branch".to_owned(), SdkValue::opt_text(branch)));
        let held = |name: &str| changes.get(name).cloned().unwrap_or_default();
        fields.push(("changes".to_owned(), held("entries")));
        fields.push(("partial".to_owned(), held("partial")));
    }
    Ok(value)
}

/// `workspace.commit(ws, { message?, force?, expected? })` of the
/// Workspace at `path`, the Branch's head read in this process for
/// `expected`.
pub fn commit(host: &Host, path: Option<&str>, force: bool, expected: Option<&str>) -> Outcome {
    let open = host.open_args();
    let mut resolve = |text: &str| {
        let args = SdkValue::map([
            ("project", SdkValue::text(&host.project)),
            ("commit", SdkValue::text(text)),
        ]);
        layerfs_store::service::call("branch.get_commit", &open, args)
    };
    commit_with(
        host,
        path,
        (force, expected),
        (
            &mut |branch, _| layerfs_store::service::head(&open, branch).ok(),
            &mut resolve,
        ),
    )
}

/// Reads the Commit a text names (`branch.get_commit`'s answer): F-hist's
/// resolver, a full id or a prefix of 8 or more of its characters.
pub type Resolve<'a> = dyn FnMut(&str) -> Outcome + 'a;

/// The full id of the Commit `--expected` names, resolved in the Store: a
/// prefix more than one Commit starts with is refused `InvalidInput` with
/// `matches` beside it; one no Commit has, `NoSuchCommit`.
fn expected_id(text: &str, resolve: &mut Resolve<'_>) -> Result<String, SdkRefusal> {
    let found = resolve(text).map_err(|refusal| {
        match (
            refusal.code == "InvalidInput",
            refusal.detail.get("matches"),
        ) {
            (true, Some(matches)) => {
                let matches = matches.clone();
                with_next(refusal, "matches", matches)
            }
            _ => refusal,
        }
    })?;
    found.get_text("commit").map(str::to_owned).ok_or_else(|| {
        layerfs_home::outcome::refused("Failed", "the Store named no Commit for --expected")
    })
}

/// [`commit`], the Branch's head read by `head_of` when `expected` (a
/// Commit id or prefix, resolved by `resolve`) names the head the Commit is
/// made over: a head that is not it refuses the Commit `Conflict`, naming
/// the head as `commit`, before anything is captured. The check is one read before the Commit
/// is sent; a head that moves after it is merged as any Commit merges.
pub fn commit_with(
    host: &Host,
    path: Option<&str>,
    (force, expected): (bool, Option<&str>),
    (head_of, resolve): (&mut HeadOf<'_>, &mut Resolve<'_>),
) -> Outcome {
    let state = sandbox(host)?;
    let path = required(path, "path")?;
    let expected = expected
        .map(|text| expected_id(text, resolve))
        .transpose()?;
    let expected = expected.as_deref();
    let token = state.token(path)?;
    let mut control = crate::daemon::trace::control(&state.controls)?;
    let status = match expected {
        Some(_) => Some(workspace(&mut control).status(token).map_err(operation)?),
        None => None,
    };
    let branch = lock(&state.mounts)?
        .get(path)
        .map(|mount| mount.branch.clone());
    if let (Some(expected), Some(status)) = (expected, &status) {
        let head = head_of(status.binding.branch.id, branch.as_deref()).ok_or_else(|| {
            layerfs_home::outcome::refused(
                "Unavailable",
                "the Branch's head could not be read to check --expected; nothing was committed",
            )
        })?;
        let shown = head.map(|id| layerfs_bridge::sdk_values::hex(id.as_slice()));
        if shown.as_deref() != Some(expected) {
            let refusal = layerfs_home::outcome::refused(
                "Conflict",
                format!(
                    "the Branch's head is {}, not --expected {expected}; nothing was committed",
                    shown.as_deref().unwrap_or("none")
                ),
            );
            let refusal = with_next(refusal, "commit", SdkValue::opt_text(shown));
            let refusal = with_next(refusal, "conflicts", SdkValue::List(Vec::new()));
            return Err(with_next(refusal, "conflicts_count", SdkValue::Int(0)));
        }
    }
    let reply = workspace(&mut control)
        .commit_with(CommitControl {
            token,
            force,
            message: host.message.text().map(str::to_owned),
        })
        .map_err(operation)?;
    let value = values::commit(&reply)?;
    Ok(layerfs_home::home::with_field(
        value,
        "branch",
        SdkValue::opt_text(branch),
    ))
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
        if !(force && crate::unmount_guard::forceable(&value)) {
            return crate::unmount_guard::repeat(&value);
        }
    }
    let guard = layerfs_store::project::discovery::guard(&state.directory)?;
    // Another process may have registered while this call waited for the
    // short admission section. Check again before sending anything.
    if let Some(value) = crate::unmount_guard::find(&state, path)? {
        if !(force && crate::unmount_guard::forceable(&value)) {
            return crate::unmount_guard::repeat(&value);
        }
    }
    let token = state.unmount_token(path)?;
    // its running commands (D-10): `Busy`, or with `force` cancelled first
    let wsid = layerfs_bridge::sdk_values::hex(&token.workspace.to_bytes());
    let unreached = &mut crate::exec::cancel::unreached;
    crate::exec::cancel::settle(&state.directory, &state.id, Some(&wsid), force, unreached)?;
    // One short deadline, not the full admission wait: a Workspace unmount
    // must not hold the folder guard while a refusing daemon keeps the
    // ordinary 60 s control wait running (#540).
    crate::files::refresh_within(&state, UNMOUNT_STATUS_WAIT)?;
    // the daemon's kernel probe answers Busy for a process still holding it
    let attempt = crate::unmount_guard::begin(&state, token, path, force)?;
    drop(guard);
    // Only this Workspace's lease remains held. Other Workspaces and
    // processes can take Sandbox ownership while the daemon helper waits.
    let mut sent = false;
    let outcome = (|| {
        let mut control = crate::daemon::trace::control(&state.controls)?;
        sent = true;
        let mut api = workspace(&mut control);
        // A request whose sending was never attempted reached no daemon:
        // the attempt's record is undone like a call that never connected.
        let mut answered = |failure: Box<crate::OperationFailure>| {
            if matches!(&failure.cause, OperationCause::Exchange(exchange) if !exchange.attempted) {
                sent = false;
            }
            operation(failure)
        };
        // Forced: one abort write ends the connection, also after a normal
        // unmount stalled at Join; uncommitted changes are dropped.
        if force {
            match api.unmount(token, true).map_err(&mut answered)? {
                Some(outcome) => Ok(values::force_unmounted(&outcome)),
                None => values::unmounted(&Unmounted::Closed),
            }
        } else {
            match api.unmount_checked(token, discard) {
                Ok(answer) => values::unmounted(&answer),
                Err(error) => Err(answered(error)),
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
        crate::unmount_guard::finish(&attempt, path, &outcome)
    };
    if let Err(error) = publication {
        return crate::unmount_guard::cleanup_failure(outcome, error);
    }
    let outcome = match outcome {
        Err(refusal) if refusal.code == "Dirty" => Err(dirty(&state, path, refusal)),
        other => other,
    };
    if outcome.is_ok() {
        let cleanup = (|| {
            let _guard = layerfs_store::project::discovery::guard(&state.directory)?;
            let _sandbox = crate::record::lock_in(&state.directory, &state.id)?;
            let record = crate::files::workspace_file(&state, token);
            match std::fs::remove_file(&record) {
                Ok(()) => layerfs_home::home::generation::moved_above(&record),
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
            // its commands' output files, beside the daemon's log
            if let Ok(runtime) = state.runtime() {
                crate::output::remove(&crate::runtime::scratch(runtime), &wsid)?;
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

/// The entries a Dirty refusal of the Workspace at `path` lists, as
/// `workspace status` lists them (call surface 2.0): `changes` (the first
/// [`SHOWN`] entries of one bounded overlay read, each `{ path, change,
/// type }`), `changes_count` (the entries read) and `pending_merge`. When
/// that read fails, the daemon's own paths stand, each as `{ path }`.
fn dirty(state: &crate::registry::SandboxState, path: &str, refusal: SdkRefusal) -> SdkRefusal {
    let read = crate::daemon::trace::control(&state.controls)
        .and_then(|mut control| changed(&mut control, path, None));
    let entries = match read.ok().and_then(|found| found.get("entries").cloned()) {
        Some(SdkValue::List(entries)) => entries,
        _ => match refusal.detail.get("changed") {
            Some(SdkValue::List(paths)) => (paths.iter())
                .map(|path| SdkValue::map([("path", path.clone())]))
                .collect(),
            _ => Vec::new(),
        },
    };
    let pending = refusal.detail.get_bool("pendingMerge");
    let count = SdkValue::Int(entries.len() as i64);
    let shown = SdkValue::List(entries.into_iter().take(SHOWN).collect());
    let refusal = with_next(refusal, "changes", shown);
    let refusal = with_next(refusal, "changes_count", count);
    let refusal = with_next(refusal, "pending_merge", SdkValue::Bool(pending));
    layerfs_home::outcome::with_fix(
        refusal,
        format!(
            "layerfs workspace commit {}:{} -m <message>",
            state.id,
            path_name(state, path)
        ),
    )
}

/// The entries a refusal or a status shows of a bounded read.
pub const SHOWN: usize = 20;

/// The name of the Workspace at `path` in its Sandbox's records, else its
/// path.
fn path_name(state: &crate::registry::SandboxState, path: &str) -> String {
    (crate::files::workspaces(&state.directory)
        .ok()
        .into_iter()
        .flatten())
    .find(|(_, record)| record.path() == Some(path))
    .and_then(|(_, record)| record.name().map(str::to_owned))
    .unwrap_or_else(|| path.to_owned())
}
