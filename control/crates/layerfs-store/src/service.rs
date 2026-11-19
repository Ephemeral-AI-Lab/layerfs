//! Store calls in this process (#352 F-020): the only route for fs, branch
//! and project Store calls.
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
use crate::open_args::{OpenArgs, ROUTING};
use layerfs_bridge::control::{SdkCall, SdkRefusal, SdkValue};
use layerfs_home::helpers::lock;
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_store_service::{OpenRefusal, StoreService};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

/// One opened Store and the clients that hold it.
struct Opened {
    service: Arc<StoreService>,
    clients: usize,
    /// This service's private overlay directory.
    directory: PathBuf,
}

static STORES: LazyLock<Mutex<BTreeMap<PathBuf, Opened>>> = LazyLock::new(Default::default);

/// Opens the Store at `store` for this process, or refuses.
fn open(store: &Path, args: &OpenArgs) -> Result<Opened, SdkRefusal> {
    let identity = crate::identity::identity(args)?;
    let directory = std::env::temp_dir().join(format!(
        "layerfs-store-{}-{}",
        std::process::id(),
        layerfs_bridge::sdk_values::hex(&layerfs_home::helpers::random()?[..12])
    ));
    std::fs::create_dir(&directory).map_err(failed)?;
    let template = directory.join("overlay.sqlite");
    let opened = match identity.kind {
        crate::identity::Kind::Sqlite => {
            StoreService::open_direct(store, &identity.binding, identity.cursor_key, &template)
                .map_err(|refusal| match refusal {
                    OpenRefusal::Unsupported(reason) => refused("Unsupported", reason.0),
                    OpenRefusal::Missing(reason) => refused("NoStore", reason),
                    OpenRefusal::Busy(reason) => refused("Busy", reason),
                    OpenRefusal::Failed(reason) => refused("Failed", reason),
                })
        }
        crate::identity::Kind::Postgresql { .. } => {
            crate::postgresql::open::open_postgres(args.postgresql.as_ref(), &identity, &template)
        }
    };
    match opened {
        Ok(service) => Ok(Opened {
            service: Arc::new(service),
            clients: 0,
            directory,
        }),
        Err(refusal) => {
            let _ = std::fs::remove_dir_all(&directory);
            Err(refusal)
        }
    }
}

/// One more client of the Store a call names, opening it first.
pub(crate) fn acquire(args: &OpenArgs) -> Result<(), SdkRefusal> {
    let store = args.store_path()?;
    let mut stores = lock(&STORES)?;
    if !stores.contains_key(&store) {
        let opened = open(&store, args)?;
        stores.insert(store.clone(), opened);
    }
    let opened = stores.get_mut(&store).expect("opened above");
    opened.clients += 1;
    Ok(())
}

/// One client less; the last one closes the Store and removes its overlay
/// directory. A call still running keeps the service alive until it
/// returns, and the service closes when it is dropped then.
pub(crate) fn release(args: &OpenArgs) -> Result<(), SdkRefusal> {
    let store = args.store_path()?;
    let mut stores = lock(&STORES)?;
    let Some(opened) = stores.get_mut(&store) else {
        return Err(refused("InvalidInput", "this process holds no such Store"));
    };
    opened.clients = opened.clients.saturating_sub(1);
    if opened.clients > 0 {
        return Ok(());
    }
    let opened = stores.remove(&store).expect("present above");
    drop(stores);
    let closed = match Arc::try_unwrap(opened.service) {
        Ok(service) => service.close().map_err(failed),
        Err(_) => Ok(()),
    };
    let removed = std::fs::remove_dir_all(&opened.directory).map_err(failed);
    closed.and(removed)
}

/// The open service of the Store a call names.
fn service(args: &OpenArgs) -> Result<Arc<StoreService>, SdkRefusal> {
    let store = args.store_path()?;
    lock(&STORES)?
        .get(&store)
        .map(|opened| opened.service.clone())
        .ok_or_else(|| refused("InvalidInput", "the client has not opened its Store"))
}

/// Runs `work` on the Store a call names, opened for it when this process
/// holds it not yet and closed after when no other client holds it (#506
/// Store-level calls: nothing stays open for a pull to find).
pub fn with_service<T>(
    args: &OpenArgs,
    work: impl FnOnce(&Arc<StoreService>) -> Result<T, SdkRefusal>,
) -> Result<T, SdkRefusal> {
    acquire(args)?;
    let result = service(args).and_then(|service| work(&service));
    let released = release(args);
    let value = result?;
    released?;
    Ok(value)
}

/// Runs one Store call in this process on the Store `open` names, with
/// the call's own arguments `args` (any routing key in them is dropped).
pub fn call(method: &str, open: &OpenArgs, args: SdkValue) -> Outcome {
    let service = service(open)?;
    let args = match args {
        SdkValue::Map(fields) => SdkValue::Map(
            fields
                .into_iter()
                .filter(|(name, _)| !ROUTING.contains(&name.as_str()))
                .collect(),
        ),
        other => other,
    };
    let call = SdkCall {
        method: method.to_owned(),
        args,
    };
    layerfs_store_service::methods::call(&service, &call)
}

/// The newest Commit of `branch`, read in this process; `None` when it has none.
pub fn head(
    args: &OpenArgs,
    branch: layerfs_history::BranchId,
) -> Result<Option<layerfs_history::CommitId>, SdkRefusal> {
    if args.postgresql.is_some() {
        return crate::postgresql::reader::head(args, branch);
    }
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

/// Ends the mount intent Workspace `workspace` of Branch `branch` left in
/// the Store `args` names, if one remains: a forced `sandbox.remove` calls
/// it once the Sandbox whose daemon owned the intent is stopped, so the
/// intent does not refuse every later compaction (RC-B F1). Each one left
/// is released once, or marked uncertain when the Store refuses the delete
/// (`release_or_mark`, the daemon's own unmount release); answers how many
/// there were. Nothing is retried.
pub fn end_mount(
    args: &OpenArgs,
    branch: layerfs_history::BranchId,
    workspace: [u8; 32],
) -> Result<usize, SdkRefusal> {
    with_service(args, |service| {
        let catalog = service.store.history();
        let scope = layerfs_history::GcScopeKind::Branch;
        let intents = catalog
            .gc_intents(scope, branch.as_slice())
            .map_err(failed)?;
        let left = intents.iter().filter(|intent| {
            intent.kind == layerfs_history::GcIntentKind::Mount
                && intent.workspace == Some(workspace)
        });
        let mut ended = 0;
        for intent in left {
            layerfs_store_service::history::admission::release_or_mark(catalog, &intent.op_id)
                .map_err(failed)?;
            ended += 1;
        }
        Ok(ended)
    })
}
