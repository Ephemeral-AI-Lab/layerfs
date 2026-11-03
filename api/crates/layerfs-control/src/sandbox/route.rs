//! The route of a Sandbox host operation (#535 §6.4): run in this process, or
//! sent to its machine's worker with the records it needs, and run there
//! (`in_worker`); the host arguments both sides build, and spooled output
//! streamed back as events.
use super::deploy::{self, Deploy, Side};
use super::record::SandboxRecord;
use crate::common::context::Context;
use crate::common::home::{self, paths};
use crate::common::outcome::{failed, refused, Outcome};
use crate::common::place::{self, Executor, Start};
use crate::machine::worker::Unreached;
use crate::project::record::ProjectRecord;
use crate::store::record::{StoreKind, StoreRecord};
use crate::store::resolve;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The host Sandbox operations' arguments for project `<store>/<project>`,
/// plus `more`, run as `how` says.
fn host_args(
    home: &Path,
    (store, project, record): (&str, &str, &StoreRecord),
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
        ("store_record", record.to_record().clone()),
        (
            "storePath",
            SdkValue::text(store_path(home, store, kind).display().to_string()),
        ),
        ("deploy", deploy),
    ];
    // a PostgreSQL Store's URL with its password, resolved now and never
    // written (H §7.4)
    if kind == StoreKind::Postgresql {
        let url = crate::store::connect::url(record)?;
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

/// Runs host Sandbox operation `op` for Sandbox `record` on its machine,
/// starting the machine when it is down.
pub(crate) fn on_machine(
    context: &mut Context<'_>,
    record: &SandboxRecord,
    op: &str,
    more: Vec<(&str, SdkValue)>,
) -> Outcome {
    reach(context, record, op, more, Start::OnDemand)
        .unwrap_or_else(|Unreached(refusal)| Err(refusal))
}

/// [`on_machine`], with `start` saying whether a machine that is down is
/// started; [`Unreached`] when it was not and nothing was sent.
pub(crate) fn reach(
    context: &mut Context<'_>,
    record: &SandboxRecord,
    op: &str,
    more: Vec<(&str, SdkValue)>,
    start: Start,
) -> std::result::Result<Outcome, Unreached> {
    let mut unreached = None;
    let outcome = host_op(context, (record, op), more, start, &mut unreached);
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
    start: Start,
    unreached: &mut Option<Unreached>,
) -> Outcome {
    let home = context.home.clone();
    let machine = record.machine();
    let project = record.project().unwrap_or("");
    let target = resolve::target(&home, project)?;
    let mut more = more;
    if let Some(id) = record.id() {
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
            .image()
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
    crate::common::names::StoreName::parse(store)?;
    crate::common::names::ProjectName::parse(project)?;
    let sent = args.get("store_record").cloned().unwrap_or_default();
    let record = StoreRecord::from_record(store, sent).with("machine", SdkValue::text("native"));
    // written only when it changed (#535 R5): most calls send the same one
    let local = StoreRecord::read(root, store)?;
    if local.as_ref().map(StoreRecord::to_record) != Some(record.to_record()) {
        record.write(root)?;
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
    let kind = record.kind()?;
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
