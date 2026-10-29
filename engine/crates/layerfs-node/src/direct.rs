//! Store calls in this process (#352 F-020): the only route for fs, branch,
//! project and backend Store calls. A view's store path (#510) opens the
//! view's own read-only service (`views`).
//!
//! One `StoreService` per Store path per process, opened by the first client
//! that names the path and closed when the last one releases it. It is a live
//! opener of the installed Store, as a daemon is: never sealed here. The
//! kernel guard runs before any file is opened; where it refuses (macOS, a
//! filesystem that does not carry SQLite locks), every Store call fails
//! `Unsupported` with its reason. There is no other route.
//!
//! The private overlays file-changing calls construct through live in this
//! process's temporary directory, under a random name so a directory left by
//! a crashed process with the same pid, or by another copy of the addon in
//! this process, never blocks the open (#436). They are removed with the
//! service; a process that ends without closing leaves its own. Each call
//! runs once: a refused or failed call returns its own error, nothing is sent
//! again.
use crate::job::{failed, refused, Outcome};
use crate::registry::lock;
use layerfs_bridge::control::{SdkCall, SdkRefusal, SdkValue};
use layerfs_store_service::{telemetry, OpenRefusal, StoreService};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

/// One opened Store and the clients that hold it.
struct Opened {
    service: Arc<StoreService>,
    clients: usize,
    /// This service's private overlay directory; none for a view (#510).
    directory: Option<PathBuf>,
}

static STORES: LazyLock<Mutex<BTreeMap<PathBuf, Opened>>> = LazyLock::new(Default::default);

/// The Store path a call names.
fn path(args: &SdkValue) -> Result<PathBuf, SdkRefusal> {
    match args.get_text("storePath") {
        Some(path) if !path.is_empty() => Ok(PathBuf::from(path)),
        _ => Err(refused(
            "InvalidInput",
            "Store calls need the client's storePath (the installed Store file)",
        )),
    }
}

/// Opens the Store at `store` for this process, or refuses.
fn open(store: &Path, args: &SdkValue) -> Result<Opened, SdkRefusal> {
    // #510: a view's store path names an open view; it constructs nothing
    if let Some(view) = store.to_str().filter(|path| crate::views::is_view(path)) {
        return Ok(Opened {
            service: crate::views::service(view)?,
            clients: 0,
            directory: None,
        });
    }
    let identity = crate::stores::identity(args)?;
    let directory = std::env::temp_dir().join(format!(
        "layerfs-store-{}-{}",
        std::process::id(),
        layerfs_bridge::sdk_values::hex(&crate::registry::random()?[..12])
    ));
    std::fs::create_dir(&directory).map_err(failed)?;
    let template = directory.join("overlay.sqlite");
    let opened = match identity.kind {
        crate::stores::Kind::Sqlite => {
            StoreService::open_direct(store, &identity.binding, identity.cursor_key, &template)
                .map_err(|refusal| match refusal {
                    OpenRefusal::Unsupported(reason) => refused("Unsupported", reason.0),
                    OpenRefusal::Missing(reason) => refused("NoStore", reason),
                    OpenRefusal::Failed(reason) => refused("Failed", reason),
                })
        }
        crate::stores::Kind::Postgresql { .. } => {
            crate::stores::open_postgres(args, &identity, &template)
        }
    };
    match opened {
        Ok(service) => Ok(Opened {
            service: Arc::new(service),
            clients: 0,
            directory: Some(directory),
        }),
        Err(refusal) => {
            let _ = std::fs::remove_dir_all(&directory);
            Err(refusal)
        }
    }
}

/// `storeOpen`: one more client of the Store a call names, opening it first.
pub(crate) fn acquire(args: &SdkValue) -> Outcome {
    let store = path(args)?;
    let mut stores = lock(&STORES)?;
    if !stores.contains_key(&store) {
        let opened = open(&store, args)?;
        stores.insert(store.clone(), opened);
    }
    let opened = stores.get_mut(&store).expect("opened above");
    opened.clients += 1;
    Ok(SdkValue::map([(
        "clients",
        SdkValue::Int(opened.clients as i64),
    )]))
}

/// `storeClose`: one client less; the last one closes the Store and removes
/// its overlay directory. A call still running keeps the service alive until
/// it returns, and the service closes when it is dropped then.
pub(crate) fn release(args: &SdkValue) -> Outcome {
    let store = path(args)?;
    let mut stores = lock(&STORES)?;
    let Some(opened) = stores.get_mut(&store) else {
        return Err(refused("InvalidInput", "this process holds no such Store"));
    };
    opened.clients = opened.clients.saturating_sub(1);
    if opened.clients > 0 {
        return Ok(SdkValue::map([(
            "clients",
            SdkValue::Int(opened.clients as i64),
        )]));
    }
    let opened = stores.remove(&store).expect("present above");
    drop(stores);
    let closed = match Arc::try_unwrap(opened.service) {
        Ok(service) => service.close().map_err(failed),
        Err(_) => Ok(()),
    };
    let removed = match &opened.directory {
        Some(directory) => std::fs::remove_dir_all(directory).map_err(failed),
        None => Ok(()),
    };
    closed.and(removed)?;
    Ok(SdkValue::map([("clients", SdkValue::Int(0))]))
}

/// The open service of the Store a call names.
fn service(args: &SdkValue) -> Result<Arc<StoreService>, SdkRefusal> {
    let store = path(args)?;
    lock(&STORES)?
        .get(&store)
        .map(|opened| opened.service.clone())
        .ok_or_else(|| refused("InvalidInput", "the client has not opened its Store"))
}

/// Runs `work` on the Store a call names, opened for it when this process
/// holds it not yet and closed after when no other client holds it (#506
/// Store-level calls: nothing stays open for a pull to find).
pub(crate) fn with_service<T>(
    args: &SdkValue,
    work: impl FnOnce(&Arc<StoreService>) -> Result<T, SdkRefusal>,
) -> Result<T, SdkRefusal> {
    acquire(args)?;
    let result = service(args).and_then(|service| work(&service));
    let released = release(args);
    let value = result?;
    released?;
    Ok(value)
}

/// Whether this process holds the Store at `path` open.
pub(crate) fn holds(path: &Path) -> Result<bool, SdkRefusal> {
    Ok(lock(&STORES)?.contains_key(path))
}

/// Runs one Store call (`backend` for the backend table) in this process;
/// on a view, a failed remote read answers with its own kind (#510).
pub(crate) fn call(method: &str, args: SdkValue, backend: bool) -> Outcome {
    match args.get_text("storePath") {
        Some(path) if crate::views::is_view(path) => {
            let path = path.to_owned();
            crate::views::answered(&path, || served(method, args, backend))
        }
        _ => served(method, args, backend),
    }
}

fn served(method: &str, args: SdkValue, backend: bool) -> Outcome {
    let service = service(&args)?;
    let args = match args {
        SdkValue::Map(fields) => SdkValue::Map(
            fields
                .into_iter()
                .filter(|(name, _)| {
                    !matches!(
                        name.as_str(),
                        "storePath" | "sandbox" | "deploy" | "home" | "store" | "postgresql"
                    )
                })
                .collect(),
        ),
        other => other,
    };
    let call = SdkCall {
        method: method.to_owned(),
        args,
    };
    let run = || {
        if backend {
            layerfs_store_service::operator::dispatch(&service, &call)
        } else {
            layerfs_api::call(&service, &call)
        }
    };
    if !crate::trace::traced() {
        return run();
    }
    // A traced call records its own timing, SQL and delta work, as a daemon
    // did for a traced observation.
    let sessions = service.opened.sql_sessions();
    let before = (sessions.totals(), service.store.save_counters());
    let recording = telemetry::Recording::start();
    let outcome = run();
    let reports = recording.finish();
    let mut fields = vec![("timing".to_owned(), telemetry::timing(&reports))];
    let sql = telemetry::sql(before.0, sessions.totals());
    let delta = telemetry::delta(before.1, service.store.save_counters());
    for (name, value) in [("sql", sql), ("delta", delta)] {
        fields.extend(value.map(|value| (name.to_owned(), value)));
    }
    crate::trace::collect(SdkValue::Map(fields));
    outcome
}

/// The Branch id of `name` in the project a call names, read in this process.
pub(crate) fn branch_id(
    args: &SdkValue,
    name: &str,
) -> Result<layerfs_history::BranchId, SdkRefusal> {
    let service = service(args)?;
    let stack = layerfs_store_service::operator::args::project(&service.store, args)?;
    Ok(layerfs_store_service::history::lookup::branch(&service.store, &stack, name)?.id)
}

/// The newest Commit of `branch`, read in this process; `None` when it has none.
pub(crate) fn head(
    args: &SdkValue,
    branch: layerfs_history::BranchId,
) -> Result<Option<layerfs_history::CommitId>, SdkRefusal> {
    let service = service(args)?;
    let page = service
        .store
        .published(|history| {
            history.commit_history(&layerfs_history::CommitHistoryRequest {
                branch,
                start: None,
                cursor: None,
                limit: 1,
            })
        })
        .map_err(failed)?;
    Ok(page.records.first().map(|record| record.id))
}
