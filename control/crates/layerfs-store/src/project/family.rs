//! The `project` family (#533 step 9): projects are `<store>/<project>`
//! everywhere. Init adds a folder as a new project to an existing Store, a
//! live add through the Store's writer (it does not seal), where the
//! Store's calls run: in the caller's process for a sqlite Store on
//! `native` or a PostgreSQL Store, in the machine's worker for a sqlite
//! Store on any other machine (#536 H11). The folder is always on the
//! caller's host: for a worker this process copies it to the machine
//! first (H11a). Records in the home list projects; `remove` drops one
//! project's history from its Store.
use crate::calls::Page;
use crate::lease;
use crate::place::{self, Executor};
use crate::project::add::{add_project, ProjectAdd};
use crate::project::discovery::home_of;
use crate::project::frame::{InitFrame, RemoveFrame};
use crate::project::postgres_init::PgProjectAdd;
use crate::project::record::ProjectRecord;
use crate::record::{StoreRecord, StoreRef};
use crate::resolve;
use crate::sqlite::family::progress;
use crate::Related;
use layerfs_bridge::control::SdkValue;
use layerfs_home::events::Events;
use layerfs_home::home::paths;
use layerfs_home::names::{ProjectRef, StoreName};
use layerfs_home::outcome::{failed, refused, Outcome, View};
use std::path::Path;

/// Default Init time limit: one hour.
const INIT_TIMEOUT_MS: i64 = 3_600_000;

/// `project.list(store?, { page_size, page_pos })`.
#[derive(Clone, Debug, Default)]
pub struct ProjectList {
    /// The one Store it lists.
    pub store: Option<StoreName>,
    /// The page.
    pub page: Page,
}

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

/// One project as callers see it; a record that does not read shows no
/// `created`.
fn summary(home: &Path, store: &str, name: &str, record: &StoreRecord, view: View) -> SdkValue {
    let project = ProjectRecord::read(home, store, name).ok().flatten();
    let project =
        project.unwrap_or_else(|| ProjectRecord::from_record(store, name, SdkValue::Null));
    project.to_sdk(view, record)
}

/// `project.get(project)`: one project's record.
pub fn get(home: &Path, project: &ProjectRef) -> Outcome {
    let target = resolve::target(home, &project.to_string())?;
    Ok(summary(
        home,
        &target.store,
        &target.project,
        &target.record,
        View::Full,
    ))
}

/// `project.list(store?, { page_size, page_pos })`, by Store then name.
pub fn list(home: &Path, request: &ProjectList) -> Outcome {
    let stores = match &request.store {
        Some(store) => {
            if !paths::store_record(home, store).exists() {
                return Err(refused("NotFound", format!("no Store named {store}")));
            }
            vec![store.to_string()]
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
    let size = request.page.size.unwrap_or(50).clamp(1, 1000) as usize;
    let pos = request.page.pos.unwrap_or(0).max(0) as usize;
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
    let record = StoreRecord::read(&home, store)?.ok_or_else(|| {
        refused(
            "NotFound",
            format!("no Store named {store}; `store create {store}` makes one"),
        )
    })?;
    record.unblocked()?;
    progress(events, "check", None);
    let timeout = request.timeout.unwrap_or(INIT_TIMEOUT_MS);
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
    let add = project_add(&home, store, name, folder, timeout);
    progress(events, "install", None);
    let added = add_project(&add)?;
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
    let request = InitFrame {
        store: store.clone(),
        name: Some(name.to_owned()),
        staged: copy.id.clone(),
        timeout_ms: Some(timeout),
        store_record: Some(record.to_record().clone()),
    }
    .encode();
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
    store: &StoreName,
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
    let added = crate::project::postgres_init::add_project(&add)?;
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
pub fn remove(
    home: &Path,
    events: &mut Events<'_>,
    project: &ProjectRef,
    related: Related,
) -> Outcome {
    let text = project.to_string();
    let home = home.to_path_buf();
    let target = resolve::target(&home, &text)?;
    target.record.unblocked()?;
    (related.can_remove_project)(&home, &text)?;
    let request = RemoveFrame {
        store: project.store.clone(),
        project: target.project.clone(),
        store_record: Some(target.record.to_record().clone()),
    };
    if let StoreRef::Postgres { .. } = target.store_ref {
        return Err(refused(
            "Unsupported",
            "removing a project from a PostgreSQL Store is not built",
        ));
    }
    let removed = match place::plan(&home, &target.store_ref)? {
        Executor::Local => {
            let _lease = lease::hold(&home, &target.store)?;
            crate::project::remove::remove_project(&home_of(Some(&home))?, &request)?
        }
        Executor::Worker(mut worker) => {
            let _lease = lease::hold(&home, &target.store)?;
            worker.call("project.remove", request.encode(), &mut |e| events.event(e))?
        }
    };
    let entry = paths::project_dir(&home, &target.store, &target.project);
    if entry.exists() {
        std::fs::remove_dir_all(&entry).map_err(failed)?;
    }
    Ok(layerfs_home::home::with_field(
        removed,
        "project",
        SdkValue::text(text),
    ))
}
