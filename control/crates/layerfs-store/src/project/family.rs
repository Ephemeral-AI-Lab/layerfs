//! The `project` family (#533 step 9): projects are `<store>/<project>`
//! everywhere. Init adds a folder as a new project to an existing Store, a
//! live add through the Store's writer (it does not seal), where the
//! Store's calls run: in the caller's process for a sqlite Store on
//! `native` or a PostgreSQL Store, in the machine's worker for a sqlite
//! Store on any other machine (#536 H11). The folder is always on the
//! caller's host: for a worker this process copies it to the machine
//! first (H11a). Records in the home list projects; `remove` drops one
//! project's history from its Store.
use crate::lease;
use crate::place::{self, Executor};
use crate::project::record::ProjectRecord;
use crate::record::{StoreRecord, StoreRef};
use crate::resolve;
use crate::sqlite::family::progress;
use crate::Related;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::home::paths;
use layerfs_home::outcome::{failed, refused, Outcome, View};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Default Init time limit: one hour.
const INIT_TIMEOUT_MS: i64 = 3_600_000;

fn split(project: &str) -> Result<(&str, &str)> {
    project.split_once('/').ok_or_else(|| {
        refused(
            "InvalidInput",
            format!("project `{project}` must be written with its Store"),
        )
    })
}

/// One project as callers see it; a record that does not read shows no
/// `created`.
fn summary(home: &Path, store: &str, name: &str, record: &StoreRecord, view: View) -> SdkValue {
    let project = ProjectRecord::read(home, store, name).ok().flatten();
    let project =
        project.unwrap_or_else(|| ProjectRecord::from_record(store, name, SdkValue::Null));
    project.to_sdk(view, record)
}

/// `project.get(project)`: one project's record.
pub fn get(home: &Path, args: &SdkValue) -> Outcome {
    let project = args.get_text("project").unwrap_or("");
    let target = resolve::target(home, project)?;
    Ok(summary(
        home,
        &target.store,
        &target.project,
        &target.record,
        View::Full,
    ))
}

/// `project.list(store?, { page_size, page_pos })`, by Store then name.
pub fn list(home: &Path, args: &SdkValue) -> Outcome {
    let stores = match args.get_text("store") {
        Some(store) => {
            layerfs_home::names::StoreName::parse(store)?;
            if !paths::store_record(home, store).exists() {
                return Err(refused("NotFound", format!("no Store named {store}")));
            }
            vec![store.to_owned()]
        }
        None => paths::names(&home.join("store")).map_err(failed)?,
    };
    let mut items = Vec::new();
    for store in stores {
        let Some(record) = StoreRecord::read(home, &store)? else {
            continue;
        };
        for name in paths::names(&paths::store_dir(home, &store).join("project")).map_err(failed)? {
            items.push(summary(home, &store, &name, &record, View::List));
        }
    }
    let size = args.get_int("page_size").unwrap_or(50).clamp(1, 1000) as usize;
    let pos = args.get_int("page_pos").unwrap_or(0).max(0) as usize;
    let total = items.len();
    let page: Vec<SdkValue> = items.into_iter().skip(pos).take(size).collect();
    let next = if pos + page.len() < total {
        SdkValue::Int((pos + page.len()) as i64)
    } else {
        SdkValue::Null
    };
    Ok(SdkValue::map([
        ("items", SdkValue::List(page)),
        ("next_page_pos", next),
    ]))
}

/// The host operation's arguments for project `name` of Store `store`.
fn host_args(home: &Path, store: &str, more: Vec<(&str, SdkValue)>) -> SdkValue {
    let mut fields = vec![
        (
            "home".to_owned(),
            SdkValue::text(home.display().to_string()),
        ),
        ("store".to_owned(), SdkValue::text(store)),
        (
            "storePath".to_owned(),
            SdkValue::text(paths::store_file(home, store).display().to_string()),
        ),
    ];
    fields.extend(more.into_iter().map(|(n, v)| (n.to_owned(), v)));
    SdkValue::Map(fields)
}

/// `project.init(project, folder, { timeout, on_progress })`. Its progress
/// is a push's (#536 §10b), `{ phase, done, total }`: `check`, `send`
/// (bytes, to a worker only), `install` (Init adds the folder to the
/// Store) and `record` (the home's project record), whatever the route.
pub fn init(home: &Path, events: &mut Events<'_>, args: &SdkValue) -> Outcome {
    let home = home.to_path_buf();
    let project = args.get_text("project").unwrap_or("");
    let (store, name) = split(project)?;
    layerfs_home::names::ProjectName::parse(name)?;
    let folder = args.get_text("folder").unwrap_or("");
    if !Path::new(folder).is_absolute() {
        return Err(refused(
            "InvalidInput",
            "the folder must be an absolute path",
        ));
    }
    let record = StoreRecord::read(&home, store)?.ok_or_else(|| {
        refused(
            "NotFound",
            format!("no Store named {store}; `store create {store}` makes one"),
        )
    })?;
    record.unblocked()?;
    progress(events, "check", None);
    let timeout = args.get_int("timeout").unwrap_or(INIT_TIMEOUT_MS);
    let store_ref = record.store_ref()?;
    if let StoreRef::Postgres { .. } = store_ref {
        return init_postgresql(&home, events, store, name, folder, timeout, &record);
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
    let call = host_args(
        &home,
        store,
        vec![
            ("name", SdkValue::text(name)),
            ("path", SdkValue::text(folder)),
            ("timeoutMs", SdkValue::Int(timeout)),
        ],
    );
    progress(events, "install", None);
    let added = crate::identity::add_project(&call)?;
    // the project's record keeps when it was made
    progress(events, "record", None);
    {
        let _lock = layerfs_home::home::lock::take(&home)?;
        if let Some(project) = ProjectRecord::read(&home, store, name)? {
            project.created_now().write(&home)?;
        }
    }
    let mut out = summary(&home, store, name, &record, View::Full);
    for field in ["layer", "root"] {
        if let Some(value) = added.get(field) {
            out = layerfs_home::home::with_field(out, field, value.clone());
        }
    }
    Ok(out)
}

/// What one `project.init` into a worker's Store names.
struct InitCall<'a> {
    store: &'a str,
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
/// attempt per request: a lost answer is `Unknown`, and the home gains no
/// record (`project list` does not show a project whose outcome is not
/// known; the Store may hold it).
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
    if paths::project_record(&home, store, name).exists() {
        return Err(refused(
            "Exists",
            format!("Store {store} already has a project named {name}"),
        ));
    }
    let total = send::measure(Path::new(folder), name)?;
    let copy = send::Copy {
        store,
        name,
        machine,
        folder: Path::new(folder),
        id: send::new_id()?,
    };
    send::room(&mut worker, &copy, total)?;
    // the lease is taken under the Store's lock with the record read there
    let (lease, record) = lease::claim(&home, store)?;
    send::send(events, &mut worker, &copy, total)?;
    let request = SdkValue::map([
        ("store", SdkValue::text(store)),
        ("name", SdkValue::text(name)),
        ("staged", SdkValue::text(&copy.id)),
        ("timeoutMs", SdkValue::Int(timeout)),
        ("store_record", record.to_record().clone()),
    ]);
    progress(events, "install", None);
    let added = worker.call("project.init", request, &mut |event| events.event(event));
    drop(lease);
    let added = added.map_err(|mut refusal| {
        if refusal.code == "Unknown" {
            let left = send::left(machine, &copy.id);
            refusal.message = format!(
                "{}; the worker removes its copy {left} when its Init ends",
                refusal.message
            );
        }
        refusal
    })?;
    let stack = added.get("stack").cloned().unwrap_or_default();
    progress(events, "record", None);
    {
        let _lock = layerfs_home::home::lock::take(&home)?;
        ProjectRecord::initialised(store, name, stack).write(&home)?;
    }
    let mut out = summary(&home, store, name, &record, View::Full);
    for field in ["layer", "root"] {
        if let Some(value) = added.get(field) {
            out = layerfs_home::home::with_field(out, field, value.clone());
        }
    }
    Ok(out)
}

/// `project.init` into a PostgreSQL Store (#533 step 10b): the live add
/// runs in this process against the schema; the project's record names its
/// LayerStack, as `store.connect` writes it for the projects it finds.
fn init_postgresql(
    home: &Path,
    events: &mut Events<'_>,
    store: &str,
    name: &str,
    folder: &str,
    timeout: i64,
    record: &StoreRecord,
) -> Outcome {
    let home = home.to_path_buf();
    let path = paths::project_record(&home, store, name);
    if path.exists() {
        return Err(refused(
            "Exists",
            format!("Store {store} already has a project named {name}"),
        ));
    }
    let call = host_args(
        &home,
        store,
        vec![
            ("name", SdkValue::text(name)),
            ("path", SdkValue::text(folder)),
            ("timeoutMs", SdkValue::Int(timeout)),
            ("store_record", record.to_record().clone()),
            ("postgresql", crate::postgresql::connect::selection(record)?),
        ],
    );
    // this process uses the Store meanwhile, as a sqlite Init does
    let _lease = lease::hold(&home, store)?;
    progress(events, "install", None);
    let added = crate::project::postgres_init::add_project(&call)?;
    let stack = added.get("stack").cloned().unwrap_or_default();
    progress(events, "record", None);
    ProjectRecord::initialised(store, name, stack).write(&home)?;
    let mut out = summary(&home, store, name, record, View::Full);
    for field in ["layer", "root"] {
        if let Some(value) = added.get(field) {
            out = layerfs_home::home::with_field(out, field, value.clone());
        }
    }
    Ok(out)
}

/// `project.remove(project)`: drops the project's Layers, Branches and
/// Commits from its Store in one write, then its record. Its Sandboxes must
/// be closed first; the other projects stay.
pub fn remove(home: &Path, events: &mut Events<'_>, args: &SdkValue, related: Related) -> Outcome {
    let project = args.get_text("project").unwrap_or("");
    let home = home.to_path_buf();
    let target = resolve::target(&home, project)?;
    target.record.unblocked()?;
    (related.can_remove_project)(&home, project)?;
    let args = vec![
        ("project", SdkValue::text(&target.project)),
        ("store_record", target.record.to_record().clone()),
    ];
    if let StoreRef::Postgres { .. } = target.store_ref {
        return Err(refused(
            "Unsupported",
            "removing a project from a PostgreSQL Store is not built",
        ));
    }
    let removed = match place::plan(&home, &target.store_ref)? {
        Executor::Local => {
            let _lease = lease::hold(&home, &target.store)?;
            crate::identity::remove_project(&host_args(&home, &target.store, args))?
        }
        Executor::Worker(mut worker) => {
            let _lease = lease::hold(&home, &target.store)?;
            let mut fields = vec![("store".to_owned(), SdkValue::text(&target.store))];
            fields.extend(args.into_iter().map(|(n, v)| (n.to_owned(), v)));
            worker.call("project.remove", SdkValue::Map(fields), &mut |e| {
                events.event(e)
            })?
        }
    };
    let entry = paths::project_dir(&home, &target.store, &target.project);
    if entry.exists() {
        std::fs::remove_dir_all(&entry).map_err(failed)?;
    }
    Ok(layerfs_home::home::with_field(
        removed,
        "project",
        SdkValue::text(project),
    ))
}
