//! The route of a Sandbox host operation (#535 §6.4): run in this process, or
//! sent to its machine's worker with the records it needs, and run there
//! (`in_worker`); the host arguments both sides build, and spooled output
//! streamed back as events.
use super::deploy::{self, Deploy, Side};
use super::record::SandboxRecord;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::home::{self, paths};
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_machine::worker::Unreached;
use layerfs_store::place::{self, Executor};
use layerfs_store::project::record::ProjectRecord;
use layerfs_store::record::{StoreKind, StoreRecord};
use layerfs_store::resolve;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The host Sandbox operations' arguments for project `<store>/<project>`,
/// plus `more`, run as `how` says; a PostgreSQL Store's `postgresql` is
/// the one the caller resolved and reached ([`reached`]).
fn host_args(
    home: &Path,
    (store, project, record): (&str, &str, &StoreRecord),
    (kind, postgresql): (StoreKind, Option<SdkValue>),
    how: Deploy,
    more: Vec<(&str, SdkValue)>,
) -> Result<SdkValue> {
    let prefix = SdkValue::text(format!(
        "lfs-{}-",
        &layerfs_home::home::opened(home)?.id[..8]
    ));
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
            // the runtime requires; one directory per Sandbox beside the
            // image cache it shares
            let local = paths::run_root(home);
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
        ("store_record", record.to_record().clone()),
        (
            "storePath",
            SdkValue::text(store_path(home, store, record, kind).display().to_string()),
        ),
        ("deploy", deploy),
    ];
    // a PostgreSQL Store's URL with its password, never written (H §7.4)
    if let Some(postgresql) = postgresql {
        fields.push(("postgresql", postgresql));
    }
    fields.extend(more);
    Ok(SdkValue::Map(
        fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect(),
    ))
}

/// A PostgreSQL Store's `postgresql` argument for a Sandbox on `machine`
/// (#535 §7.4): the record's URL with its password, resolved here in the
/// caller, with a loopback host rewritten to the machine's alias or refused
/// (`layerfs_machine::platform::reach`). It travels only in the one request that runs
/// the call and is never written to a record.
/// With it, what a failed reach check tells a user to check on that
/// machine's platform, if anything.
fn reached(
    home: &Path,
    machine: &str,
    record: &StoreRecord,
) -> Result<(SdkValue, Option<&'static str>)> {
    let url = layerfs_store::record::url(record)?;
    // a build without machines (the guest's) runs its own Sandboxes on the
    // machine it is, which shares its loopback
    #[cfg(feature = "machine")]
    let (url, hint) = {
        let id = layerfs_home::home::opened(home)?.id;
        let spec =
            layerfs_machine::machine_spec(home, &id, &layerfs_machine::machine_of(home, machine)?)?;
        let hint = (spec.kind == layerfs_machine::platform::Platform::AppleContainer).then_some(
            "an apple-container machine reaches a server on this host only when it listens on the container network's bridge (0.0.0.0, as Docker Desktop publishes a port), not on 127.0.0.1 alone",
        );
        let url = layerfs_machine::platform::reach(&spec, &url)
            .map_err(layerfs_machine::machine_refusal)?;
        (url, hint)
    };
    #[cfg(not(feature = "machine"))]
    let hint = {
        let _ = (home, machine);
        None
    };
    Ok((
        layerfs_store::postgresql::connect::selection_at(record, url),
        hint,
    ))
}

/// The reach check of a PostgreSQL Sandbox's create (#535 §7.4 step 4), on
/// the side that launches it: one session to the server it will dial (this
/// process's held host session for the Store at `key`, #65) reads the
/// LayerFS schema's binding, which must be the Store record's, and finds
/// `project` there. It proves the server is reached and is the same Store;
/// a failure names the machine and the host it tried, never the URL, and
/// adds `hint`. A build without the PostgreSQL engine says so, not that it
/// cannot reach. A project the schema does not hold is `NoSuchProject`.
fn reach_check(
    who: &str,
    (store, key, project): (&StoreRecord, &Path, &str),
    postgresql: &SdkValue,
    hint: Option<&str>,
) -> Result<()> {
    let url = postgresql.get_text("url").unwrap_or("");
    let host = layerfs_isolation::PostgresUrl::parse(url)
        .map(|url| url.host().to_owned())
        .unwrap_or_default();
    let reached = layerfs_store::postgresql::reader::reach(key, store, postgresql, project)
        .map_err(|refusal| {
            let name = store.name();
            let message = match (refusal.code.as_str(), hint) {
                ("Unsupported", _) => {
                    return refused(
                        "Unsupported",
                        format!(
                            "{who} cannot check PostgreSQL Store {name}'s server: {}",
                            refusal.message
                        ),
                    )
                }
                (_, Some(hint)) => format!("{}; {hint}", refusal.message),
                (_, None) => refusal.message,
            };
            refused(
                "Unavailable",
                format!("{who} cannot reach PostgreSQL Store {name}'s server at {host}: {message}"),
            )
        })?;
    match reached {
        Ok(true) => Ok(()),
        Ok(false) => Err(refused(
            "NoSuchProject",
            format!(
                "no project {project} in the PostgreSQL Store's schema {}",
                postgresql.get_text("schema").unwrap_or("")
            ),
        )),
        Err(layerfs_store::postgresql::reader::OtherStore) => Err(refused(
            "Unavailable",
            format!(
                "the PostgreSQL server {who} reaches at {host} holds another Store than {}",
                store.name()
            ),
        )),
    }
}

/// The path the open-Store cache keys `store` by: its file, or for a
/// PostgreSQL Store the path [`route`] keys it by, which no file has.
fn store_path(home: &Path, store: &str, record: &StoreRecord, kind: StoreKind) -> PathBuf {
    match kind {
        StoreKind::Postgresql => record.postgresql_key(home, store),
        StoreKind::Sqlite => paths::store_file(home, store),
    }
}

/// How the side that runs a host operation runs it in its own process:
/// `sandbox::dispatch::call` for the Sandbox calls, `workspace::dispatch::call`
/// for the Workspace calls and a worker's requests (layerfs-project#106).
pub type Local = fn(&str, SdkValue) -> Outcome;

/// Runs host Sandbox operation `op` for Sandbox `record` on its machine,
/// refused `Unavailable` naming `layerfs machine start` when it is down.
pub fn on_machine(
    home: &Path,
    events: &mut Events<'_>,
    record: &SandboxRecord,
    op: &str,
    more: Vec<(&str, SdkValue)>,
    local: Local,
) -> Outcome {
    reach(home, events, record, op, more, true, local)
        .unwrap_or_else(|Unreached(refusal)| Err(refusal))
}

/// [`on_machine`], answering [`Unreached`] when the machine's worker was
/// not reached and nothing was sent (nothing starts it, #535 D11);
/// `load_image` says whether the call may load the docker machine's
/// default image (a get or status call does not, P4).
pub fn reach(
    home: &Path,
    events: &mut Events<'_>,
    record: &SandboxRecord,
    op: &str,
    more: Vec<(&str, SdkValue)>,
    load_image: bool,
    local: Local,
) -> std::result::Result<Outcome, Unreached> {
    let mut unreached = None;
    let outcome = host_op(
        home,
        events,
        (record, op),
        more,
        (load_image, local),
        &mut unreached,
    );
    match unreached {
        Some(unreached) => Err(unreached),
        None => Ok(outcome),
    }
}

/// [`reach`]'s operation; a request that was not sent leaves its
/// [`Unreached`] in `unreached`.
fn host_op(
    home: &Path,
    events: &mut Events<'_>,
    (record, op): (&SandboxRecord, &str),
    more: Vec<(&str, SdkValue)>,
    (load_image, local): (bool, Local),
    unreached: &mut Option<Unreached>,
) -> Outcome {
    let home = home.to_path_buf();
    let machine = record.machine();
    let project = record.project().unwrap_or("");
    let target = resolve::target(&home, project)?;
    let mut more = more;
    // the Sandbox's name is its identity on every machine
    more.push(("sandbox", SdkValue::text(record.name())));
    // the call's image (sandbox.create) else the Sandbox's own travels in
    // the deploy settings, never as a top-level argument
    let called = more
        .iter()
        .position(|(n, _)| *n == "image")
        .map(|i| more.remove(i).1);
    let image = match called {
        Some(SdkValue::Text(text)) => Some(text),
        _ => record
            .image()
            .filter(|_| op != "sandbox.create")
            .map(str::to_owned),
    };
    let kind = target.store_ref.kind();
    let (postgresql, hint) = match kind {
        StoreKind::Postgresql => {
            let (value, hint) = reached(&home, machine, &target.record)?;
            (Some(value), hint)
        }
        StoreKind::Sqlite => (None, None),
    };
    // a Sandbox's host operations run where its machine runs them, for
    // either Store kind (#535 §7.4)
    let executor = place::on_machine(&home, machine)?;
    // what the machine's record chooses; the side that runs the call
    // resolves the rest
    let choice = deploy::choose(
        &home,
        machine,
        image.as_deref(),
        postgresql.is_some(),
        load_image,
    )?;
    match executor {
        Executor::Local => {
            let key = store_path(&home, &target.store, &target.record, kind);
            if let (Some(postgresql), "sandbox.create") = (&postgresql, op) {
                reach_check(
                    &format!("machine {machine}"),
                    (&target.record, &key, &target.project),
                    postgresql,
                    hint,
                )?;
            }
            let held = postgresql.is_some();
            let how = deploy::resolve(&Side::Home(&home), &target.store, &choice)?;
            let args = host_args(
                &home,
                (&target.store, &target.project, &target.record),
                (kind, postgresql),
                how,
                more,
            )?;
            // the host code reads the project's Branches through its open
            // Store, which this process leases. A PostgreSQL Store opens no
            // Store service (#65): the lease covers this call, and a read
            // (a create's reach check, a mount's Branch, a status's head)
            // runs on the process's one held read session, opened when one
            // is needed; exec, Commit, unmount and close open nothing.
            let lease = layerfs_store::lease::hold(&home, &target.store)?;
            let (_lease, opened) = match held {
                true => (Some(lease), None),
                false => (None, Some(layerfs_store::open::hold(&args, Some(lease))?)),
            };
            if op == "backend.daemon.workspace.status" {
                return local(op, args);
            }
            let out = local(op, args)?;
            let out = drain(out, &mut |event| events.event(event));
            closed(op, opened.as_deref(), &out);
            out
        }
        Executor::Worker(mut worker) => {
            // the guest runs the same call on its own native machine, with
            // the records it needs sent along
            let project_record = ProjectRecord::read(&home, &target.store, &target.project)?
                .map(|record| record.to_record().clone())
                .unwrap_or_default();
            let mut fields = vec![
                ("op", SdkValue::text(op)),
                ("store", SdkValue::text(&target.store)),
                ("project", SdkValue::text(&target.project)),
                ("store_record", target.record.to_record().clone()),
                ("project_record", project_record),
            ];
            fields.push(("deploy", choice));
            if let Some(postgresql) = postgresql {
                fields.push(("postgresql", postgresql));
                // what the worker's reach check names and suggests
                fields.push(("machine", SdkValue::text(machine)));
                if let Some(hint) = hint {
                    fields.push(("reach_hint", SdkValue::text(hint)));
                }
            }
            fields.push((
                "more",
                SdkValue::Map(more.into_iter().map(|(n, v)| (n.to_owned(), v)).collect()),
            ));
            let request =
                SdkValue::Map(fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect());
            match worker.attempt("sandbox.host", request, &mut |event| events.event(event)) {
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
/// the caller sends the Store's and project's records; the worker keeps a
/// thin record of the Store (#536 §6) and the project's record under its
/// root, with the Store on this machine's own `native`.
pub fn in_worker(
    root: &Path,
    args: &SdkValue,
    events: &mut dyn FnMut(SdkValue),
    runner: Local,
) -> Outcome {
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
    layerfs_home::names::StoreName::parse(store)?;
    layerfs_home::names::ProjectName::parse(project)?;
    // a PostgreSQL Store's URL and password reference stay with the caller:
    // the request's `postgresql` carries what this side dials (#535 §7.4)
    let sent = match args.get("store_record").cloned().unwrap_or_default() {
        SdkValue::Map(fields) => SdkValue::Map(
            fields
                .into_iter()
                .filter(|(n, _)| !matches!(n.as_str(), "url" | "password"))
                .collect(),
        ),
        other => other,
    };
    let record = StoreRecord::from_record(store, sent).with("machine", SdkValue::text("native"));
    let kind = record.kind()?;
    // written only when it changed (#535 R5): most calls find it as it is
    let thin = StoreRecord::from_record(
        store,
        SdkValue::map([
            ("v", SdkValue::Int(1)),
            ("name", SdkValue::text(store)),
            ("kind", SdkValue::text(kind.as_str())),
        ]),
    );
    let local = StoreRecord::read(root, store)?;
    if local.as_ref().map(StoreRecord::to_record) != Some(thin.to_record()) {
        thin.write(root)?;
    }
    // the caller's record replaces one left by a project of the same name
    // that was removed and made again (its `created` differs); one this
    // side's own Init wrote (its manifest, no `created`: Init through the
    // worker, H11a) takes the caller's fields and keeps its manifest
    let sent = args.get("project_record").cloned().unwrap_or_default();
    let sent = ProjectRecord::from_record(store, project, sent);
    match ProjectRecord::read(root, store, project)? {
        None => sent.write(root)?,
        Some(local) if sent.created().is_none() || local.created() == sent.created() => {}
        Some(local) if local.created().is_none() => local.with_fields_of(&sent).write(root)?,
        Some(_) => sent.write(root)?,
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
    let postgresql = match kind {
        StoreKind::Postgresql => Some(args.get("postgresql").cloned().ok_or_else(|| {
            refused(
                "InvalidInput",
                "a PostgreSQL Sandbox call carries its server's resolved postgresql value",
            )
        })?),
        StoreKind::Sqlite => None,
    };
    let key = store_path(root, store, &record, kind);
    if let (Some(postgresql), "sandbox.create") = (&postgresql, op) {
        let who = match args.get_text("machine") {
            Some(machine) => format!("machine {machine}"),
            None => "the worker's machine".to_owned(),
        };
        reach_check(
            &who,
            (&record, &key, project),
            postgresql,
            args.get_text("reach_hint"),
        )?;
    }
    let held = postgresql.is_some();
    let host = host_args(
        root,
        (store, project, &record),
        (kind, postgresql),
        how,
        more,
    )?;
    // a PostgreSQL Store opens no Store service here (#65); its reads run
    // on the held read session
    let opened = match held {
        true => None,
        false => Some(layerfs_store::open::hold(&host, None)?),
    };
    if op == "backend.daemon.workspace.status" {
        return runner(op, host);
    }
    let name = host.get_text("sandbox").unwrap_or("").to_owned();
    let out = drain(runner(op, host)?, events);
    closed(op, opened.as_deref(), &out);
    let out = out?;
    // the caller reaches the new Sandbox's daemon directly where its
    // machine forwards the port (#535 G4): the answer carries its keys
    match op {
        "sandbox.create" => crate::direct::with_keys(root, &name, out),
        _ => Ok(out),
    }
}

/// A closed Sandbox's sqlite Store, held open here at `opened`: its log is
/// checkpointed and truncated before the close is answered (#86).
fn closed(op: &str, opened: Option<&Path>, out: &Outcome) {
    if let (Some(path), "sandbox.close", true) = (opened, op, out.is_ok()) {
        let _ = layerfs_store::open::released(path);
    }
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
    let mut buffer = vec![0u8; crate::output::INLINE];
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
                let read = crate::output::page(&args, &mut buffer)?;
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
    crate::output::release(&SdkValue::map([("output", SdkValue::text(&id))]))?;
    sent?;
    let out = match out {
        SdkValue::Map(fields) => {
            SdkValue::Map(fields.into_iter().filter(|(n, _)| n != "output").collect())
        }
        other => other,
    };
    Ok(home::with_field(out, "streamed", SdkValue::Bool(true)))
}
