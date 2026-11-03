//! The `project` family (#533 step 9): projects are `<store>/<project>`
//! everywhere. Init adds a folder as a new project to an existing Store, a
//! live add through the Store's writer (it does not seal), in the caller's
//! process, into a sqlite Store on `native`; a project for another machine
//! is made on `native` and moved with its Store. Records in the home list
//! projects; `remove` drops one project's history from its Store.
use crate::common::context::Context;
use crate::common::home::lease;
use crate::common::home::paths;
use crate::common::outcome::{failed, refused, Outcome, View};
use crate::common::place::{self, Executor, Start};
use crate::project::record::ProjectRecord;
use crate::store::record::{StoreRecord, StoreRef};
use crate::store::resolve;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
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

/// Runs one `project` call.
pub fn run(context: &mut Context<'_>, call: &str, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    match call {
        "list" => list(&home, args),
        "get" => {
            let project = args.get_text("project").unwrap_or("");
            let target = resolve::target(&home, project)?;
            Ok(summary(
                &home,
                &target.store,
                &target.project,
                &target.record,
                View::Full,
            ))
        }
        "init" => init(context, args),
        "remove" => remove(context, args.get_text("project").unwrap_or("")),
        _ => Err(refused("InvalidInput", format!("no call project.{call}"))),
    }
}

/// `project.list(store?, { page_size, page_pos })`, by Store then name.
fn list(home: &Path, args: &SdkValue) -> Outcome {
    let stores = match args.get_text("store") {
        Some(store) => {
            crate::common::names::StoreName::parse(store)?;
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

/// `project.init(project, folder, { timeout, on_progress })`.
fn init(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    let project = args.get_text("project").unwrap_or("");
    let (store, name) = split(project)?;
    crate::common::names::ProjectName::parse(name)?;
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
    record.moving()?;
    let timeout = args.get_int("timeout").unwrap_or(INIT_TIMEOUT_MS);
    let store_ref = record.store_ref()?;
    if let StoreRef::Postgres { .. } = store_ref {
        return init_postgresql(context, store, name, folder, timeout, &record);
    }
    // Init runs where the Store's calls run in this process
    if let Executor::Worker(worker) = place::plan(&home, store_ref, Start::Never)? {
        let machine = worker.machine();
        return Err(refused(
            "Unsupported",
            format!(
                "Store {store} is on machine {machine}; Init runs on native: \
                 `store move {store} native`, Init, then move it back"
            ),
        ));
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
    let watch = host_args(&home, store, vec![("project", SdkValue::text(name))]);
    let added = observed(context, &watch, || {
        crate::store::identity::add_project(&call)
    })?;
    // the project's record keeps when it was made
    {
        let _lock = crate::common::home::lock::take(&home)?;
        if let Some(project) = ProjectRecord::read(&home, store, name)? {
            project.created_now().write(&home)?;
        }
    }
    let mut out = summary(&home, store, name, &record, View::Full);
    for field in ["layer", "root"] {
        if let Some(value) = added.get(field) {
            out = crate::common::home::with_field(out, field, value.clone());
        }
    }
    Ok(out)
}

/// Runs an add on its own thread while this one reports its counts.
fn observed(
    context: &mut Context<'_>,
    watch: &SdkValue,
    add: impl FnOnce() -> Outcome + Send,
) -> Outcome {
    let outcome = std::thread::scope(|scope| {
        let worker = scope.spawn(add);
        let mut last = SdkValue::Null;
        while !worker.is_finished() {
            if let Ok(progress) = crate::project::init::progress(watch) {
                if progress != SdkValue::Null && progress != last {
                    context.progress(progress.clone());
                    last = progress;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        worker.join()
    });
    outcome.map_err(|_| refused("Failed", "the Init thread panicked"))?
}

/// `project.init` into a PostgreSQL Store (#533 step 10b): the live add
/// runs in this process against the schema; the project's record names its
/// LayerStack, as `store.connect` writes it for the projects it finds.
fn init_postgresql(
    context: &mut Context<'_>,
    store: &str,
    name: &str,
    folder: &str,
    timeout: i64,
    record: &StoreRecord,
) -> Outcome {
    let home = context.home.clone();
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
            ("postgresql", crate::store::connect::selection(record)?),
        ],
    );
    let watch = host_args(&home, store, vec![("project", SdkValue::text(name))]);
    let added = observed(context, &watch, || {
        crate::project::postgres_init::add_project(&call)
    })?;
    let stack = added.get("stack").cloned().unwrap_or_default();
    ProjectRecord::initialised(store, name, stack).write(&home)?;
    let mut out = summary(&home, store, name, record, View::Full);
    for field in ["layer", "root"] {
        if let Some(value) = added.get(field) {
            out = crate::common::home::with_field(out, field, value.clone());
        }
    }
    Ok(out)
}

/// `project.remove(project)`: drops the project's Layers, Branches and
/// Commits from its Store in one write, then its record. Its Sandboxes must
/// be closed first; the other projects stay.
fn remove(context: &mut Context<'_>, project: &str) -> Outcome {
    let home = context.home.clone();
    let target = resolve::target(&home, project)?;
    target.record.moving()?;
    crate::inventory::can_remove_project(&home, project)?;
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
    let removed = match place::plan(&home, &target.store_ref, Start::OnDemand)? {
        Executor::Local => {
            let _lease = lease::hold(&home, &target.store)?;
            crate::store::identity::remove_project(&host_args(&home, &target.store, args))?
        }
        Executor::Worker(mut worker) => {
            let _lease = lease::hold(&home, &target.store)?;
            let mut fields = vec![("store".to_owned(), SdkValue::text(&target.store))];
            fields.extend(args.into_iter().map(|(n, v)| (n.to_owned(), v)));
            worker.call("project.remove", SdkValue::Map(fields), &mut |e| {
                context.event(e)
            })?
        }
    };
    let entry = paths::project_dir(&home, &target.store, &target.project);
    if entry.exists() {
        std::fs::remove_dir_all(&entry).map_err(failed)?;
    }
    Ok(crate::common::home::with_field(
        removed,
        "project",
        SdkValue::text(project),
    ))
}
