//! The route of a Sandbox host operation (#535 §6.4): run in this process, or
//! sent to its machine's worker with the records it needs, and run there
//! (`in_worker`); the typed host request both sides build ([`Host`],
//! layerfs-project#108), and spooled output streamed back as events.
use super::deploy::{self, Side};
use super::record::SandboxRecord;
use crate::host::{Call, Deploy, Host, Message, Op, Sent};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::home::{self, paths};
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_machine::worker::Unreached;
use layerfs_store::open_args::PgSelection;
use layerfs_store::place::{self, Executor};
use layerfs_store::project::record::ProjectRecord;
use layerfs_store::record::{StoreKind, StoreRecord};
use layerfs_store::resolve;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The host Sandbox operations' request for project `<store>/<project>`,
/// naming Sandbox `sandbox` with `message`, run with the deploy `settings`
/// [`deploy::resolve`] gave; a PostgreSQL Store's `postgresql` is the one
/// the caller resolved and reached ([`reached`]).
fn host_args(
    home: &Path,
    (store, project, record): (&str, &str, &StoreRecord),
    (kind, postgresql): (StoreKind, Option<PgSelection>),
    settings: Deploy,
    (sandbox, message): (Option<String>, Message),
) -> Result<Host> {
    let prefix = format!("lfs-{}-", &layerfs_home::home::opened(home)?.id[..8]);
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
        std::fs::set_permissions(&local, std::fs::Permissions::from_mode(0o755)).map_err(failed)?;
    }
    // namespaces commands run as another identity: the home's folders
    // above the root must let them pass (a macOS native command does not)
    if backend == "namespaces" {
        home::traversable(home, &local)?;
    }
    let deploy = Deploy {
        sandbox: Some(backend.to_owned()),
        local_dir: Some(local.display().to_string()),
        container_prefix: Some(prefix),
        ..settings
    };
    Ok(Host {
        home: home.to_path_buf(),
        store: store.to_owned(),
        project: project.to_owned(),
        sandbox,
        store_record: Some(record.clone()),
        store_path: Some(store_path(home, store, record, kind).display().to_string()),
        deploy: Some(deploy),
        // a PostgreSQL Store's URL with its password, never written (H §7.4)
        postgresql,
        message,
        started: None,
    })
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
) -> Result<(PgSelection, Option<&'static str>)> {
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
    postgresql: &PgSelection,
    hint: Option<&str>,
) -> Result<()> {
    let url = postgresql.url.as_deref().unwrap_or("");
    let host = layerfs_sandbox_backend::PostgresUrl::parse(url)
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
                postgresql.schema.as_deref().unwrap_or("")
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
pub type Local = fn(Host, Call) -> Outcome;

/// One host operation as its caller asks it: the operation with its
/// fields, its Commit message, and the image a `sandbox.create` names.
pub struct Asked {
    /// The operation and its fields.
    pub call: Call,
    /// Its Commit message (exec and Commit send one).
    pub message: Message,
    /// The image `sandbox.create` names; every other call runs the
    /// Sandbox's own.
    pub image: Option<String>,
}

impl From<Call> for Asked {
    fn from(call: Call) -> Self {
        Self {
            call,
            message: Message::Absent,
            image: None,
        }
    }
}

/// Runs host Sandbox operation `asked` for Sandbox `record` on its machine,
/// refused `Unavailable` naming `layerfs machine start` when it is down.
pub fn on_machine(
    home: &Path,
    events: &mut Events<'_>,
    record: &SandboxRecord,
    asked: impl Into<Asked>,
    local: Local,
) -> Outcome {
    reach(home, events, record, asked, local).unwrap_or_else(|Unreached(refusal)| Err(refusal))
}

/// [`on_machine`], answering [`Unreached`] when the machine's worker was
/// not reached and nothing was sent (nothing starts it, #535 D11).
pub fn reach(
    home: &Path,
    events: &mut Events<'_>,
    record: &SandboxRecord,
    asked: impl Into<Asked>,
    local: Local,
) -> std::result::Result<Outcome, Unreached> {
    let mut unreached = None;
    let outcome = host_op(home, events, record, asked.into(), local, &mut unreached);
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
    record: &SandboxRecord,
    asked: Asked,
    local: Local,
    unreached: &mut Option<Unreached>,
) -> Outcome {
    let home = home.to_path_buf();
    let machine = record.machine();
    let project = record.project().unwrap_or("");
    let target = resolve::target(&home, project)?;
    let Asked {
        call,
        message,
        image,
    } = asked;
    let op = call.op();
    // the Sandbox's name is its identity on every machine
    let sandbox = Some(record.name().to_owned());
    // the call's image (sandbox.create) else the Sandbox's own travels in
    // the deploy settings, never as a top-level argument
    let image = match image {
        Some(text) => Some(text),
        _ => record
            .image()
            .filter(|_| op != Op::SandboxCreate)
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
    let choice = deploy::choose(&home, machine, image.as_deref())?;
    match executor {
        Executor::Local => {
            let key = store_path(&home, &target.store, &target.record, kind);
            if let (Some(postgresql), Op::SandboxCreate) = (&postgresql, op) {
                reach_check(
                    &format!("machine {machine}"),
                    (&target.record, &key, &target.project),
                    postgresql,
                    hint,
                )?;
            }
            let held = postgresql.is_some();
            let how = deploy::resolve(&Side::Home(&home), &choice)?;
            let host = host_args(
                &home,
                (&target.store, &target.project, &target.record),
                (kind, postgresql),
                how,
                (sandbox, message),
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
                false => (
                    None,
                    Some(layerfs_store::open::hold(&host.open_args(), Some(lease))?),
                ),
            };
            if op == Op::Status {
                return local(host, call);
            }
            let out = local(host, call)?;
            let out = drain(out, &mut |event| events.event(event));
            closed(op, opened.as_deref(), &out);
            out
        }
        Executor::Worker(mut worker) => {
            // the guest runs the same call on its own native machine, with
            // the records it needs sent along
            let project_record = ProjectRecord::read(&home, &target.store, &target.project)?
                .unwrap_or_else(|| {
                    ProjectRecord::from_record(&target.store, &target.project, SdkValue::Null)
                });
            let sent = Sent {
                call,
                message,
                sandbox,
                store: target.store.clone(),
                project: target.project.clone(),
                store_record: target.record.clone(),
                project_record,
                choice,
                machine: postgresql.as_ref().map(|_| machine.to_owned()),
                reach_hint: postgresql.as_ref().and(hint).map(str::to_owned),
                postgresql,
            };
            let request = crate::host::encode(&sent)?;
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

/// Runs a Sandbox host operation in a worker for its caller (`sandbox.host`,
/// decoded by [`crate::host::decode`]): the caller sends the Store's and
/// project's records; the worker keeps a thin record of the Store (#536 §6)
/// and the project's record under its root, with the Store on this
/// machine's own `native`.
pub fn in_worker(
    root: &Path,
    sent: Sent,
    events: &mut dyn FnMut(SdkValue),
    runner: Local,
) -> Outcome {
    let Sent {
        call,
        message,
        sandbox,
        store,
        project,
        store_record,
        project_record,
        choice,
        postgresql,
        machine,
        reach_hint,
    } = sent;
    let op = call.op();
    let (store, project) = (store.as_str(), project.as_str());
    layerfs_home::names::StoreName::parse(store)?;
    layerfs_home::names::ProjectName::parse(project)?;
    let record = store_record.with("machine", SdkValue::text("native"));
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
    let sent = project_record;
    match ProjectRecord::read(root, store, project)? {
        None => sent.write(root)?,
        Some(local) if sent.created().is_none() || local.created() == sent.created() => {}
        Some(local) if local.created().is_none() => local.with_fields_of(&sent).write(root)?,
        Some(_) => sent.write(root)?,
    }
    let how = deploy::resolve(&Side::Worker(root), &choice)?;
    let postgresql = match kind {
        StoreKind::Postgresql => Some(postgresql.ok_or_else(|| {
            refused(
                "InvalidInput",
                "a PostgreSQL Sandbox call carries its server's resolved postgresql value",
            )
        })?),
        StoreKind::Sqlite => None,
    };
    let key = store_path(root, store, &record, kind);
    if let (Some(postgresql), Op::SandboxCreate) = (&postgresql, op) {
        let who = match &machine {
            Some(machine) => format!("machine {machine}"),
            None => "the worker's machine".to_owned(),
        };
        reach_check(
            &who,
            (&record, &key, project),
            postgresql,
            reach_hint.as_deref(),
        )?;
    }
    let held = postgresql.is_some();
    let host = host_args(
        root,
        (store, project, &record),
        (kind, postgresql),
        how,
        (sandbox, message),
    )?;
    // a PostgreSQL Store opens no Store service here (#65); its reads run
    // on the held read session
    let opened = match held {
        true => None,
        false => Some(layerfs_store::open::hold(&host.open_args(), None)?),
    };
    if op == Op::Status {
        return runner(host, call);
    }
    let name = host.sandbox.clone().unwrap_or_default();
    let out = match &call {
        // a command whose record its caller keeps: its start goes back to
        // that caller as an event while it runs
        Call::Exec {
            workspace: Some(_), ..
        } => reporting(host, call, runner, events),
        _ => runner(host, call),
    };
    let out = drain(out?, events);
    closed(op, opened.as_deref(), &out);
    let out = out?;
    // the caller reaches the new Sandbox's daemon directly where its
    // machine forwards the port (#535 G4): the answer carries its keys
    match op {
        Op::SandboxCreate => crate::direct::with_keys(root, &name, out),
        _ => Ok(out),
    }
}

/// Runs `call` (an exec whose record its caller keeps) with `runner` on a
/// thread of its own, passing each event its exec sends through `started`
/// to `events` as it comes, until it returns. The wait for events ends when
/// the runner does: nothing here times it.
fn reporting(
    mut host: Host,
    call: Call,
    runner: Local,
    events: &mut dyn FnMut(SdkValue),
) -> Outcome {
    let (sender, received) = std::sync::mpsc::channel();
    host.started = Some(sender.clone());
    std::thread::scope(|scope| {
        let ran = scope.spawn(move || {
            // its end is told even when the runner panics
            let _ended = Ended(sender);
            runner(host, call)
        });
        while let Ok(event) = received.recv() {
            if event == SdkValue::Null {
                break;
            }
            events(event);
        }
        ran.join()
            .unwrap_or_else(|_| Err(refused("Failed", "the exec stopped before it answered")))
    })
}

/// Tells [`reporting`] that its runner ended, when dropped.
struct Ended(std::sync::mpsc::Sender<SdkValue>);

impl Drop for Ended {
    fn drop(&mut self) {
        let _ = self.0.send(SdkValue::Null);
    }
}

/// A closed Sandbox's sqlite Store, held open here at `opened`: its log is
/// checkpointed and truncated before the close is answered (#86).
fn closed(op: Op, opened: Option<&Path>, out: &Outcome) {
    if let (Some(path), Op::SandboxClose, true) = (opened, op, out.is_ok()) {
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
            let mut offset = 0u64;
            loop {
                let (length, end) = crate::output::page(&id, stream, offset, &mut buffer)?;
                if length > 0 {
                    emit(SdkValue::map([
                        ("stream", SdkValue::text(stream)),
                        ("bytes", SdkValue::Bytes(buffer[..length].to_vec())),
                    ]));
                }
                offset += length as u64;
                if end || length == 0 {
                    break;
                }
            }
        }
        Ok::<(), SdkRefusal>(())
    })();
    crate::output::release(&id)?;
    sent?;
    let out = match out {
        SdkValue::Map(fields) => {
            SdkValue::Map(fields.into_iter().filter(|(n, _)| n != "output").collect())
        }
        other => other,
    };
    Ok(home::with_field(out, "streamed", SdkValue::Bool(true)))
}
