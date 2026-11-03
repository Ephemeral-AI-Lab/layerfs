//! The route of a Sandbox host operation (#535 §6.4): run in this process, or
//! sent to its machine's worker with the records it needs, and run there
//! (`in_worker`); the host arguments both sides build, and spooled output
//! streamed back as events.
use super::deploy::{self, Deploy, Side};
use super::record::SandboxRecord;
use crate::common::context::Context;
use crate::common::home::{self, paths};
use crate::common::outcome::{failed, refused, Outcome};
use crate::common::place::{self, Executor};
use crate::machine::worker::Unreached;
use crate::project::record::ProjectRecord;
use crate::store::record::{StoreKind, StoreRecord};
use crate::store::resolve;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
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
    let prefix = SdkValue::text(format!("lfs-{}-", &home::open(home)?.id[..8]));
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
            SdkValue::text(store_path(home, store, kind).display().to_string()),
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
/// (`layerfs_machine::reach`). It travels only in the one request that runs
/// the call and is never written to a record.
fn reached(home: &Path, machine: &str, record: &StoreRecord) -> Result<SdkValue> {
    let url = crate::common::secrets::url(record)?;
    // a build without machines (the guest's) runs its own Sandboxes on the
    // machine it is, which shares its loopback
    #[cfg(feature = "machine")]
    let url = {
        let id = home::open(home)?.id;
        let spec =
            crate::machine::machine_spec(home, &id, &crate::machine::machine_of(home, machine)?)?;
        layerfs_machine::reach(&spec, &url).map_err(crate::machine::machine_refusal)?
    };
    #[cfg(not(feature = "machine"))]
    let _ = (home, machine);
    Ok(crate::store::connect::selection_at(record, url))
}

/// The reach check of a PostgreSQL Sandbox's create (#535 §7.4 step 4), on
/// the side that launches it: one connection to the server it will dial
/// reads the LayerFS schema's binding, which must be the Store record's.
/// It proves the server is reached and is the same Store; a failure names
/// the host it tried, never the URL.
fn reach_check(who: &str, store: &StoreRecord, postgresql: &SdkValue) -> Result<()> {
    let url = postgresql.get_text("url").unwrap_or("");
    let host = layerfs_sandbox::PostgresUrl::parse(url)
        .map(|url| url.host().to_owned())
        .unwrap_or_default();
    let probe = SdkValue::map([("postgresql", postgresql.clone())]);
    let binding = crate::store::postgres::read_binding(&probe).map_err(|refusal| {
        refused(
            "Unavailable",
            format!(
                "{who} cannot reach PostgreSQL Store {}'s server at {host}: {}",
                store.name(),
                refusal.message
            ),
        )
    })?;
    if store.binding() != Some(binding.as_slice()) {
        return Err(refused(
            "Unavailable",
            format!(
                "the PostgreSQL server {who} reaches at {host} holds another Store than {}",
                store.name()
            ),
        ));
    }
    Ok(())
}

/// The path the open-Store cache keys `store` by: its file, or for a
/// PostgreSQL Store the path [`route`] keys it by, which no file has.
fn store_path(home: &Path, store: &str, kind: StoreKind) -> PathBuf {
    match kind {
        StoreKind::Postgresql => paths::store_dir(home, store).join("postgresql"),
        StoreKind::Sqlite => paths::store_file(home, store),
    }
}

/// Runs host Sandbox operation `op` for Sandbox `record` on its machine,
/// refused `Unavailable` naming `layerfs machine start` when it is down.
pub(crate) fn on_machine(
    context: &mut Context<'_>,
    record: &SandboxRecord,
    op: &str,
    more: Vec<(&str, SdkValue)>,
) -> Outcome {
    reach(context, record, op, more, true).unwrap_or_else(|Unreached(refusal)| Err(refusal))
}

/// [`on_machine`], answering [`Unreached`] when the machine's worker was
/// not reached and nothing was sent (nothing starts it, #535 D11);
/// `load_image` says whether the call may load the docker machine's
/// default image (a get or status call does not, P4).
pub(crate) fn reach(
    context: &mut Context<'_>,
    record: &SandboxRecord,
    op: &str,
    more: Vec<(&str, SdkValue)>,
    load_image: bool,
) -> std::result::Result<Outcome, Unreached> {
    let mut unreached = None;
    let outcome = host_op(context, (record, op), more, load_image, &mut unreached);
    match unreached {
        Some(unreached) => Err(unreached),
        None => Ok(outcome),
    }
}

/// [`reach`]'s operation; a request that was not sent leaves its
/// [`Unreached`] in `unreached`.
fn host_op(
    context: &mut Context<'_>,
    (record, op): (&SandboxRecord, &str),
    more: Vec<(&str, SdkValue)>,
    load_image: bool,
    unreached: &mut Option<Unreached>,
) -> Outcome {
    let home = context.home.clone();
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
    let postgresql = match kind {
        StoreKind::Postgresql => Some(reached(&home, machine, &target.record)?),
        StoreKind::Sqlite => None,
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
            if let (Some(postgresql), "sandbox.create") = (&postgresql, op) {
                reach_check(&format!("machine {machine}"), &target.record, postgresql)?;
            }
            let how = deploy::resolve(&Side::Home(&home), &target.store, &choice)?;
            let args = host_args(
                &home,
                (&target.store, &target.project, &target.record),
                (kind, postgresql),
                how,
                more,
            )?;
            // the host code reads the project's Branches through its open
            // Store, which this process leases, PostgreSQL ones too
            let lease = crate::common::home::lease::hold(&home, &target.store)?;
            crate::store::open::hold(&args, Some(lease))?;
            if op == "backend.daemon.workspace.status" {
                return crate::sandbox::dispatch::backend(op.to_owned(), args);
            }
            let out = crate::sandbox::dispatch::call(op, args)?;
            drain(out, &mut |event| context.event(event))
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
            }
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
/// the caller sends the Store's and project's records; the worker keeps a
/// thin record of the Store (#536 §6) and the project's record under its
/// root, with the Store on this machine's own `native`.
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
    crate::common::names::StoreName::parse(store)?;
    crate::common::names::ProjectName::parse(project)?;
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
    // that was removed and made again (its `created` differs)
    let sent = args.get("project_record").cloned().unwrap_or_default();
    let sent = ProjectRecord::from_record(store, project, sent);
    let stale = match ProjectRecord::read(root, store, project)? {
        None => true,
        Some(local) => sent.created().is_some() && local.created() != sent.created(),
    };
    if stale {
        sent.write(root)?;
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
    if let (Some(postgresql), "sandbox.create") = (&postgresql, op) {
        reach_check("the worker's machine", &record, postgresql)?;
    }
    let host = host_args(
        root,
        (store, project, &record),
        (kind, postgresql),
        how,
        more,
    )?;
    crate::store::open::hold(&host, None)?;
    if op == "backend.daemon.workspace.status" {
        return crate::sandbox::dispatch::backend(op.to_owned(), host);
    }
    let name = host.get_text("sandbox").unwrap_or("").to_owned();
    let out = drain(crate::sandbox::dispatch::call(op, host)?, events)?;
    // the caller reaches the new Sandbox's daemon directly where its
    // machine forwards the port (#535 G4): the answer carries its keys
    match op {
        "sandbox.create" => crate::sandbox::direct::with_keys(root, &name, out),
        _ => Ok(out),
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
