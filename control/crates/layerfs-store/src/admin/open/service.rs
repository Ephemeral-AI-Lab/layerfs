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
use crate::admin::open::args::{OpenArgs, ROUTING};
use layerfs_home::helpers::lock;
use layerfs_home::outcome::{failed, refused, with_fix, Outcome};
use layerfs_protocol::sdk::{SdkCall, SdkRefusal, SdkValue};
use layerfs_store_service::{OpenRefusal, StoreService};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

/// The entry points allowed to borrow the service. A host call's outer
/// cache slot accounts for every host borrower until its reply is ready.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum Route {
    Direct,
    Host,
}

impl Route {
    fn check(self, opened: &Opened) -> Result<(), SdkRefusal> {
        if self == opened.owner {
            Ok(())
        } else {
            Err(refused(
                "InUse",
                "the Store service belongs to another route",
            ))
        }
    }
}

/// One opened Store and the clients that hold it.
struct Opened {
    owner: Route,
    service: Arc<StoreService>,
    clients: usize,
    /// This service's private overlay directory.
    directory: PathBuf,
}

/// A Store occupies its slot from reservation through physical release.
pub(crate) enum Slot<T, C> {
    Opening(Arc<Completion<()>>),
    Ready(T),
    Closing(Arc<Completion<C>>),
}

type ServiceSlot = Slot<Opened, ()>;
static STORES: Mutex<BTreeMap<PathBuf, ServiceSlot>> = Mutex::new(BTreeMap::new());

/// Shares the result of one transition with callers already waiting for it.
pub(crate) struct Completion<T> {
    result: Mutex<Option<Result<T, SdkRefusal>>>,
    done: Condvar,
}

impl<T: Clone> Completion<T> {
    pub(crate) fn new() -> Self {
        Self {
            result: Mutex::new(None),
            done: Condvar::new(),
        }
    }

    pub(crate) fn finish(&self, result: Result<T, SdkRefusal>) -> Result<T, SdkRefusal> {
        *self
            .result
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(result.clone());
        self.done.notify_all();
        result
    }

    pub(crate) fn wait(&self) -> Result<T, SdkRefusal> {
        let result = self
            .result
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let result = self
            .done
            .wait_while(result, |result| result.is_none())
            .unwrap_or_else(|error| error.into_inner());
        result.as_ref().expect("transition finished").clone()
    }
}

/// Lets the owner repair its cache slot and wake waiters even when open or
/// close unwinds. The initiating caller still observes its original panic.
pub(crate) fn transition<T, R>(
    work: impl FnOnce() -> Result<T, SdkRefusal>,
    publish: impl FnOnce(Result<T, SdkRefusal>) -> R,
) -> R {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)) {
        Ok(result) => publish(result),
        Err(panic) => {
            publish(Err(refused("Failed", "Store cache transition panicked")));
            std::panic::resume_unwind(panic)
        }
    }
}

/// Opens the Store at `store` for this process, or refuses.
fn open(store: &Path, args: &OpenArgs, owner: Route) -> Result<Opened, SdkRefusal> {
    let identity = crate::admin::record::identity::identity(args)?;
    let directory = overlay_directory()?;
    let template = directory.join("overlay.sqlite");
    let opened = match identity.kind {
        crate::admin::record::identity::Kind::Sqlite => {
            StoreService::open_direct(store, &identity.binding, identity.cursor_key, &template)
                .map_err(|refusal| match refusal {
                    OpenRefusal::Unsupported(reason) => refused("Unsupported", reason.0),
                    OpenRefusal::Missing(reason) => refused("NoStore", reason),
                    OpenRefusal::Busy(reason) => refused("Busy", reason),
                    OpenRefusal::Outdated(status) => {
                        refused(crate::admin::status::state::OLD_STORE, status)
                    }
                    OpenRefusal::Failed(reason) => refused("Failed", reason),
                })
        }
        crate::admin::record::identity::Kind::Postgresql { .. } => {
            crate::backend::postgresql::open::open_postgres(
                args.postgresql.as_ref(),
                &identity,
                &template,
            )
        }
    };
    // an older-format Store names the call that clears it (`state::outdated_fix`)
    let opened = opened.map_err(|refusal| with_upgrade_fix(args, &identity, refusal));
    match opened {
        Ok(service) => Ok(Opened {
            owner,
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

/// A new private overlay directory in this process's temporary directory,
/// under a random name (#436), readable only by its owner.
fn overlay_directory() -> Result<PathBuf, SdkRefusal> {
    let directory = std::env::temp_dir().join(format!(
        "layerfs-store-{}-{}",
        std::process::id(),
        layerfs_protocol::sdk::hex(&layerfs_home::helpers::random()?[..12])
    ));
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(&directory).map_err(failed)?;
    Ok(directory)
}

fn with_upgrade_fix(
    args: &OpenArgs,
    identity: &crate::admin::record::identity::Identity,
    refusal: SdkRefusal,
) -> SdkRefusal {
    match (&args.store, refusal.code.as_str()) {
        (Some(store), crate::admin::status::state::OLD_STORE) => {
            let postgresql = matches!(
                identity.kind,
                crate::admin::record::identity::Kind::Postgresql { .. }
            );
            let fix =
                crate::admin::status::state::outdated_fix(store, postgresql, &refusal.message);
            with_fix(refusal, fix)
        }
        _ => refusal,
    }
}

/// Direct calls that use only published data, with no writer or Workspace.
pub(crate) fn is_one_shot_read(method: &str) -> bool {
    matches!(method, "fs.read" | "fs.stat" | "fs.list")
}

/// Checks the authoritative record's binding on the only session this read uses.
pub(crate) fn postgres_read(method: &str, args: &OpenArgs, fields: SdkValue) -> Outcome {
    let service = open_postgres_read(args)?;
    layerfs_home::trace::step("store_open");
    let outcome = dispatch(&service, method, fields);
    layerfs_home::trace::step("call");
    drop(service);
    layerfs_home::trace::step("store_close");
    outcome
}

/// A PostgreSQL read service over one new session, Full-checked against
/// the authoritative record's binding: one call's (P-A), or a host
/// worker's held session (#819 P-B, [`crate::postgresql::pool`]).
pub(crate) fn open_postgres_read(args: &OpenArgs) -> Result<StoreService, SdkRefusal> {
    let identity = crate::admin::record::identity::identity(args)?;
    crate::backend::postgresql::open::open_read_only(args.postgresql.as_ref(), &identity)
        .map_err(|refusal| with_upgrade_fix(args, &identity, refusal))
}

/// A writable PostgreSQL service over new sessions, held by a host worker
/// between CLI writes (#819 P-B, [`crate::postgresql::pool`]), with its own
/// private overlay directory, which the caller removes after closing it.
pub(crate) fn open_postgres_write(args: &OpenArgs) -> Result<(StoreService, PathBuf), SdkRefusal> {
    let identity = crate::admin::record::identity::identity(args)?;
    let directory = overlay_directory()?;
    let template = directory.join("overlay.sqlite");
    let opened = crate::backend::postgresql::open::open_postgres_held(
        args.postgresql.as_ref(),
        &identity,
        &template,
    )
    .map_err(|refusal| with_upgrade_fix(args, &identity, refusal));
    match opened {
        Ok(service) => Ok((service, directory)),
        Err(refusal) => {
            let _ = std::fs::remove_dir_all(&directory);
            Err(refusal)
        }
    }
}

/// One more client of the Store a call names, opening it first.
pub(crate) fn acquire(args: &OpenArgs, owner: Route) -> Result<(), SdkRefusal> {
    let store = args.store_path()?;
    let pending = loop {
        let mut stores = lock(&STORES)?;
        match stores.get_mut(&store) {
            Some(Slot::Ready(opened)) => {
                owner.check(opened)?;
                opened.clients += 1;
                return Ok(());
            }
            Some(Slot::Opening(pending) | Slot::Closing(pending)) => {
                let pending = pending.clone();
                drop(stores);
                pending.wait()?;
            }
            None => {
                let pending = Arc::new(Completion::new());
                stores.insert(store.clone(), Slot::Opening(pending.clone()));
                break pending;
            }
        }
    };
    transition(
        || open(&store, args, owner),
        |result| {
            let mut stores = lock(&STORES)?;
            pending.finish(match result {
                Ok(mut opened) => {
                    opened.clients = 1;
                    *stores.get_mut(&store).expect("open reserved") = Slot::Ready(opened);
                    Ok(())
                }
                Err(refusal) => {
                    stores.remove(&store);
                    Err(refusal)
                }
            })
        },
    )
}

/// One client less; the last one closes the Store and removes its overlay
/// directory. A call still running keeps the service alive until it
/// returns, and the service closes when it is dropped then: its idle
/// producer handle's kept ordinals are returned here, and only a handle
/// that running call gives back later is dropped without that release.
pub(crate) fn release(args: &OpenArgs, owner: Route) -> Result<(), SdkRefusal> {
    let store = args.store_path()?;
    let pending = Arc::new(Completion::new());
    let opened = {
        let mut stores = lock(&STORES)?;
        let Some(Slot::Ready(opened)) = stores.get_mut(&store) else {
            return Err(refused("InvalidInput", "this process holds no such Store"));
        };
        owner.check(opened)?;
        opened.clients -= 1;
        if opened.clients > 0 {
            return Ok(());
        }
        let Some(Slot::Ready(opened)) =
            stores.insert(store.clone(), Slot::Closing(pending.clone()))
        else {
            unreachable!("ready while locked");
        };
        opened
    };
    transition(
        || close_service(opened),
        |result| {
            lock(&STORES)?.remove(&store);
            pending.finish(result)
        },
    )
}

fn close_service(opened: Opened) -> Result<(), SdkRefusal> {
    let closed = match Arc::try_unwrap(opened.service) {
        Ok(service) => service.close().map_err(failed),
        // A call still running holds the service; its idle producer handle
        // returns its kept ordinals now (#768). A handle that call hands
        // back after this is dropped with the service, unreleased.
        Err(service) if opened.owner == Route::Direct => {
            service.store.release_producer();
            Ok(())
        }
        Err(_) => Err(refused(
            "InUse",
            "a host call still holds the Store service",
        )),
    };
    let removed = std::fs::remove_dir_all(&opened.directory).map_err(failed);
    closed.and(removed)
}

/// The open service of the Store a call names.
pub(crate) fn service(args: &OpenArgs, owner: Route) -> Result<Arc<StoreService>, SdkRefusal> {
    let store = args.store_path()?;
    let stores = lock(&STORES)?;
    let Some(Slot::Ready(opened)) = stores.get(&store) else {
        return Err(refused(
            "InvalidInput",
            "the client has not opened its Store",
        ));
    };
    owner.check(opened)?;
    Ok(opened.service.clone())
}

/// Runs `work` on the Store a call names, opened for it when this process
/// holds it not yet and closed after when no other client holds it (#506
/// Store-level calls: nothing stays open for a pull to find).
pub fn with_service<T>(
    args: &OpenArgs,
    work: impl FnOnce(&Arc<StoreService>) -> Result<T, SdkRefusal>,
) -> Result<T, SdkRefusal> {
    acquire(args, Route::Direct)?;
    transition(
        || service(args, Route::Direct).and_then(|service| work(&service)),
        |result| {
            let released = release(args, Route::Direct);
            let value = result?;
            released?;
            Ok(value)
        },
    )
}

/// Runs one Store call in this process on the Store `open` names, with
/// the call's own arguments `args` (any routing key in them is dropped).
pub fn call(method: &str, open: &OpenArgs, args: SdkValue) -> Outcome {
    call_owned(method, open, args, Route::Direct)
}

pub(crate) fn call_owned(method: &str, open: &OpenArgs, args: SdkValue, owner: Route) -> Outcome {
    let service = service(open, owner)?;
    dispatch(&service, method, args)
}

/// [`call_owned`] with the write's content or changes pulled from `source`.
pub(crate) fn call_streamed(
    method: &str,
    open: &OpenArgs,
    args: SdkValue,
    owner: Route,
    source: &mut dyn layerfs_store_service::methods::Source,
) -> Outcome {
    let service = service(open, owner)?;
    dispatch_streamed(&service, method, args, source)
}

/// Runs write `method` once on `service`, its content or changes pulled
/// from `source` (a host worker's held PostgreSQL write session).
pub(crate) fn dispatch_streamed(
    service: &StoreService,
    method: &str,
    args: SdkValue,
    source: &mut dyn layerfs_store_service::methods::Source,
) -> Outcome {
    let call = SdkCall {
        method: method.to_owned(),
        args: routed_out(args),
    };
    layerfs_store_service::methods::call_streamed(service, &call, source)
}

fn routed_out(args: SdkValue) -> SdkValue {
    match args {
        SdkValue::Map(fields) => SdkValue::Map(
            fields
                .into_iter()
                .filter(|(name, _)| !ROUTING.contains(&name.as_str()))
                .collect(),
        ),
        other => other,
    }
}

pub(crate) fn dispatch(service: &StoreService, method: &str, args: SdkValue) -> Outcome {
    let call = SdkCall {
        method: method.to_owned(),
        args: routed_out(args),
    };
    layerfs_store_service::methods::call(service, &call)
}
