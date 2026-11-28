//! Workspace calls (#373). `workspace.mount(sandbox, branch)` mounts the new
//! Workspace in the running Sandbox the call names, and nowhere else: there
//! is no placement (owner 2026-10-23), so a mount never starts a Sandbox and
//! takes no project lock; `sandbox.create` is the only start. A Workspace is
//! `{ sandbox, path }`; status, Commit and unmount go to its Sandbox's
//! daemon. Nothing is retried: a refused or failed step is the call's answer.
//! A Sandbox whose daemon ended itself when idle is refused `NoSuchSandbox`
//! by a mount and reclaimed by its `sandbox.remove`. Each Workspace is
//! recorded in its Sandbox's folder, `workspace/<id>.json`, with the name
//! its mount gave it, if any (unique in the Sandbox; the other calls take
//! it in place of the path, [`by_name`]).
use super::values;
use layerfs_history::{BranchId, CommitId, WorkspaceId};
use layerfs_home::lock::mutex::lock;
use layerfs_home::names::WorkspaceName;
use layerfs_home::record::token::random;
use layerfs_home::reply::outcome::shell_word;
use layerfs_home::reply::outcome::{failed, with_next, Outcome};
use layerfs_home::reply::values::required;
use layerfs_protocol::daemon::{CommitControl, Reply};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_sandbox::internal::workspace;
use layerfs_sandbox::lifecycle::registry::{sandbox, sandbox_live, Mount};
use layerfs_sandbox::lifecycle::serve::WORKSPACES;
use layerfs_sandbox::record::{DaemonRecord, WorkspaceRecord};
use layerfs_sandbox::route::daemon::refusal;
use layerfs_sandbox::route::machine::Host;
use layerfs_sandbox::OperationCause;
use layerfs_sandbox::Unmounted;

/// A mount's `NoSuchSandbox`: `refusal` (its detail kept) with the next
/// step in its message, which agents read.
fn no_sandbox(mut refusal: SdkRefusal) -> SdkRefusal {
    refusal.message.push_str(
        ". Create a Sandbox with `layerfs sandbox create` and mount there: `layerfs workspace mount <sandbox> <branch>` with the `sandbox` it returns",
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
            return Err(layerfs_home::reply::outcome::refused(
                "InvalidInput",
                "workspace.mount names its Sandbox: `layerfs workspace mount <sandbox> <branch>`, with a Sandbox from `layerfs sandbox create` or `layerfs sandbox list`",
            ))
        }
    };
    if branch == "main" {
        // main is the project's line of Layers, not a Branch (#535 terminal pass)
        return Err(layerfs_home::reply::outcome::refused(
            "InvalidInput",
            "main is the project's published line, not a Branch, and is not mounted: fork a Branch from it with `layerfs branch fork <project> <name>` and mount that",
        ));
    }
    let directory = layerfs_sandbox::record::discovery::sandbox_dir(host)?;
    let missing = || {
        no_sandbox(layerfs_home::reply::outcome::refused(
            "NoSuchSandbox",
            format!("no Sandbox {sandbox} runs on this machine"),
        ))
    };
    if layerfs_sandbox::record::discovery::started(&directory)?.is_none() {
        return Err(missing());
    }
    let name = match name {
        Some(text) => Some(WorkspaceName::parse(text)?),
        None => None,
    };
    // a name in use is refused before the daemon is reached
    let recorded = layerfs_sandbox::record::discovery::workspaces(&directory)?;
    free(sandbox, &recorded, name.as_ref())?;
    let state = sandbox_live(host).map_err(|refusal| match refusal.code.as_str() {
        "NoSuchSandbox" => no_sandbox(refusal),
        _ => refusal,
    })?;
    let _guard = layerfs_store::data::project::record::discovery::guard(&state.directory)?;
    // under the Sandbox's guard: a close may have removed it, or mounts filled it
    let Some(saved) = layerfs_sandbox::record::discovery::started(&directory)? else {
        return Err(missing());
    };
    let workspaces = layerfs_sandbox::record::discovery::workspaces(&directory)?;
    // checked again under the guard, where it holds
    free(sandbox, &workspaces, name.as_ref())?;
    if workspaces.len() >= WORKSPACES as usize {
        let refusal = layerfs_home::reply::outcome::refused(
            "Capacity",
            format!(
                "Sandbox {sandbox} already holds {WORKSPACES} Workspaces. Create a new Sandbox with `layerfs sandbox create` and mount there, or list this one's Workspaces with `layerfs workspace list --sandbox {sandbox}` and unmount the ones you are done with (`layerfs workspace unmount <workspace>`)"
            ),
        );
        let fix = format!(
            "layerfs workspace list --sandbox {sandbox}",
            sandbox = shell_word(sandbox)
        );
        return Err(layerfs_home::reply::outcome::with_fix(refusal, fix));
    }
    let project = host.project.as_str();
    let id = WorkspaceId::from_authority(random()?).map_err(failed)?;
    let mounted = {
        let mut control = state.controls.lend()?;
        // The place the mount records, from the records under the same home
        // the Store uses: this Sandbox's machine and generation and that
        // machine's current incarnation. The daemon that binds the mount
        // registers them beside its Workspace, so a later machine
        // incarnation or the Sandbox tombstone can prove the mount dead.
        let home = layerfs_store::data::project::record::discovery::home_of(Some(&host.home))?;
        let place = layerfs_sandbox::record::mount_place(&home, &directory, sandbox)?;
        workspace(&mut control).placed(place).mount_named(
            id,
            (project, branch),
            name.as_ref().map(WorkspaceName::as_str),
        )
    };
    let mounted = mounted.map_err(|failure| match (&failure.bound, &failure.failure.cause) {
        // the daemon's lookup refusal, nothing bound: the call's answer
        (None, OperationCause::Unexpected(Reply::SdkRefused(refusal))) => refusal.clone(),
        _ => refusal::mount_failure(failure),
    })?;
    let mut value = values::mounted(&mounted, branch);
    if let SdkValue::Map(fields) = &mut value {
        fields.push(("sandbox".to_owned(), SdkValue::text(state.id.as_str())));
        fields.push(("container".to_owned(), SdkValue::text(state.name.as_str())));
        // a forwarded Sandbox's runtime is its machine's: its record says
        let image = match &state.runtime {
            Some(runtime) => layerfs_sandbox::backend::select::image(runtime),
            None => saved.image().unwrap_or_default().to_owned(),
        };
        fields.push(("image".to_owned(), SdkValue::text(image)));
        // what its record names (#529)
        fields.push(("platform".to_owned(), platform_of(&saved)));
        fields.push((
            "runtime".to_owned(),
            SdkValue::text(layerfs_sandbox::backend::select::runtime_name(&saved)),
        ));
    }
    if let SdkValue::Map(fields) = &mut value {
        let shown = name.as_ref().map(|name| name.to_string());
        fields.push(("name".to_owned(), SdkValue::opt_text(shown)));
    }
    let name = name.as_ref().map(WorkspaceName::as_str);
    layerfs_sandbox::record::discovery::save_workspace(&state, &mounted, branch, name)?;
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
        return Err(layerfs_home::reply::outcome::refused(
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
    let directory = layerfs_sandbox::record::discovery::sandbox_dir(host)?;
    let found = (layerfs_sandbox::record::discovery::workspaces(&directory)?.into_iter())
        .find(|(_, record)| record.name() == Some(name.as_str()));
    let Some((_, record)) = found else {
        let sandbox = host.sandbox.as_deref().unwrap_or("");
        return Err(layerfs_home::reply::outcome::refused(
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
        layerfs_store::data::branch::head(&open, branch).ok()
    })
}

/// The changes of the Workspace at `path` at or below `under`, from the
/// daemon's one bounded overlay read: `{ entries: [{ path, change, type
/// }], partial }`.
pub fn changed(
    control: &mut layerfs_sandbox::internal::Control,
    path: &str,
    under: Option<&str>,
) -> Outcome {
    let mut args = vec![("path".to_owned(), SdkValue::text(path))];
    args.extend(under.map(|under| ("under".to_owned(), SdkValue::text(under))));
    let method = layerfs_sandbox::route::daemon::channel::operator::CHANGED;
    layerfs_sandbox::route::daemon::channel::operator::call(control, method, SdkValue::Map(args))
        .map_err(refusal::sdk_error)
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
    let mut control = state.controls.lend()?;
    let status = workspace(&mut control)
        .status(token)
        .map_err(refusal::failure)?;
    let changes = changed(&mut control, required(path, "path")?, under)?;
    drop(control);
    let branch = lock(&state.mounts)?
        .get(required(path, "path")?)
        .map(|mount| mount.branch.clone());
    let head = head_of(status.binding.branch.id, branch.as_deref());
    let mut value = values::status(&status, head);
    if let SdkValue::Map(fields) = &mut value {
        fields.push(("branch".to_owned(), SdkValue::opt_text(branch)));
        fields.extend(changes_of(&status, &changes));
    }
    Ok(value)
}

/// A status's `changes`, `partial` and its activity's phase and start, from
/// the daemon's changed-path read (`found`). The phase and start are kept
/// only while the activity that read observed is the Status record's.
fn changes_of(
    status: &layerfs_protocol::daemon::WorkspaceStatus,
    found: &SdkValue,
) -> Vec<(String, SdkValue)> {
    let held = |name: &str| found.get(name).cloned().unwrap_or_default();
    let mut fields = vec![
        ("changes".to_owned(), held("entries")),
        ("partial".to_owned(), held("partial")),
    ];
    if found.get_text("activity") == Some(format!("{:?}", status.activity).as_str()) {
        fields.push(("activityPhase".to_owned(), held("activityPhase")));
        fields.push(("activitySinceMs".to_owned(), held("activitySinceMs")));
    }
    fields
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
        layerfs_store::admin::open::service::call("branch.get_commit", &open, args)
    };
    commit_with(
        host,
        path,
        (force, expected),
        (
            &mut |id, name| {
                let args = SdkValue::map([
                    ("project", SdkValue::text(&host.project)),
                    ("name", SdkValue::text(name?)),
                ]);
                let listed =
                    layerfs_store::admin::open::service::call("project.list_branches", &open, args);
                listed_head(&listed.ok()?, id)
            },
            &mut resolve,
        ),
    )
}

/// Reads a Branch's head as `branch get` answers it, given the Branch's id
/// and name: its newest own Commit, else the Commit (a Layer) it was
/// forked at; `None` when it could not be read.
pub type BranchHead<'a> = dyn FnMut(BranchId, Option<&str>) -> Option<String> + 'a;

/// The head `branch get` answers (its `commit`) of the one Branch a
/// `project.list_branches` answer lists, when that Branch is `id`.
pub fn listed_head(listed: &SdkValue, id: BranchId) -> Option<String> {
    let item = match listed.get("items") {
        Some(SdkValue::List(items)) => items.first()?,
        _ => return None,
    };
    let same = item.get_text("id") == Some(&layerfs_protocol::sdk::hex(id.as_slice()));
    same.then(|| item.get_text("commit").map(str::to_owned))
        .flatten()
}

/// Reads the Commit a text names (`branch.get_commit`'s answer): F-hist's
/// resolver, a full id or a prefix of 8 or more of its characters.
pub type Resolve<'a> = dyn FnMut(&str) -> Outcome + 'a;

/// The daemon's `HeadMoved` for a Commit sent with `--expected`: the head
/// moved after the check above, and the publication refused it with nothing
/// committed. Answered as the check answers a head that is not the expected
/// one: `Conflict`, naming the head as `commit`.
fn moved_from_expected(
    expected: Option<layerfs_history::NodeId>,
    failure: &layerfs_sandbox::OperationFailure,
) -> Option<SdkRefusal> {
    let OperationCause::Remote(refused) = &failure.cause else {
        return None;
    };
    let moved = match (expected, refused.code, &refused.moved) {
        (Some(_), layerfs_protocol::daemon::ControlCode::HeadMoved, Some(moved)) => moved,
        _ => return None,
    };
    let head = layerfs_protocol::sdk::node_text(moved.actual_head.map_or(
        layerfs_history::NodeId::Layer(moved.actual_base),
        layerfs_history::NodeId::Commit,
    ));
    let refusal = layerfs_home::reply::outcome::refused(
        "Conflict",
        format!(
            "the Branch's head moved to {head} after --expected was checked; nothing was committed"
        ),
    );
    let refusal = with_next(refusal, "commit", SdkValue::text(head));
    Some(with_next(refusal, "conflicts", SdkValue::List(Vec::new())))
}

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
        layerfs_home::reply::outcome::refused("Failed", "the Store named no Commit for --expected")
    })
}

/// [`commit`], the Branch's head read by `head_of` when `expected` (a
/// Commit id or prefix, resolved by `resolve`) names the head the Commit is
/// made over: a head that is not it refuses the Commit `Conflict`, naming
/// the head as `commit`, before anything is captured. The head is the one
/// `branch get` answers, so a Branch with no Commit of its own is at the
/// Commit it was forked at (RC-A F2). The expected head also travels with
/// the Commit: the daemon merges only with it, and the publication checks it
/// again inside its own transaction, so a head that moves after this read is
/// refused `HeadMoved` with nothing published, forced or not.
pub fn commit_with(
    host: &Host,
    path: Option<&str>,
    (force, expected): (bool, Option<&str>),
    (head_of, resolve): (&mut BranchHead<'_>, &mut Resolve<'_>),
) -> Outcome {
    let state = sandbox(host)?;
    let path = required(path, "path")?;
    let expected = expected
        .map(|text| expected_id(text, resolve))
        .transpose()?;
    let expected = expected.as_deref();
    let token = state.token(path)?;
    let mut control = state.controls.lend()?;
    let status = match expected {
        Some(_) => Some(
            workspace(&mut control)
                .status(token)
                .map_err(refusal::failure)?,
        ),
        None => None,
    };
    let branch = lock(&state.mounts)?
        .get(path)
        .map(|mount| mount.branch.clone());
    if let (Some(expected), Some(status)) = (expected, &status) {
        let head = head_of(status.binding.branch.id, branch.as_deref()).ok_or_else(|| {
            layerfs_home::reply::outcome::refused(
                "Unavailable",
                "the Branch's head could not be read to check --expected; nothing was committed",
            )
        })?;
        if head != expected {
            let refusal = layerfs_home::reply::outcome::refused(
                "Conflict",
                format!(
                    "the Branch's head is {head}, not --expected {expected}; nothing was committed"
                ),
            );
            let refusal = with_next(refusal, "commit", SdkValue::text(head));
            return Err(with_next(refusal, "conflicts", SdkValue::List(Vec::new())));
        }
    }
    let expected = expected
        .map(|text| {
            layerfs_protocol::sdk::unhex(text)
                .and_then(|bytes| layerfs_history::NodeId::from_slice(&bytes).ok())
                .ok_or_else(|| {
                    layerfs_home::reply::outcome::refused(
                        "Failed",
                        "the Store named no Commit id for --expected; nothing was committed",
                    )
                })
        })
        .transpose()?;
    let reply = workspace(&mut control)
        .commit_with(CommitControl {
            token,
            force,
            message: host.message.text().map(str::to_owned),
            expected,
        })
        .map_err(|failure| match moved_from_expected(expected, &failure) {
            Some(refusal) => refusal,
            None => refusal::failure(failure),
        })?;
    let value = values::commit(&reply)?;
    // the Layer the Branch was forked at, from the mount this process
    // holds (no read): an unchanged fresh fork's head (call surface 2.0)
    let layer = lock(&state.mounts)?.get(path).map(|mount| {
        let branch = &mount.workspace.bound.binding.branch;
        layerfs_protocol::sdk::hex(branch.base_layer.as_slice())
    });
    let value = layerfs_home::disk::with_field(value, "baseLayer", SdkValue::opt_text(layer));
    Ok(layerfs_home::disk::with_field(
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
    if let Some(value) = layerfs_sandbox::record::unmount_guard::find(&state, path)? {
        if !layerfs_sandbox::record::unmount_guard::admits(&value, force) {
            return layerfs_sandbox::record::unmount_guard::repeat(&value);
        }
    }
    let guard = layerfs_store::data::project::record::discovery::guard(&state.directory)?;
    // Another process may have registered while this call waited for the
    // short admission section. Check again before sending anything.
    if let Some(value) = layerfs_sandbox::record::unmount_guard::find(&state, path)? {
        if !layerfs_sandbox::record::unmount_guard::admits(&value, force) {
            return layerfs_sandbox::record::unmount_guard::repeat(&value);
        }
    }
    let token = state.unmount_token(path)?;
    // its running commands (D-10): `Busy`, or with `force` cancelled first
    let wsid = layerfs_protocol::sdk::hex(&token.workspace.to_bytes());
    let unreached = &mut layerfs_sandbox::exec::settle::unreached;
    layerfs_sandbox::exec::settle::settle(
        &state.directory,
        &state.id,
        Some(&wsid),
        force,
        unreached,
    )?;
    // One short deadline, not the full admission wait: a Workspace unmount
    // must not hold the folder guard while a refusing daemon keeps the
    // ordinary 60 s control wait running (#540).
    layerfs_sandbox::record::discovery::refresh_within(&state, UNMOUNT_STATUS_WAIT)?;
    // the daemon's kernel probe answers Busy for a process still holding it
    let attempt = layerfs_sandbox::record::unmount_guard::begin(&state, token, path, force)?;
    drop(guard);
    // Only this Workspace's lease remains held. Other Workspaces and
    // processes can take Sandbox ownership while the daemon helper waits.
    let mut sent = false;
    let outcome = (|| {
        let mut control = state.controls.lend()?;
        sent = true;
        let mut api = workspace(&mut control);
        // A request whose sending was never attempted reached no daemon:
        // the attempt's record is undone like a call that never connected.
        let mut answered = |failure: Box<layerfs_sandbox::OperationFailure>| {
            if matches!(&failure.cause, OperationCause::Exchange(exchange) if !exchange.attempted) {
                sent = false;
            }
            refusal::failure(failure)
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
        layerfs_sandbox::record::unmount_guard::cancel(&attempt)
    } else {
        layerfs_sandbox::record::unmount_guard::finish(&attempt, path, &outcome)
    };
    if let Err(error) = publication {
        return layerfs_sandbox::record::unmount_guard::cleanup_failure(outcome, error);
    }
    let outcome = match outcome {
        Err(refusal) if refusal.code == "Dirty" => Err(dirty(&state, path, refusal)),
        other => other,
    };
    if outcome.is_ok() {
        let cleanup = (|| {
            let _guard = layerfs_store::data::project::record::discovery::guard(&state.directory)?;
            let _sandbox = layerfs_sandbox::record::lock_in(&state.directory, &state.id)?;
            let record = layerfs_sandbox::record::discovery::workspace_file(&state, token);
            match std::fs::remove_file(&record) {
                Ok(()) => layerfs_home::record::generation::moved_above(&record),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(failed(error)),
            }
            // its folder, with any command record left in it
            let folder = layerfs_sandbox::record::exec::workspace_dir(&state.directory, &wsid);
            match std::fs::remove_dir_all(folder) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                    return Err(failed(error))
                }
                _ => {}
            }
            // its commands' output files, beside the daemon's log
            if let Ok(runtime) = state.runtime() {
                layerfs_sandbox::exec::output::remove(
                    &layerfs_sandbox::backend::select::scratch(runtime),
                    &wsid,
                )?;
            }
            lock(&state.mounts)?.remove(path);
            Ok(())
        })();
        if let Err(error) = cleanup {
            return layerfs_sandbox::record::unmount_guard::cleanup_failure(outcome, error);
        }
    }
    outcome
}

/// A Dirty refusal of the Workspace at `path`, with what [`dirty_fields`]
/// lists from one bounded overlay read and V22-7's fix.
fn dirty(
    state: &layerfs_sandbox::lifecycle::registry::SandboxState,
    path: &str,
    refusal: SdkRefusal,
) -> SdkRefusal {
    let read = state
        .controls
        .lend()
        .and_then(|mut control| changed(&mut control, path, None));
    layerfs_home::reply::outcome::with_fix(
        dirty_fields(refusal, read.ok().as_ref()),
        format!(
            "layerfs workspace commit {}:{}",
            shell_word(&state.id),
            shell_word(&path_name(state, path))
        ),
    )
}

/// The fields a Dirty `refusal` names for the next call, as `workspace
/// status`'s short form lists them (call surface 2.0): `changes` (the first
/// [`layerfs_home::reply::values::SHOWN`] entries of the overlay read `read`, each `{ path, change,
/// type }`, `type` left out for a file), `changes_count` only beside a cut
/// list (RC-A F3), and `pending_merge`. With no read, the daemon's own
/// paths stand, each as `{ path }`.
pub fn dirty_fields(refusal: SdkRefusal, read: Option<&SdkValue>) -> SdkRefusal {
    let (entries, partial) = match read.and_then(|found| found.get("entries")) {
        Some(SdkValue::List(entries)) => (
            entries.iter().map(status_entry).collect(),
            read.is_some_and(|found| found.get_bool("partial")),
        ),
        _ => match refusal.detail.get("changed") {
            Some(SdkValue::List(paths)) => (
                (paths.iter())
                    .map(|path| SdkValue::map([("path", path.clone())]))
                    .collect(),
                refusal.detail.get_bool("truncated"),
            ),
            _ => (Vec::new(), false),
        },
    };
    let pending = refusal.detail.get_bool("pendingMerge");
    let (shown, count) = layerfs_home::reply::values::shown(entries, 0, partial);
    let mut refusal = with_next(refusal, "changes", shown);
    if count != SdkValue::Null {
        refusal = with_next(refusal, "changes_count", count);
    }
    with_next(refusal, "pending_merge", SdkValue::Bool(pending))
}

/// One change as `workspace status`'s short form shows it: `type` left
/// out when it is `file`, the usual type.
fn status_entry(entry: &SdkValue) -> SdkValue {
    match entry {
        SdkValue::Map(fields) => SdkValue::Map(
            (fields.iter())
                .filter(|(name, value)| !(name == "type" && *value == SdkValue::text("file")))
                .cloned()
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The name of the Workspace at `path` in its Sandbox's records, else its
/// path.
fn path_name(state: &layerfs_sandbox::lifecycle::registry::SandboxState, path: &str) -> String {
    (layerfs_sandbox::record::discovery::workspaces(&state.directory)
        .ok()
        .into_iter()
        .flatten())
    .find(|(_, record)| record.path() == Some(path))
    .and_then(|(_, record)| record.name().map(str::to_owned))
    .unwrap_or_else(|| path.to_owned())
}
