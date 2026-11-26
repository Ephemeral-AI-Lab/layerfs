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
use layerfs_home::outcome::{failed, refused, with_fix, Outcome};
use layerfs_store_service::{OpenRefusal, StoreService};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

/// The entry points allowed to borrow the service. A host call's outer
/// cache slot accounts for every host borrower until its reply is ready.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum Owner {
    Direct,
    Host,
}

impl Owner {
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
    owner: Owner,
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
fn open(store: &Path, args: &OpenArgs, owner: Owner) -> Result<Opened, SdkRefusal> {
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
                    OpenRefusal::Outdated(status) => refused(crate::state::OLD_STORE, status),
                    OpenRefusal::Failed(reason) => refused("Failed", reason),
                })
        }
        crate::identity::Kind::Postgresql { .. } => {
            crate::postgresql::open::open_postgres(args.postgresql.as_ref(), &identity, &template)
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

fn with_upgrade_fix(
    args: &OpenArgs,
    identity: &crate::identity::Identity,
    refusal: SdkRefusal,
) -> SdkRefusal {
    match (&args.store, refusal.code.as_str()) {
        (Some(store), crate::state::OLD_STORE) => {
            let postgresql = matches!(identity.kind, crate::identity::Kind::Postgresql { .. });
            let fix = crate::state::outdated_fix(store, postgresql, &refusal.message);
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
    let identity = crate::identity::identity(args)?;
    crate::postgresql::open::open_read_only(args.postgresql.as_ref(), &identity)
        .map_err(|refusal| with_upgrade_fix(args, &identity, refusal))
}

/// One more client of the Store a call names, opening it first.
pub(crate) fn acquire(args: &OpenArgs, owner: Owner) -> Result<(), SdkRefusal> {
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
pub(crate) fn release(args: &OpenArgs, owner: Owner) -> Result<(), SdkRefusal> {
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
        Err(service) if opened.owner == Owner::Direct => {
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
fn service(args: &OpenArgs, owner: Owner) -> Result<Arc<StoreService>, SdkRefusal> {
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
    acquire(args, Owner::Direct)?;
    transition(
        || service(args, Owner::Direct).and_then(|service| work(&service)),
        |result| {
            let released = release(args, Owner::Direct);
            let value = result?;
            released?;
            Ok(value)
        },
    )
}

/// Runs one Store call in this process on the Store `open` names, with
/// the call's own arguments `args` (any routing key in them is dropped).
pub fn call(method: &str, open: &OpenArgs, args: SdkValue) -> Outcome {
    call_owned(method, open, args, Owner::Direct)
}

pub(crate) fn call_owned(method: &str, open: &OpenArgs, args: SdkValue, owner: Owner) -> Outcome {
    let service = service(open, owner)?;
    dispatch(&service, method, args)
}

/// [`call_owned`] with the write's content or changes pulled from `source`.
pub(crate) fn call_streamed(
    method: &str,
    open: &OpenArgs,
    args: SdkValue,
    owner: Owner,
    source: &mut dyn layerfs_store_service::methods::Source,
) -> Outcome {
    let service = service(open, owner)?;
    let call = SdkCall {
        method: method.to_owned(),
        args: routed_out(args),
    };
    layerfs_store_service::methods::call_streamed(&service, &call, source)
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

/// The newest Commit of `branch`, read in this process; `None` when it has none.
pub fn head(
    args: &OpenArgs,
    branch: layerfs_history::BranchId,
) -> Result<Option<layerfs_history::CommitId>, SdkRefusal> {
    if args.postgresql.is_some() {
        return crate::postgresql::reader::head(args, branch);
    }
    let service = service(args, Owner::Direct)?;
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

/// The id of Branch `name` of project `project` in the Store `args` names,
/// read with `project.list_branches`' exact name lookup.
pub fn branch_named(
    args: &OpenArgs,
    project: &str,
    name: &str,
) -> Result<layerfs_history::BranchId, SdkRefusal> {
    let asked = SdkValue::map([
        ("project", SdkValue::text(project)),
        ("name", SdkValue::text(name)),
    ]);
    let listed = with_service(args, |_| call("project.list_branches", args, asked))?;
    let id = match listed.get("items") {
        Some(SdkValue::List(items)) => items.first().and_then(|item| item.get_text("id")),
        _ => None,
    };
    id.and_then(layerfs_bridge::sdk_values::unhex)
        .and_then(|bytes| layerfs_history::BranchId::from_slice(&bytes).ok())
        .ok_or_else(|| refused("NoSuchBranch", format!("no Branch {name} in {project}")))
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

/// Settles the intents at `machine` the ordered `incarnation` proves dead,
/// where that Store's calls run: this process when the Store lives here, else
/// the machine's worker. The home recorded the incarnation in the
/// `MachineRecord` before this call. One bounded call per Store; a failure is
/// reported, never retried.
pub fn reconcile_machine(
    home: &Path,
    store: &str,
    machine: &str,
    incarnation: &str,
) -> Result<SdkValue, SdkRefusal> {
    // The host holds the Store's lease while it sends the call, as
    // `store_call` does; a Store an operation is moving is left, never retried.
    let (_lease, record) = crate::lease::claim(home, store)?;
    let store_ref = record.store_ref()?;
    match crate::place::plan(home, store_ref)? {
        crate::place::Executor::Local => {
            let args = OpenArgs::of_record(home, store, &record)?;
            // the reconcile compares placed mount intents: it reads the view
            crate::place_view::scoped(&args, || {
                with_service(&args, |service| {
                    layerfs_store_service::history::admission::reconcile_machine(
                        service.store.history(),
                        machine,
                        incarnation,
                    )
                    .map(|reconcile| {
                        layerfs_store_service::history::admission::reconcile_value(&reconcile)
                    })
                    .map_err(layerfs_store_service::history::lookup::history)
                })
            })
        }
        crate::place::Executor::Worker(mut worker) => {
            let mut fields = vec![
                ("store".to_owned(), SdkValue::text(store)),
                ("store_record".to_owned(), record.to_record().clone()),
                ("machine".to_owned(), SdkValue::text(machine)),
                ("incarnation".to_owned(), SdkValue::text(incarnation)),
            ];
            if let Some(place) = crate::place_view::encoded_from_home(home) {
                fields.push(("place_view".to_owned(), place));
            }
            let request = SdkValue::Map(fields);
            worker.call("store.reconcile", request, &mut |_: SdkValue| {})
        }
    }
}
