//! The `sandbox` and `workspace` families (#533 step 12): Sandboxes named
//! across the home (unique under `layerfs.lock`), recorded in
//! `machine/<machine>/sandbox/<name>/record.json`, each running one project's
//! daemon on the machine that holds its Store. A Workspace is
//! `<sandbox>:<path>`. A Sandbox on `native` runs in this process's host
//! Sandbox code: host processes over loopback NFS on macOS, namespaces on
//! Linux. On any other machine the same call runs in that machine's worker,
//! which keeps the Sandbox's records under its own root; this home keeps
//! the name, the project and the machine. A Sandbox on `docker` is a
//! container whose image comes from the docker machine's record and travels
//! with the call (H §7.6, §7.7); a sqlite Store's is started by the docker
//! machine's worker on every host (G8). Where a call runs is
//! `place::plan`'s answer; the deploy settings are chosen here and resolved
//! where it runs (`deploy`). A PostgreSQL Store's Sandboxes run on `native`
//! or `docker` from this process, which resolves the password (O3), until
//! the worker takes the resolved URL (#535 phase 5a).
use super::deploy::{self, Deploy, Side};
use crate::common::context::Context;
use crate::common::home::{self, paths, read_record, write_record};
use crate::common::outcome::{failed, refused, Outcome};
use crate::common::place::{self, Executor, Start};
use crate::machine::worker::Unreached;
use crate::store::record::{StoreKind, StoreRef};
use crate::store::resolve;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Every Sandbox record in the home: `(machine, name, record)`.
fn all(home: &Path) -> Result<Vec<(String, String, SdkValue)>> {
    let mut found = Vec::new();
    for machine in paths::names(&home.join("machine")).map_err(failed)? {
        let dir = paths::machine_dir(home, &machine).join("sandbox");
        for name in paths::names(&dir).map_err(failed)? {
            if let Some(record) = read_record(&dir.join(&name).join("record.json"))? {
                found.push((machine.clone(), name, record));
            }
        }
    }
    Ok(found)
}

/// The Sandbox named `name`: its machine and record, or `NoSuchSandbox`.
fn sandbox_of(home: &Path, name: &str) -> Result<(String, SdkValue)> {
    crate::common::names::name("sandbox", name).map_err(|why| refused("InvalidInput", why))?;
    all(home)?
        .into_iter()
        .find(|(_, n, _)| n == name)
        .map(|(machine, _, record)| (machine, record))
        .ok_or_else(|| {
            refused(
                "NoSuchSandbox",
                format!(
                    "no Sandbox named {name}; `sandbox.create(project, name, machine)` starts one"
                ),
            )
        })
}

/// The host Sandbox operations' arguments for project `<store>/<project>`,
/// plus `more`, run as `how` says.
fn host_args(
    home: &Path,
    (store, project, record): (&str, &str, &SdkValue),
    kind: StoreKind,
    how: Deploy,
    more: Vec<(&str, SdkValue)>,
) -> Result<SdkValue> {
    let prefix = SdkValue::text(format!("lfs-{}-", &home::open(home)?.id[..8]));
    let in_container = matches!(how, Deploy::Docker(_));
    let deploy = match how {
        Deploy::Docker(SdkValue::Map(mut settings)) => {
            settings.push(("sandbox".to_owned(), SdkValue::text("docker")));
            settings.push(("containerPrefix".to_owned(), prefix));
            SdkValue::Map(settings)
        }
        Deploy::Docker(_) => return Err(refused("InvalidInput", "docker settings are a map")),
        Deploy::Native(settings) => {
            let backend = if cfg!(target_os = "macos") {
                "native"
            } else {
                "namespaces"
            };
            // the local runtime's root: this user's, traversable (0755), as
            // the runtime requires
            let local = paths::machine_dir(home, "native").join("sandbox-root");
            {
                use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
                std::fs::DirBuilder::new()
                    .recursive(true)
                    .mode(0o755)
                    .create(&local)
                    .map_err(failed)?;
                std::fs::set_permissions(&local, std::fs::Permissions::from_mode(0o755))
                    .map_err(failed)?;
            }
            // namespaces commands run as another identity: the home's folders
            // above the root must let them pass (a macOS native command does not)
            if backend == "namespaces" {
                home::traversable(home, &local)?;
            }
            let mut fields = vec![
                ("sandbox".to_owned(), SdkValue::text(backend)),
                (
                    "localDir".to_owned(),
                    SdkValue::text(local.display().to_string()),
                ),
                ("containerPrefix".to_owned(), prefix),
            ];
            fields.extend(settings);
            SdkValue::Map(fields)
        }
    };
    let mut fields = vec![
        ("home", SdkValue::text(home.display().to_string())),
        ("store", SdkValue::text(store)),
        ("project", SdkValue::text(project)),
        ("store_record", record.clone()),
        (
            "storePath",
            SdkValue::text(store_path(home, store, kind).display().to_string()),
        ),
        ("deploy", deploy),
    ];
    // a PostgreSQL Store's URL with its password, resolved now and never
    // written (H §7.4)
    if kind == StoreKind::Postgresql {
        let url = crate::store::connect::url(record, store)?;
        let mut postgresql = vec![("url", SdkValue::text(&url))];
        if in_container && cfg!(not(target_os = "linux")) {
            if let Some(daemon_url) = layerfs_sandbox::docker::desktop_url(&url) {
                postgresql.push(("daemon_url", SdkValue::text(daemon_url)));
            }
        }
        fields.push((
            "postgresql",
            SdkValue::Map(
                postgresql
                    .into_iter()
                    .map(|(n, v)| (n.to_owned(), v))
                    .collect(),
            ),
        ));
    }
    fields.extend(more);
    Ok(SdkValue::Map(
        fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect(),
    ))
}

/// The path the open-Store cache keys `store` by: its file, or for a
/// PostgreSQL Store the path [`route`] keys it by, which no file has.
fn store_path(home: &Path, store: &str, kind: StoreKind) -> PathBuf {
    match kind {
        StoreKind::Postgresql => paths::store_dir(home, store).join("postgresql"),
        StoreKind::Sqlite => paths::store_file(home, store),
    }
}

/// Runs one `sandbox` call.
pub fn sandbox(context: &mut Context<'_>, call: &str, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    match call {
        "create" => create(context, args),
        "list" => {
            let only = args.get_text("project");
            let items: Vec<SdkValue> = all(&home)?
                .into_iter()
                .filter(|(_, _, r)| only.is_none() || r.get_text("project") == only)
                .map(|(_, _, record)| record)
                .collect();
            let (items, next) = page(items, args);
            Ok(SdkValue::map([
                ("items", SdkValue::List(items)),
                ("next_page_pos", next),
            ]))
        }
        "get" => {
            let name = args.get_text("name").unwrap_or("");
            let (machine, record) = sandbox_of(&home, name)?;
            // a get starts no machine (P4): a machine that is down answers
            // the record with no Workspaces and why
            let listed = reach(
                context,
                &machine,
                &record,
                "sandbox.list_workspaces",
                vec![],
                Start::Never,
            );
            let (all, down) = match listed {
                Ok(workspaces) => match workspaces?.get("workspaces") {
                    Some(SdkValue::List(items)) => (items.clone(), None),
                    _ => (Vec::new(), None),
                },
                Err(Unreached(why)) => (Vec::new(), Some(why.message)),
            };
            let (items, next) = page(all, args);
            let mut record = home::with_field(record, "workspaces", SdkValue::List(items));
            if let Some(why) = down {
                record = home::with_field(record, "down_reason", SdkValue::Text(why));
            }
            Ok(home::with_field(record, "next_page_pos", next))
        }
        "close" => {
            let name = args.get_text("name").unwrap_or("");
            let (machine, record) = sandbox_of(&home, name)?;
            if record.get_text("id").is_none() {
                if let Some(closed) = close_unstarted(&home, &machine, name)? {
                    return Ok(closed);
                }
            }
            let (machine, record) = sandbox_of(&home, name)?;
            let force = matches!(args.get("force"), Some(SdkValue::Bool(true)));
            let closed = on_machine(
                context,
                &machine,
                &record,
                "sandbox.close",
                vec![("force", SdkValue::Bool(force))],
            );
            match closed {
                Ok(value) => {
                    let _lock = home::lock::take(&home)?;
                    std::fs::remove_dir_all(paths::sandbox_dir(&home, &machine, name))
                        .map_err(failed)?;
                    Ok(home::with_field(value, "closed", SdkValue::text(name)))
                }
                // a Sandbox already gone loses its record too
                Err(refusal) if refusal.code == "NoSuchSandbox" => {
                    let _lock = home::lock::take(&home)?;
                    std::fs::remove_dir_all(paths::sandbox_dir(&home, &machine, name))
                        .map_err(failed)?;
                    Ok(SdkValue::map([
                        ("closed", SdkValue::text(name)),
                        ("was_gone", SdkValue::Bool(true)),
                    ]))
                }
                Err(refusal) => Err(refusal),
            }
        }
        _ => Err(refused("InvalidInput", format!("no call sandbox.{call}"))),
    }
}

/// `sandbox.create(project, name, machine, { image })`.
fn create(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    let project = args.get_text("project").unwrap_or("");
    let name = args.get_text("name").unwrap_or("");
    let machine = args.get_text("machine").unwrap_or("");
    crate::common::names::name("sandbox", name).map_err(|why| refused("InvalidInput", why))?;
    if matches!(name, "native" | "docker") {
        return Err(refused(
            "InvalidInput",
            format!("a Sandbox is not named {name}: that names a built-in machine (H §7.6)"),
        ));
    }
    // the Store's place, a move and the name are checked under the lock
    // that claims the name, so a store.move cannot pass between them
    let _lock = home::lock::take(&home)?;
    let target = resolve::target(&home, project)?;
    resolve::moving(&target.record, &target.store)?;
    // refused too while the machine is being removed
    #[cfg(feature = "machine")]
    {
        // where a Sandbox of this Store's kind may run is the support
        // table's answer, word for word (#535 §7.3)
        use layerfs_machine::{sandbox_allowed, HostOs};
        let platform = crate::machine::platform_of(&crate::machine::machine_of(&home, machine)?)?;
        sandbox_allowed(platform, HostOs::current(), target.store_ref.kind()).map_err(
            |reason| {
                refused(
                    "Unsupported",
                    format!(
                        "a Sandbox of {} Store {} cannot run on machine {machine} ({}): {reason}",
                        target.store_ref.kind().as_str(),
                        target.store,
                        platform.name()
                    ),
                )
            },
        )?;
    }
    #[cfg(not(feature = "machine"))]
    if read_record(&paths::machine_record(&home, machine))?.is_none() {
        return Err(refused("NotFound", format!("no machine named {machine}")));
    }
    if let StoreRef::Postgres { .. } = target.store_ref {
        // a PostgreSQL Store is on no machine; its daemon dials the server.
        // A build without machines (the guest worker's) knows only these two.
        #[cfg(not(feature = "machine"))]
        if !matches!(machine, "native" | "docker") {
            return Err(refused(
                "Unsupported",
                format!("a Sandbox of PostgreSQL Store {} runs on native or docker; on VMs and ssh hosts it comes later (O3)", target.store),
            ));
        }
    } else if target.store_ref.machine()? != machine {
        let store_machine = target.store_ref.machine()?;
        return Err(refused(
            "Unsupported",
            format!("Store {} is on machine {store_machine}; a Sandbox of {project} runs there (`store move` moves the Store)", target.store),
        ));
    }
    // claim the name across the home; the creator's pid lets a close tell
    // a create in flight from one whose process is gone
    if all(&home)?.iter().any(|(_, n, _)| n == name) {
        return Err(refused("Exists", format!("a Sandbox named {name} exists")));
    }
    write_record(
        &paths::sandbox_dir(&home, machine, name).join("record.json"),
        &SdkValue::map([
            ("name", SdkValue::text(name)),
            ("project", SdkValue::text(project)),
            ("machine", SdkValue::text(machine)),
            ("state", SdkValue::text("creating")),
            ("created", SdkValue::Int(home::now_ms())),
            ("pid", SdkValue::Int(i64::from(std::process::id()))),
        ]),
    )?;
    drop(_lock);
    let mut more = vec![];
    if let Some(image) = args.get_text("image") {
        more.push(("image", SdkValue::text(image)));
    }
    let record = read_record(&paths::sandbox_dir(&home, machine, name).join("record.json"))?
        .unwrap_or_default();
    let started = on_machine(context, machine, &record, "sandbox.create", more);
    let _lock = home::lock::take(&home)?;
    let path = paths::sandbox_dir(&home, machine, name).join("record.json");
    match started {
        Ok(value) => {
            let mut record = record;
            for (field, value) in [
                ("id", value.get("sandbox").cloned().unwrap_or_default()),
                (
                    "platform",
                    value.get("platform").cloned().unwrap_or_default(),
                ),
                ("runtime", value.get("runtime").cloned().unwrap_or_default()),
                ("image", value.get("image").cloned().unwrap_or_default()),
                ("state", SdkValue::text("running")),
            ] {
                record = home::with_field(record, field, value);
            }
            write_record(&path, &record)?;
            Ok(record)
        }
        Err(refusal) if refusal.code == "Unknown" => {
            write_record(
                &path,
                &home::with_field(record, "state", SdkValue::text("unknown")),
            )?;
            Err(refusal)
        }
        Err(refusal) => {
            let _ = std::fs::remove_dir_all(paths::sandbox_dir(&home, machine, name));
            Err(refusal)
        }
    }
}

/// Runs host Sandbox operation `op` for Sandbox `record` on `machine`,
/// starting the machine when it is down.
fn on_machine(
    context: &mut Context<'_>,
    machine: &str,
    record: &SdkValue,
    op: &str,
    more: Vec<(&str, SdkValue)>,
) -> Outcome {
    reach(context, machine, record, op, more, Start::OnDemand)
        .unwrap_or_else(|Unreached(refusal)| Err(refusal))
}

/// [`on_machine`], with `start` saying whether a machine that is down is
/// started; [`Unreached`] when it was not and nothing was sent.
fn reach(
    context: &mut Context<'_>,
    machine: &str,
    record: &SdkValue,
    op: &str,
    more: Vec<(&str, SdkValue)>,
    start: Start,
) -> std::result::Result<Outcome, Unreached> {
    let mut unreached = None;
    let outcome = host_op(context, (machine, record, op), more, start, &mut unreached);
    match unreached {
        Some(unreached) => Err(unreached),
        None => Ok(outcome),
    }
}

/// [`reach`]'s operation; a request that was not sent leaves its
/// [`Unreached`] in `unreached`.
fn host_op(
    context: &mut Context<'_>,
    (machine, record, op): (&str, &SdkValue, &str),
    more: Vec<(&str, SdkValue)>,
    start: Start,
    unreached: &mut Option<Unreached>,
) -> Outcome {
    let home = context.home.clone();
    let project = record.get_text("project").unwrap_or("");
    let target = resolve::target(&home, project)?;
    let mut more = more;
    if let Some(id) = record.get_text("id") {
        more.push(("sandbox", SdkValue::text(id)));
    }
    // the call's image (sandbox.create) else the Sandbox's own travels in
    // the deploy settings, never as a top-level argument
    let called = more
        .iter()
        .position(|(n, _)| *n == "image")
        .map(|i| more.remove(i).1);
    let image = match called {
        Some(SdkValue::Text(text)) => Some(text),
        _ => record
            .get_text("image")
            .filter(|_| op != "sandbox.create")
            .map(str::to_owned),
    };
    let postgresql = target.store_ref.kind() == StoreKind::Postgresql;
    // a PostgreSQL Store's Sandboxes run from this process, which resolves
    // the password (O3)
    let executor = match postgresql {
        true => Executor::Local,
        false => place::on_machine(&home, machine, start)?,
    };
    // what the machine's record chooses; the side that runs the call
    // resolves the rest
    let choice = deploy::choose(&home, machine, image.as_deref(), postgresql, start)?;
    match executor {
        Executor::Local => {
            let how = deploy::resolve(&Side::Home(&home), &target.store, &choice)?;
            let args = host_args(
                &home,
                (&target.store, &target.project, &target.record),
                target.store_ref.kind(),
                how,
                more,
            )?;
            // the host code reads the project's Branches through its open Store
            let lease = match postgresql {
                true => None,
                false => Some(crate::common::home::lease::hold(&home, &target.store)?),
            };
            crate::store::open::hold(&args, lease)?;
            if op == "backend.daemon.workspace.status" {
                return crate::sandbox::dispatch::backend(op.to_owned(), args);
            }
            let out = crate::sandbox::dispatch::call(op, args)?;
            drain(out, &mut |event| context.event(event))
        }
        Executor::Worker(mut worker) => {
            // the guest runs the same call on its own native machine, with
            // the records it needs sent along
            let project_record = read_record(&paths::project_record(
                &home,
                &target.store,
                &target.project,
            ))?
            .unwrap_or_default();
            let mut fields = vec![
                ("op", SdkValue::text(op)),
                ("store", SdkValue::text(&target.store)),
                ("project", SdkValue::text(&target.project)),
                ("store_record", target.record.clone()),
                ("project_record", project_record),
            ];
            fields.push(("deploy", choice));
            fields.push((
                "more",
                SdkValue::Map(more.into_iter().map(|(n, v)| (n.to_owned(), v)).collect()),
            ));
            let request =
                SdkValue::Map(fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect());
            match worker.attempt("sandbox.host", request, &mut |event| context.event(event)) {
                Ok(outcome) => outcome,
                Err(Unreached(refusal)) => {
                    *unreached = Some(Unreached(refusal.clone()));
                    Err(refusal)
                }
            }
        }
    }
}

/// Runs a Sandbox host operation in a worker for its caller (`sandbox.host`):
/// the Store's and project's records are written under the root first,
/// with the Store on this machine's own `native`.
pub(crate) fn in_worker(root: &Path, args: &SdkValue, events: &mut dyn FnMut(SdkValue)) -> Outcome {
    let op = args
        .get_text("op")
        .ok_or_else(|| refused("InvalidInput", "operation name"))?;
    if !matches!(
        op,
        "sandbox.create"
            | "sandbox.close"
            | "sandbox.list_workspaces"
            | "workspace.mount"
            | "workspace.exec"
            | "workspace.commit"
            | "workspace.unmount"
            | "backend.daemon.workspace.status"
    ) {
        return Err(refused(
            "InvalidInput",
            format!("the worker runs no Sandbox operation {op}"),
        ));
    }
    let store = args.get_text("store").unwrap_or("");
    let project = args.get_text("project").unwrap_or("");
    crate::common::names::name("store", store).map_err(|why| refused("InvalidInput", why))?;
    crate::common::names::name("project", project).map_err(|why| refused("InvalidInput", why))?;
    let record = home::with_field(
        args.get("store_record").cloned().unwrap_or_default(),
        "machine",
        SdkValue::text("native"),
    );
    // written only when it changed (#535 R5): most calls send the same one
    let path = paths::store_record(root, store);
    if read_record(&path)?.as_ref() != Some(&record) {
        write_record(&path, &record)?;
    }
    // the caller's record replaces one left by a project of the same name
    // that was removed and made again (its `created` differs)
    let project_path = paths::project_record(root, store, project);
    let sent = args.get("project_record").cloned().unwrap_or_default();
    let stale = match read_record(&project_path)? {
        None => true,
        Some(local) => sent.get("created").is_some() && local.get("created") != sent.get("created"),
    };
    if stale {
        write_record(&project_path, &sent)?;
    }
    let more: Vec<(&str, SdkValue)> = match args.get("more") {
        Some(SdkValue::Map(fields)) => fields
            .iter()
            .map(|(n, v)| (n.as_str(), v.clone()))
            .collect(),
        _ => Vec::new(),
    };
    let choice = args.get("deploy").cloned().unwrap_or_default();
    let how = deploy::resolve(&Side::Worker(root), store, &choice)?;
    let kind = StoreRef::from_record(store, &record)?.kind();
    let host = host_args(root, (store, project, &record), kind, how, more)?;
    crate::store::open::hold(&host, None)?;
    if op == "backend.daemon.workspace.status" {
        return crate::sandbox::dispatch::backend(op.to_owned(), host);
    }
    drain(crate::sandbox::dispatch::call(op, host)?, events)
}

/// An exec result whose output the host spooled (a stream over 1 MiB):
/// each spooled stream goes to the caller as output events (`{ stream,
/// bytes }`), one page at a time and in order, then the spool is closed.
/// The reply keeps the stream null with its byte count and says
/// `streamed`; the CLI's `--stream` and the front ends' sinks receive it.
/// Any other result passes through.
fn drain(out: SdkValue, emit: &mut dyn FnMut(SdkValue)) -> Outcome {
    let Some(id) = out.get_text("output").map(str::to_owned) else {
        return Ok(out);
    };
    let mut buffer = vec![0u8; crate::sandbox::output::INLINE];
    let sent = (|| {
        for (stream, size) in [("stdout", "stdoutBytes"), ("stderr", "stderrBytes")] {
            if !matches!(out.get(stream), Some(SdkValue::Null)) || out.get_int(size) == Some(0) {
                continue;
            }
            let mut offset = 0i64;
            loop {
                let args = SdkValue::map([
                    ("output", SdkValue::text(&id)),
                    ("stream", SdkValue::text(stream)),
                    ("offset", SdkValue::Int(offset)),
                ]);
                let read = crate::sandbox::output::page(&args, &mut buffer)?;
                let length = read.get_int("length").unwrap_or(0);
                if length > 0 {
                    emit(SdkValue::map([
                        ("stream", SdkValue::text(stream)),
                        ("bytes", SdkValue::Bytes(buffer[..length as usize].to_vec())),
                    ]));
                }
                offset += length;
                if read.get_bool("end") || length == 0 {
                    break;
                }
            }
        }
        Ok::<(), SdkRefusal>(())
    })();
    crate::sandbox::output::release(&SdkValue::map([("output", SdkValue::text(&id))]))?;
    sent?;
    let out = match out {
        SdkValue::Map(fields) => {
            SdkValue::Map(fields.into_iter().filter(|(n, _)| n != "output").collect())
        }
        other => other,
    };
    Ok(home::with_field(out, "streamed", SdkValue::Bool(true)))
}

/// One page of `items` (`page_size`, default 50, 1 to 1000; `page_pos`)
/// and the next page's position, or null after the last.
fn page(items: Vec<SdkValue>, args: &SdkValue) -> (Vec<SdkValue>, SdkValue) {
    let size = args.get_int("page_size").unwrap_or(50).clamp(1, 1000) as usize;
    let pos = args.get_int("page_pos").unwrap_or(0).max(0) as usize;
    let total = items.len();
    let page: Vec<SdkValue> = items.into_iter().skip(pos).take(size).collect();
    let next = match pos + page.len() < total {
        true => SdkValue::Int((pos + page.len()) as i64),
        false => SdkValue::Null,
    };
    (page, next)
}

/// Closes Sandbox `name` whose record has no id: its create is in flight,
/// its outcome was unknown, or its process ended before recording one.
/// Refused `InUse` while the creating process lives; otherwise the record
/// goes and the name is free. `None` when the record gained an id
/// meanwhile, for the ordinary close.
fn close_unstarted(home: &Path, machine: &str, name: &str) -> Result<Option<SdkValue>> {
    let _lock = home::lock::take(home)?;
    let dir = paths::sandbox_dir(home, machine, name);
    let Some(record) = read_record(&dir.join("record.json"))? else {
        return Err(refused("NoSuchSandbox", format!("no Sandbox named {name}")));
    };
    if record.get_text("id").is_some() {
        return Ok(None);
    }
    let state = record.get_text("state").unwrap_or("creating").to_owned();
    let pid = record.get_int("pid").unwrap_or(0);
    if state == "creating" && pid > 0 && crate::common::home::lease::alive(pid as u32) {
        return Err(refused(
            "InUse",
            format!("Sandbox {name} is being created by process {pid}"),
        ));
    }
    std::fs::remove_dir_all(&dir).map_err(failed)?;
    let mut fields = vec![
        ("closed", SdkValue::text(name)),
        ("was", SdkValue::text(&state)),
    ];
    if state == "unknown" {
        fields.push((
            "left",
            SdkValue::text("its create's outcome was unknown: a Sandbox it may have started on its machine is not recorded here"),
        ));
    }
    Ok(Some(SdkValue::Map(
        fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect(),
    )))
}

/// `(sandbox, path)` of a Workspace written `<sandbox>:<path>`.
fn ws(text: &str) -> Result<(&str, &str)> {
    text.split_once(':')
        .filter(|(s, p)| !s.is_empty() && p.starts_with('/'))
        .ok_or_else(|| {
            refused(
                "InvalidInput",
                format!("a Workspace is <sandbox>:<path>, not {text:?}"),
            )
        })
}

/// Runs one `workspace` call.
pub fn workspace(context: &mut Context<'_>, call: &str, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    if call == "mount" {
        let name = args.get_text("sandbox").unwrap_or("");
        let (machine, record) = sandbox_of(&home, name)?;
        let branch = args.get("branch").cloned().unwrap_or_default();
        let mounted = on_machine(
            context,
            &machine,
            &record,
            "workspace.mount",
            vec![("branch", branch)],
        )?;
        let path = mounted.get_text("path").unwrap_or("").to_owned();
        return Ok(home::with_field(
            mounted,
            "ws",
            SdkValue::text(format!("{name}:{path}")),
        ));
    }
    let (name, path) = ws(args.get_text("ws").unwrap_or(""))?;
    let (machine, record) = sandbox_of(&home, name)?;
    let mut more = vec![("path", SdkValue::text(path))];
    let op = match call {
        "exec" => {
            more.push(("command", args.get("command").cloned().unwrap_or_default()));
            more.push(("message", args.get("message").cloned().unwrap_or_default()));
            "workspace.exec"
        }
        "commit" => {
            more.push(("message", args.get("message").cloned().unwrap_or_default()));
            // publish over a moved head instead of merging
            if let Some(force) = args.get("force") {
                more.push(("force", force.clone()));
            }
            "workspace.commit"
        }
        "unmount" => {
            for flag in ["discard", "force"] {
                if let Some(value) = args.get(flag) {
                    more.push((flag, value.clone()));
                }
            }
            "workspace.unmount"
        }
        "status" => "backend.daemon.workspace.status",
        _ => return Err(refused("InvalidInput", format!("no call workspace.{call}"))),
    };
    // a status starts no machine (P4)
    let out = match call {
        "status" => reach(context, &machine, &record, op, more, Start::Never).unwrap_or_else(
            |Unreached(why)| {
                Err(refused(
                    "Unavailable",
                    format!(
                        "{}; workspace.status starts no machine (`machine start {machine}` starts it)",
                        why.message
                    ),
                ))
            },
        ),
        _ => on_machine(context, &machine, &record, op, more),
    }?;
    if call == "exec" {
        // the command's output as events too, for front ends that stream
        for stream in ["stdout", "stderr"] {
            match out.get(stream) {
                Some(SdkValue::Text(text)) if !text.is_empty() => {
                    context.output(stream, text.as_bytes())
                }
                Some(SdkValue::Bytes(bytes)) if !bytes.is_empty() => context.output(stream, bytes),
                _ => {}
            }
        }
    }
    Ok(out)
}
