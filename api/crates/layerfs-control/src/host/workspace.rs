//! Workspace calls (#373). `workspace.mount(sandbox, branch)` mounts the new
//! Workspace in the running Sandbox the call names, and nowhere else: there
//! is no placement (owner 2026-10-23), so a mount never starts a Sandbox and
//! takes no project lock; `sandbox.create` is the only start. A Workspace is
//! `{ sandbox, path }`; status, Commit and unmount go to its Sandbox's
//! daemon. Nothing is retried: a refused or failed step is the call's answer.
//! A Sandbox whose daemon ended itself when idle is refused `NoSuchSandbox`
//! by a mount and reclaimed by the next `sandbox.create` or
//! `runtime.sandbox.reclaim`.
use crate::common::outcome::{failed, Outcome};
use crate::host::registry::{lock, naming, random, sandbox, sandbox_live, text, Mount};
use crate::host::sandbox::{operation, WORKSPACES};
use crate::internal::workspace;
use crate::sandbox::{values, Unmounted};
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
    let directory = crate::host::discovery::sandbox_dir(args, sandbox)?;
    let record = directory.join("sandbox.record");
    let missing = || {
        no_sandbox(crate::common::outcome::refused(
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
    let _guard = crate::host::discovery::guard(&state.directory)?;
    // under the Sandbox's guard: a close may have removed it, or mounts filled it
    if !record.exists() {
        return Err(missing());
    }
    if recorded(&directory)? >= WORKSPACES as usize {
        return Err(crate::common::outcome::refused(
            "Capacity",
            format!(
                "Sandbox {sandbox} already holds {WORKSPACES} Workspaces. Create a new Sandbox with `sandbox.create()` and mount there, or list this one's Workspaces with `sandbox.list_workspaces(\"{sandbox}\")` and unmount the ones you are done with (`workspace.unmount(ws)`)"
            ),
        ));
    }
    let id = WorkspaceId::from_authority(random()?).map_err(failed)?;
    // The Branch is found by name in this process; the daemon binds its id.
    let branch_id = crate::host::direct::branch_id(args, branch)?;
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
            SdkValue::text(crate::host::runtime::image(&state.runtime)),
        ));
        // what its record names (#529); a record from before has no platform
        let saved = crate::host::discovery::load(&record)?;
        fields.push(("platform".to_owned(), platform_of(&saved)));
        fields.push((
            "runtime".to_owned(),
            SdkValue::text(crate::host::runtime::runtime_name(&saved)),
        ));
    }
    crate::host::discovery::save_workspace(&state, &mounted, branch)?;
    lock(&state.mounts)?.insert(
        mounted.ready.directory.clone(),
        Mount {
            workspace: mounted,
            branch: branch.to_owned(),
        },
    );
    Ok(value)
}

/// What one record's Sandbox was found to be: running, ended (`container`,
/// and whether it stopped rather than went), or the record refused when read
/// (#529 finding 6), which leaves it out of listing and reclaim.
enum Found {
    Running,
    Ended(String, bool),
    Refused(SdkRefusal),
}

/// Inspects one record's container (or local daemon process) once; no
/// daemon is dialed. A record that cannot be read for this call (its
/// directory or owner marker, for example) is `Found::Refused`; a failure
/// to inspect what it names fails the call.
fn observed(
    args: &SdkValue,
    id: &str,
    record: &SdkValue,
    directory: &Path,
) -> Result<Found, SdkRefusal> {
    let cached = lock(&crate::host::registry::SANDBOXES)?
        .get(directory)
        .cloned();
    let observed = match cached {
        Some(state) => state.runtime.endpoint().map(drop),
        // a container, or with the local runtime a daemon process
        None => match crate::host::discovery::recorded(args, id, record) {
            Ok(recorded) => recorded.reopen().map(drop),
            Err(refusal) => return Ok(Found::Refused(refusal)),
        },
    };
    match observed {
        Ok(()) => Ok(Found::Running),
        Err(failure) if failure.container_gone() || failure.container_stopped() => {
            Ok(Found::Ended(
                text(record, "container")?.to_owned(),
                failure.container_stopped(),
            ))
        }
        Err(failure) => Err(failed(failure)),
    }
}

/// One record of the project: its number, directory and contents, or the
/// refusal reading the file gave (a damaged record).
type Entry = (String, std::path::PathBuf, Result<SdkValue, SdkRefusal>);

/// The project's recorded Sandboxes that a call may reopen, lowest number
/// first, each found once, and the records left out: `{ sandbox, code,
/// message }` for each one refused when read (#529 finding 6: damaged, or
/// failing its directory checks), which is kept in place and fails nothing.
/// Every Docker, native and namespaces record counts, whatever backend the
/// caller's deployment selects (#529: the record decides). A record of the
/// former `local` runtime is nobody's and is left out silently;
/// `runtime.sandbox.records` lists it. The deployment's Sandbox and
/// `localDir` settings are still checked, so a malformed one is refused here
/// as everywhere.
type Readable = Vec<(String, std::path::PathBuf, SdkValue, Found)>;
fn records(args: &SdkValue) -> Result<(Readable, Vec<SdkValue>), SdkRefusal> {
    if let Some(deploy) = args.get("deploy") {
        crate::host::runtime::backend(deploy)?;
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    crate::host::local::local_dir_setting(args)?;
    let (mut readable, mut skipped) = (Vec::new(), Vec::new());
    for (id, directory, value) in recorded_all(args)? {
        let record = match value {
            Ok(record) => record,
            Err(refusal) => {
                skipped.push(refused_entry(&id, &refusal));
                continue;
            }
        };
        if crate::host::runtime::recorded(&record).is_err() {
            continue;
        }
        match observed(args, &id, &record, &directory)? {
            Found::Refused(refusal) => skipped.push(refused_entry(&id, &refusal)),
            found => readable.push((id, directory, record, found)),
        }
    }
    Ok((readable, skipped))
}

/// A record left out, as listing, reclaim and create report it.
fn refused_entry(id: &str, refusal: &SdkRefusal) -> SdkValue {
    SdkValue::map([
        ("sandbox", SdkValue::text(id)),
        ("code", SdkValue::text(&refusal.code)),
        ("message", SdkValue::text(&refusal.message)),
    ])
}

/// Every recorded Sandbox of the project, lowest number first, with each record.
fn recorded_all(args: &SdkValue) -> Result<Vec<Entry>, SdkRefusal> {
    let mut found = Vec::new();
    for id in crate::host::discovery::sandbox_ids(args)? {
        let directory = crate::host::discovery::sandbox_dir(args, &id)?;
        let record = directory.join("sandbox.record");
        // a number claimed by a start that has not finished is not a Sandbox yet
        if record.exists() {
            let value = crate::host::discovery::load(&record);
            found.push((id, directory, value));
        }
    }
    Ok(found)
}

/// Reclaims every recorded Sandbox of the project found gone or stopped (an
/// idle-closed one), so its number is free again, and answers the records
/// it left out. `sandbox.create` calls it under the project's guard before
/// it starts one.
pub(crate) fn reclaim_gone(args: &SdkValue) -> Result<Vec<SdkValue>, SdkRefusal> {
    let (readable, skipped) = records(args)?;
    for (id, _, _, found) in readable {
        if let Found::Ended(container, stopped) = found {
            crate::host::sandbox::reclaim(args, &id, &container, stopped)?;
        }
    }
    Ok(skipped)
}

/// A record's `platform` (#529), or null for a record from before it, which
/// the agent reads as the platform its runtime and home imply.
fn platform_of(record: &SdkValue) -> SdkValue {
    SdkValue::opt_text(record.get_text("platform").map(str::to_owned))
}

/// `project.list_sandboxes()`: the project's running Sandboxes of every
/// platform, lowest number first: `{ sandbox, name, image, platform,
/// runtime, workspaces }`, where `workspaces` counts its Workspace records,
/// and `skipped`, the records refused when read and left out (#529 finding
/// 6). It reads records and inspects each container or daemon once; it
/// starts nothing and changes nothing. A Sandbox found gone or stopped (an
/// idle-closed one) is left out and its records kept: `sandbox.create` and
/// `runtime.sandbox.reclaim` remove them.
pub(crate) fn sandboxes(args: &SdkValue) -> Outcome {
    let mut items = Vec::new();
    let (readable, skipped) = records(args)?;
    for (id, directory, value, found) in readable {
        if !matches!(found, Found::Running) {
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
            ("platform", platform_of(&value)),
            (
                "runtime",
                SdkValue::text(crate::host::runtime::runtime_name(&value)),
            ),
            ("workspaces", SdkValue::Int(recorded(&directory)? as i64)),
        ]));
    }
    Ok(SdkValue::map([
        ("kind", SdkValue::text("ok")),
        ("items", SdkValue::List(items)),
        ("skipped", SdkValue::List(skipped)),
    ]))
}

/// `runtime.sandbox.records()`: every recorded Sandbox of the project, of
/// every backend, lowest number first: `{ sandbox, name, runtime, platform,
/// image, state, workspaces }` (`platform` null in a record from before
/// #529). `runtime` is `docker`, `native`, `namespaces` (or `local`, the
/// former runtime's; null in a damaged record); `state` is `running`,
/// `stopped`, `gone` (a Docker container no longer there), `refused` (the
/// record cannot be read for this call, #529 finding 6: damaged, or failing
/// its directory checks; with `refusal: { code, message }`) or `unknown`
/// (with `message`). The deployment's backend is not consulted (#527 step
/// 12): a Docker record's container is inspected once at its own Engine, a
/// native or namespaces record's process is read from this kernel by id,
/// start time and boot, and a `local` record is never read. Nothing is
/// reopened, dialed, started or changed.
pub(crate) fn all_records(args: &SdkValue) -> Outcome {
    let mut items = Vec::new();
    for (id, directory, value) in recorded_all(args)? {
        let value = match value {
            Ok(value) => value,
            Err(refusal) => {
                let item = record_item(
                    &id,
                    &directory,
                    &SdkValue::Null,
                    None,
                    State::Refused(refusal),
                );
                items.push(item?);
                continue;
            }
        };
        let unknown = |refusal: SdkRefusal| State::Unknown(refusal.message);
        let (runtime, state) = match crate::host::runtime::recorded(&value) {
            // a record of the former local runtime is never read
            Err(other) => (
                Some(other),
                State::Unknown("a record of the former local runtime is never used".into()),
            ),
            Ok(kind) => match crate::host::discovery::recorded(args, &id, &value) {
                // a host that cannot run it (Unsupported) is no fault of the
                // record: its process is read as below
                Err(refusal) if refusal.code != "Unsupported" => (
                    value.get_text("runtime").or(Some("docker")),
                    State::Refused(refusal),
                ),
                _ if kind.is_none() => (
                    Some("docker"),
                    match observed(args, &id, &value, &directory) {
                        Ok(Found::Running) => State::Is("running"),
                        Ok(Found::Ended(_, true)) => State::Is("stopped"),
                        Ok(Found::Ended(_, false)) => State::Is("gone"),
                        Ok(Found::Refused(refusal)) => State::Refused(refusal),
                        Err(refusal) => unknown(refusal),
                    },
                ),
                _ => (
                    value.get_text("runtime"),
                    match crate::host::discovery::local_running(&value) {
                        Ok(true) => State::Is("running"),
                        Ok(false) => State::Is("stopped"),
                        Err(refusal) => unknown(refusal),
                    },
                ),
            },
        };
        items.push(record_item(&id, &directory, &value, runtime, state)?);
    }
    Ok(SdkValue::map([
        ("kind", SdkValue::text("ok")),
        ("items", SdkValue::List(items)),
    ]))
}

/// A `runtime.sandbox.records` item's state: what its Sandbox was found to
/// be, its record refused when read, or not known (with why).
enum State {
    Is(&'static str),
    Refused(SdkRefusal),
    Unknown(String),
}

/// One `runtime.sandbox.records` item.
fn record_item(
    id: &str,
    directory: &Path,
    value: &SdkValue,
    runtime: Option<&str>,
    state: State,
) -> Result<SdkValue, SdkRefusal> {
    let mut fields = vec![
        ("sandbox", SdkValue::text(id)),
        (
            "name",
            SdkValue::text(value.get_text("name").unwrap_or_default()),
        ),
        ("runtime", SdkValue::opt_text(runtime.map(str::to_owned))),
        ("platform", platform_of(value)),
        (
            "image",
            SdkValue::text(value.get_text("image").unwrap_or_default()),
        ),
        ("workspaces", SdkValue::Int(recorded(directory)? as i64)),
    ];
    match state {
        State::Is(state) => fields.push(("state", SdkValue::text(state))),
        State::Refused(refusal) => fields.extend([
            ("state", SdkValue::text("refused")),
            (
                "refusal",
                SdkValue::map([
                    ("code", SdkValue::text(&refusal.code)),
                    ("message", SdkValue::text(&refusal.message)),
                ]),
            ),
        ]),
        State::Unknown(message) => fields.extend([
            ("state", SdkValue::text("unknown")),
            ("message", SdkValue::text(message)),
        ]),
    }
    Ok(SdkValue::Map(
        (fields.into_iter())
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    ))
}

/// `runtime.sandbox.reclaim()`: removes the records of the project's
/// Sandboxes whose containers are gone or stopped (and a stopped one's
/// container), each under the project's guard, so their numbers are free
/// again: `{ kind, reclaimed, skipped }`. A record refused when read is
/// left in place and named in `skipped` (#529 finding 6); one started again
/// under its number meanwhile is left alone. Each step is attempted once.
pub(crate) fn reclaim(args: &SdkValue) -> Outcome {
    let mut reclaimed = Vec::new();
    let (readable, skipped) = records(args)?;
    for (id, _, _, found) in readable {
        let Found::Ended(container, stopped) = found else {
            continue;
        };
        let _placing = crate::host::discovery::guard(&crate::host::discovery::project_dir(args)?)?;
        match crate::host::sandbox::reclaim(args, &id, &container, stopped) {
            Ok(_) => reclaimed.push(SdkValue::text(&id)),
            // started again under the number meanwhile, or already reclaimed
            Err(refusal) if matches!(refusal.code.as_str(), "Busy" | "NoSuchSandbox") => {}
            Err(refusal) => return Err(refusal),
        }
    }
    Ok(SdkValue::map([
        ("kind", SdkValue::text("ok")),
        ("reclaimed", SdkValue::List(reclaimed)),
        ("skipped", SdkValue::List(skipped)),
    ]))
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
    let head = crate::host::direct::head(args, status.binding.branch.id).ok();
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

/// `workspace.unmount(ws, { discard?, force? })`: `Busy` while a background
/// process this binding started still runs there, then the daemon's answer.
pub(crate) fn unmount(args: &SdkValue) -> Outcome {
    let state = sandbox(args)?;
    let path = text(args, "path")?;
    if let Some(value) = crate::host::unmount::find(&state, path)? {
        return crate::host::unmount::repeat(&value);
    }
    let guard = crate::host::discovery::guard(&state.directory)?;
    // Another process may have registered while this call waited for the
    // short admission section. Check again before sending anything.
    if let Some(value) = crate::host::unmount::find(&state, path)? {
        return crate::host::unmount::repeat(&value);
    }
    let token = state.token(path)?;
    crate::host::discovery::refresh(&state)?;
    // The runtime owner knows its own processes; the daemon's kernel probe
    // still answers Busy for any other holder.
    let running = crate::host::exec::running_in(&state, path)?;
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
    let attempt = crate::host::unmount::begin(&state, token, path)?;
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
        crate::host::unmount::cancel(&attempt)
    } else {
        crate::host::unmount::finish(&attempt, path, &outcome)
    };
    if let Err(error) = publication {
        return crate::host::unmount::cleanup_failure(outcome, error);
    }
    if outcome.is_ok() {
        let cleanup = (|| {
            let _guard = crate::host::discovery::guard(&state.directory)?;
            match std::fs::remove_file(crate::host::discovery::workspace_file(&state, token)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(failed(error)),
            }
            lock(&state.mounts)?.remove(path);
            Ok(())
        })();
        if let Err(error) = cleanup {
            return crate::host::unmount::cleanup_failure(outcome, error);
        }
    }
    outcome
}
