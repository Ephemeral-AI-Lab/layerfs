//! Store calls in this process (#352 F-020): the only route for fs, branch,
//! project and operator Store calls.
//!
//! One `StoreService` per Store path per process, opened by the first client
//! that names the path and closed when the last one releases it. It is a live
//! opener of the installed Store, as a daemon is: never sealed here. The
//! kernel guard runs before any file is opened; where it refuses (macOS, a
//! filesystem that does not carry SQLite locks), every Store call fails
//! `Unsupported` with its reason. There is no other route.
//!
//! The private overlays file-changing calls construct through live in this
//! process's temporary directory and are removed with the service. Each call
//! runs once: a refused or failed call returns its own error, nothing is sent
//! again.
use crate::outcome::{failed, refused, Outcome};
use crate::registry::lock;
use layerfs_bridge::control::{SdkCall, SdkRefusal, SdkValue};
use layerfs_store_service::{telemetry, OpenRefusal, StoreService};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

/// One opened Store and the clients that hold it.
struct Opened {
    service: Option<StoreService>,
    clients: AtomicUsize,
    /// This service's private overlay directory.
    directory: Option<PathBuf>,
}

impl Opened {
    /// Closes the service and removes its overlay directory, reporting both.
    fn close(mut self) -> Result<(), SdkRefusal> {
        let mut result = Ok(());
        if let Some(service) = self.service.take() {
            if let Err(error) = service.close() {
                result = Err(refused("Failed", format!("Store close: {error}")));
            }
        }
        if let Some(directory) = self.directory.take() {
            if let Err(error) = std::fs::remove_dir_all(&directory) {
                if error.kind() != std::io::ErrorKind::NotFound && result.is_ok() {
                    result = Err(failed(error));
                }
            }
        }
        result
    }
}

impl Drop for Opened {
    fn drop(&mut self) {
        let service = self.service.take();
        let directory = self.directory.take();
        if let Some(service) = service {
            let _ = service.close();
        }
        if let Some(directory) = directory {
            let _ = std::fs::remove_dir_all(directory);
        }
    }
}

static STORES: LazyLock<Mutex<BTreeMap<PathBuf, Arc<Opened>>>> = LazyLock::new(Default::default);
static DIRECTORIES: AtomicU64 = AtomicU64::new(0);

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
fn open(store: &Path, args: &SdkValue) -> Result<Arc<Opened>, SdkRefusal> {
    let project = crate::discovery::project(args)?;
    let directory = std::env::temp_dir().join(format!(
        "layerfs-store-{}-{}",
        std::process::id(),
        DIRECTORIES.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&directory).map_err(failed)?;
    let opened = StoreService::open_direct(
        store,
        &project.manifest.binding,
        project.manifest.cursor_key,
        &directory.join("overlay.sqlite"),
    );
    match opened {
        Ok(service) => Ok(Arc::new(Opened {
            service: Some(service),
            clients: AtomicUsize::new(0),
            directory: Some(directory),
        })),
        Err(refusal) => {
            let _ = std::fs::remove_dir_all(&directory);
            Err(match refusal {
                OpenRefusal::Unsupported(reason) => refused("Unsupported", reason.0),
                OpenRefusal::Missing(reason) => refused("NoStore", reason),
                OpenRefusal::Failed(reason) => refused("Failed", reason),
            })
        }
    }
}

/// `storeOpen`: one more client of the Store a call names, opening it first.
pub fn acquire(args: &SdkValue) -> Outcome {
    let store = path(args)?;
    let mut stores = lock(&STORES)?;
    let opened = match stores.get(&store) {
        Some(opened) => Arc::clone(opened),
        None => {
            let opened = open(&store, args)?;
            stores.insert(store.clone(), Arc::clone(&opened));
            opened
        }
    };
    let clients = opened.clients.fetch_add(1, Ordering::AcqRel) + 1;
    Ok(SdkValue::map([("clients", SdkValue::Int(clients as i64))]))
}

/// `storeClose`: one client less; the last one stops the service and removes
/// its overlay directory. A call still running keeps the Opened alive, and
/// the last owner closes both when it drops.
pub fn release(args: &SdkValue) -> Outcome {
    let store = path(args)?;
    let opened = {
        let mut stores = lock(&STORES)?;
        let Some(opened) = stores.get(&store).cloned() else {
            return Err(refused("InvalidInput", "this process holds no such Store"));
        };
        let previous = opened
            .clients
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |clients| {
                Some(clients.saturating_sub(1))
            })
            .unwrap_or(0);
        if previous > 1 {
            return Ok(SdkValue::map([(
                "clients",
                SdkValue::Int((previous - 1) as i64),
            )]));
        }
        stores.remove(&store);
        opened
    };
    match Arc::try_unwrap(opened) {
        Ok(opened) => opened.close()?,
        // In-flight calls keep the Opened alive; its drop closes both.
        Err(_) => {}
    }
    Ok(SdkValue::map([("clients", SdkValue::Int(0))]))
}

/// The open Store a call names.
fn service(args: &SdkValue) -> Result<Arc<Opened>, SdkRefusal> {
    let store = path(args)?;
    lock(&STORES)?
        .get(&store)
        .cloned()
        .ok_or_else(|| refused("InvalidInput", "the client has not opened its Store"))
}

/// Runs one Store call (`operator` for the operator table) in this process.
pub(crate) fn call(method: &str, args: SdkValue, operator: bool) -> Outcome {
    let opened = service(&args)?;
    let service = opened
        .service
        .as_ref()
        .ok_or_else(|| refused("Failed", "the Store is closing"))?;
    let args = match args {
        SdkValue::Map(fields) => SdkValue::Map(
            fields
                .into_iter()
                .filter(|(name, _)| {
                    !matches!(name.as_str(), "storePath" | "sandbox" | "deploy" | "home")
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
        if operator {
            layerfs_store_service::operator::dispatch(service, &call)
        } else {
            layerfs_api::call(service, &call)
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
    let opened = service(args)?;
    let service = opened
        .service
        .as_ref()
        .ok_or_else(|| refused("Failed", "the Store is closing"))?;
    let stack = layerfs_store_service::operator::args::project(&service.store, args)?;
    Ok(layerfs_store_service::history::lookup::branch(&service.store, &stack, name)?.id)
}

/// The newest Commit of `branch`, read in this process; `None` when it has none.
pub(crate) fn head(
    args: &SdkValue,
    branch: layerfs_history::BranchId,
) -> Result<Option<layerfs_history::CommitId>, SdkRefusal> {
    let opened = service(args)?;
    let service = opened
        .service
        .as_ref()
        .ok_or_else(|| refused("Failed", "the Store is closing"))?;
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
