//! The `project` family (#533 step 9): projects are `<store>/<project>`
//! everywhere. Init adds a folder as a new project to an existing Store, a
//! live add through the Store's writer (it does not seal), where the
//! Store's calls run: in the caller's process for a sqlite Store on
//! `native` or a PostgreSQL Store, in the machine's worker for a sqlite
//! Store on any other machine (#536 H11). The folder is always on the
//! caller's host: for a worker this process copies it to the machine
//! first (H11a). Records in the home list projects.
use crate::lease;
use crate::place::{self, Executor};
use crate::project::add::{add_project, ProjectAdd};
use crate::project::frame::InitFrame;
use crate::project::postgres_init::PgProjectAdd;
use crate::project::record::ProjectRecord;
use crate::project::{live, op};
use crate::record::{StoreRecord, StoreRef};
use crate::resolve;
use crate::sqlite::family::progress;
use layerfs_bridge::control::SdkValue;
use layerfs_home::events::Events;
use layerfs_home::names::{ProjectRef, StoreName};
use layerfs_home::op::FIELD;
use layerfs_home::outcome::{refused, Outcome};
use std::path::Path;

/// Default Init time limit: one hour.
const INIT_TIMEOUT_MS: i64 = 3_600_000;

/// `project.init(project, folder, { timeout })`.
#[derive(Clone, Debug)]
pub struct ProjectInit {
    /// The new project.
    pub project: ProjectRef,
    /// The folder on this host.
    pub folder: String,
    /// The time limit in milliseconds, [`INIT_TIMEOUT_MS`] when absent.
    pub timeout: Option<i64>,
}

/// The project.listed check `project doctor` makes: project `project`'s
/// record against its Store's own filtered list, read where the Store's
/// calls run. A project its Store no longer lists (retired, or absent) is
/// `NoSuchProject` even when its home entry remains (spec §4.2); a Store
/// that cannot be listed is its refusal. Answers the project's record
/// fields ([`ProjectRecord::answer`]).
pub fn get(home: &Path, project: &ProjectRef) -> Outcome {
    let target = resolve::target(home, &project.to_string())?;
    let live = live::projects(home, &target.store, &target.record, false)
        .map_err(|refusal| crate::unknown::named_unknown(refusal, "store", &target.store, None))?;
    let record = ProjectRecord::read(home, &target.store, &target.project)?
        .ok_or_else(|| refused("NoSuchProject", format!("no project {project}")))?;
    if live::find(&record, &target.project, &live).is_none() {
        return Err(refused("NoSuchProject", format!("no project {project}")));
    }
    Ok(layerfs_home::helpers::map_of(record.answer(&target.record)))
}

/// `project.init`'s answer: the project's record fields, `commit` (main's
/// first Layer, the full id), `root` and `outcome` `created`. `added` is
/// what the add answered (`layer`, `root`).
fn created(
    home: &Path,
    store: &StoreName,
    name: &str,
    record: &StoreRecord,
    added: &SdkValue,
) -> Outcome {
    let project = ProjectRecord::read(home, store.as_str(), name)?
        .unwrap_or_else(|| ProjectRecord::from_record(store.as_str(), name, SdkValue::Null));
    let mut fields: Vec<(String, SdkValue)> = vec![
        (
            "commit".to_owned(),
            added.get("layer").cloned().unwrap_or_default(),
        ),
        ("outcome".to_owned(), SdkValue::text("created")),
    ];
    fields.extend(
        project
            .answer(record)
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v)),
    );
    fields.push((
        "root".to_owned(),
        added.get("root").cloned().unwrap_or_default(),
    ));
    Ok(SdkValue::Map(fields))
}

/// The add of `folder` as project `name` of Store `store` in `home`.
fn project_add(
    home: &Path,
    store: &StoreName,
    name: &str,
    folder: &str,
    timeout: i64,
) -> ProjectAdd {
    ProjectAdd {
        home: home.to_path_buf(),
        store: store.clone(),
        name: name.to_owned(),
        source: folder.into(),
        timeout_ms: Some(timeout),
        store_record: None,
    }
}

/// `project.init(project, folder, { timeout, on_progress })`. Its progress
/// is a push's (#536 §10b), `{ phase, done, total }`: `check`, `send`
/// (bytes, to a worker only), `install` (Init adds the folder to the
/// Store) and `record` (the home's project record), whatever the route.
/// The project's entry is claimed with the Init's `op` before the first
/// effect and settled by the outcome ([`crate::project::op`]): while it
/// runs the project is listed `busy`, and an Init whose outcome is not
/// known leaves it `broken` `unresolved`.
pub fn init(home: &Path, events: &mut Events<'_>, request: &ProjectInit) -> Outcome {
    let home = home.to_path_buf();
    let (store, name) = (&request.project.store, request.project.project.as_str());
    let folder = request.folder.as_str();
    if !Path::new(folder).is_absolute() {
        return Err(refused(
            "InvalidInput",
            "the folder must be an absolute path",
        ));
    }
    // a folder that is not there, or not a directory, is refused before the
    // entry is claimed, so it leaves nothing behind whatever the route; a
    // link is left to the route, as before
    let link = std::fs::symlink_metadata(folder).is_ok_and(|meta| meta.file_type().is_symlink());
    if !link {
        crate::project::send::root(Path::new(folder))?;
    }
    let record = StoreRecord::read(&home, store)?.ok_or_else(|| {
        refused(
            "NotFound",
            format!("no Store named {store}; `store create {store}` makes one"),
        )
    })?;
    record.unblocked()?;
    progress(events, "check", None);
    let timeout = request.timeout.unwrap_or(INIT_TIMEOUT_MS);
    op::claim(&home, store.as_str(), name, "check")?;
    // a local add keeps its entry for diagnosis on a failure it cannot
    // classify; a worker's or a PostgreSQL add says `Unknown` when it does
    // not know
    let local = matches!(record.store_ref()?, StoreRef::Sqlite { .. })
        && matches!(place::plan(&home, record.store_ref()?), Ok(Executor::Local));
    let outcome = route(&home, events, (store, name), folder, timeout, &record);
    op::settle_init(&home, store.as_str(), name, &outcome, local);
    outcome
}

/// Init's add of `folder` as project `name` of Store `store`, where the
/// Store's calls run, into the entry the Init claimed.
fn route(
    home: &Path,
    events: &mut Events<'_>,
    (store, name): (&StoreName, &str),
    folder: &str,
    timeout: i64,
    record: &StoreRecord,
) -> Outcome {
    let home = home.to_path_buf();
    let store_ref = record.store_ref()?;
    if let StoreRef::Postgres { .. } = store_ref {
        return init_postgresql(&home, events, store, name, folder, timeout, record);
    }
    // Init runs where the Store's calls run (H11)
    if let Executor::Worker(worker) = place::plan(&home, store_ref)? {
        let call = InitCall {
            store,
            name,
            machine: store_ref.machine()?,
            folder,
            timeout,
        };
        return init_on_worker(&home, events, worker, call);
    }
    let _lease = lease::hold(&home, store)?;
    let add = project_add(&home, store, name, folder, timeout);
    progress(events, "install", None);
    op::phase(&home, store.as_str(), name, "install")?;
    let added = add_project(&add)?;
    // the project's record keeps when it was made, and the Init's `op` ends
    progress(events, "record", None);
    {
        let _lock = layerfs_home::home::lock::take(&home)?;
        if let Some(project) = ProjectRecord::read(&home, store, name)? {
            project.created_now().without(FIELD).write(&home)?;
        }
    }
    created(&home, store, name, record, &added)
}

/// What one `project.init` into a worker's Store names.
struct InitCall<'a> {
    store: &'a StoreName,
    name: &'a str,
    machine: &'a str,
    folder: &'a str,
    timeout: i64,
}

/// `project.init` into a sqlite Store on another machine (H11, H11a): this
/// process walks the folder on its own host (refusing what Init would
/// before the machine is reached), checks the machine has room for a copy,
/// copies it to the machine's worker ([`crate::project::send`]), and the
/// worker adds the copy under its own lease and removes the copy. This process holds the Store's lease meanwhile, as for
/// any call it sends there, then records the project in the home. One
/// attempt per request: a lost answer is `Unknown`, and the claimed entry
/// stays with its `op` uncertain (`project list` shows the project `broken`
/// `unresolved`; the Store may hold it).
fn init_on_worker(
    home: &Path,
    events: &mut Events<'_>,
    mut worker: crate::place::Worker,
    call: InitCall<'_>,
) -> Outcome {
    use crate::project::send;
    let home = home.to_path_buf();
    let InitCall {
        store,
        name,
        machine,
        folder,
        timeout,
    } = call;
    let total = send::measure(Path::new(folder), name)?;
    let copy = send::Copy {
        home: &home,
        store,
        name,
        machine,
        folder: Path::new(folder),
        id: send::new_id()?,
        timeout_ms: timeout,
    };
    send::room(&mut worker, &copy, total)?;
    // the lease is taken under the Store's lock with the record read there
    let (lease, record) = lease::claim(&home, store)?;
    // the copy is named on the Init's `op`, so the call that settles an
    // Init whose owner is gone (`project delete`) removes it (spec v20)
    let left = SdkValue::map([
        ("copy", SdkValue::text(&copy.id)),
        ("machine", SdkValue::text(machine)),
    ]);
    op::phase_with(&home, store.as_str(), name, "send", left)?;
    send::send(events, &worker, &copy, total)?;
    let request = InitFrame {
        store: store.clone(),
        name: Some(name.to_owned()),
        staged: copy.id.clone(),
        timeout_ms: Some(timeout),
        store_record: Some(record.to_record().clone()),
    }
    .encode();
    progress(events, "install", None);
    op::phase(&home, store.as_str(), name, "install")?;
    let sent_ms = layerfs_home::home::now_ms();
    let added = worker.call("project.init", request, &mut |event| events.event(event));
    drop(lease);
    let added = added.map_err(|refusal| {
        // A lost answer is `Unknown`; name the project it may have
        // initialized (spec §10 A6), whether the transport lost the reply or
        // the worker reported a bare `UnknownOutcome` (F43R4-2).
        let mut refusal = crate::unknown::named_unknown(refusal, "project", name, None);
        if refusal.code == "Unknown" {
            let left = send::left(machine, &copy.id);
            refusal.message = format!(
                "{}; the worker removes its copy {left} when its Init ends",
                refusal.message
            );
            // the worker's Init removes its copy when it ends; while it runs
            // it holds the copy, so `project delete` leaves it alone
            let left = send::Left {
                store,
                project: name,
                machine,
                copy: &copy.id,
                phase: "install",
                sent_ms,
                timeout_ms: timeout,
            };
            send::log_left(&home, &left, &refusal.message);
        }
        refusal
    })?;
    let stack = added.get("stack").cloned().unwrap_or_default();
    // the worker's `manifest` and `installed` complete the record (#749);
    // a worker that sends none (an older build) or sends ones that are not
    // a complete Normal pair leaves it for a move or a Sandbox create
    // to complete from the Store file, the latter logged: the project is
    // already in the Store, so the Init does not fail over it
    let initialised = ProjectRecord::initialised(store, name, stack);
    let project = match (added.get("manifest"), added.get("installed")) {
        (None, None) => initialised,
        (manifest, installed) => {
            let full = (initialised.clone())
                .with("manifest", manifest.cloned().unwrap_or_default())
                .with("installed", installed.cloned().unwrap_or_default());
            if crate::backup::restored::complete(&full) {
                full
            } else {
                let _ = layerfs_home::home::log_line(
                    &home,
                    "repair.jsonl",
                    &SdkValue::map([
                        ("at", SdkValue::Int(layerfs_home::home::now_ms())),
                        ("call", SdkValue::text("project.init")),
                        ("store", SdkValue::text(store.as_str())),
                        ("project", SdkValue::text(name)),
                        (
                            "message",
                            SdkValue::text(
                                "the worker's manifest and installed manifest are not a complete Normal pair; the record keeps neither",
                            ),
                        ),
                    ]),
                );
                initialised
            }
        }
    };
    progress(events, "record", None);
    {
        let _lock = layerfs_home::home::lock::take(&home)?;
        project.write(&home)?;
    }
    created(&home, store, name, &record, &added)
}

/// `project.init` into a PostgreSQL Store (#533 step 10b): the live add
/// runs in this process against the schema; the project's record names its
/// LayerStack, as `store.connect` writes it for the projects it finds.
fn init_postgresql(
    home: &Path,
    events: &mut Events<'_>,
    store: &StoreName,
    name: &str,
    folder: &str,
    timeout: i64,
    record: &StoreRecord,
) -> Outcome {
    let home = home.to_path_buf();
    let add = PgProjectAdd {
        add: ProjectAdd {
            store_record: Some(record.to_record().clone()),
            ..project_add(&home, store, name, folder, timeout)
        },
        postgresql: crate::postgresql::connect::selection(record)?,
    };
    // this process uses the Store meanwhile, as a sqlite Init does
    let _lease = lease::hold(&home, store)?;
    progress(events, "install", None);
    op::phase(&home, store.as_str(), name, "install")?;
    let added = crate::project::postgres_init::add_project(&add)?;
    let stack = added.get("stack").cloned().unwrap_or_default();
    progress(events, "record", None);
    ProjectRecord::initialised(store, name, stack).write(&home)?;
    created(&home, store, name, record, &added)
}
