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
use layerfs_bridge::sdk_values::hex;
use layerfs_history::MountHolderView;
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
    let result = service(args).and_then(|service| {
        let place = match layerfs_store_service::history::admission::has_place_view() {
            true => None,
            false => crate::place_view::from_open(args),
        };
        layerfs_store_service::history::admission::with_place_view(place, || work(&service))
    });
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

/// Resolves Branch `branch` of `project` in the Store `open` names, then runs
/// [`repair_mount_holders`] on it: the one implementation behind a local call
/// and a worker's `store.repair` operation.
/// The operator options travel as the call's own fields; keeping them
/// explicit is the call surface, not a bundle of unrelated state.
#[allow(clippy::too_many_arguments)]
pub fn repair_mount_holders_at(
    open: &OpenArgs,
    project: &str,
    branch: &str,
    action: &str,
    op: Option<[u8; 16]>,
    legacy: bool,
    operator: &str,
    page: u32,
    limit: u32,
) -> Result<SdkValue, SdkRefusal> {
    if action == "audit" {
        return done_audit(open, project, branch, page, limit);
    }
    let resolved = branch_named(open, project, branch)?;
    repair_mount_holders(
        open, resolved, project, branch, action, op, legacy, operator, page, limit,
    )
}

/// One bounded page of the retained `done` audit of one Branch, oldest
/// first. The live holder queries filter done rows; this is the only repair
/// action that reads them, and it reads only the Branch it names. It changes
/// no intent.
pub fn done_audit(
    args: &OpenArgs,
    project: &str,
    branch: &str,
    page: u32,
    limit: u32,
) -> Result<SdkValue, SdkRefusal> {
    with_service(args, |service| {
        let page = layerfs_store_service::history::admission::done_intents_branch(
            service.store.history(),
            project,
            branch,
            page,
            limit,
        )
        .map_err(failed)?;
        Ok(layerfs_store_service::history::admission::audit_value(
            &page,
        ))
    })
}

/// First-class operator repair of the mount intents held at one Branch.
/// `preview` lists them; `settle`/`release` act on one explicit `op`;
/// `settle_legacy` marks only rows without owner or place evidence. The call
/// runs through the Store catalog, so a PostgreSQL Store and a worker-hosted
/// SQLite Store behave the same way. Never automatic; a live-looking holder
/// is never released by preview.
///
/// Release retains the `gc_intent` row in state `done` with the action,
/// operator label and op in `evidence`, and clears the owner columns. Preview
/// writes its own retained `done` row the same way, with `action=preview`,
/// the operator label and no operation; it changes no live intent. Settle
/// changes the row to uncertain and is in the home's `trace/calls.jsonl` call
/// log only. The `audit` action reads the Branch's retained done rows in
/// bounded pages.
#[allow(clippy::too_many_arguments)]
pub fn repair_mount_holders(
    args: &OpenArgs,
    branch: layerfs_history::BranchId,
    project: &str,
    branch_name: &str,
    action: &str,
    op: Option<[u8; 16]>,
    legacy: bool,
    operator: &str,
    page: u32,
    limit: u32,
) -> Result<SdkValue, SdkRefusal> {
    with_service(args, |service| {
        let catalog = service.store.history();
        let scoped = catalog
            .gc_intents(layerfs_history::GcScopeKind::Branch, branch.as_slice())
            .map_err(failed)?;
        let mounts: Vec<layerfs_history::GcIntent> = scoped
            .into_iter()
            .filter(|intent| intent.kind == layerfs_history::GcIntentKind::Mount)
            .collect();
        // The typed, read-only view every branch below reports: it carries
        // the place and lease authority the row recorded, and building it
        // changes nothing.
        let holders: Vec<MountHolderView> = mounts.iter().map(MountHolderView::of).collect();
        let holder = |op: &[u8; 16]| mounts.iter().find(|intent| &intent.op_id == op);
        match action {
            "list" => Ok(SdkValue::map([
                ("holders", holder_rows(&holders)),
                ("legacy", SdkValue::Bool(legacy)),
                ("action", SdkValue::text("list")),
                ("op", SdkValue::Null),
                ("audit", SdkValue::Null),
            ])),
            "preview" => {
                // Preview changes no live intent, but the operator action is
                // durable: it writes a retained `done` row through the same
                // release/done columns, with the operator label and no
                // operation (`evidence` has no `op=`). The row never admits
                // (admission filters `state <> 'done'`) and the bounded
                // `gc_intent_done` read reports it.
                let operator = operator_label(operator)?;
                let mut op = [0u8; 16];
                op.copy_from_slice(&layerfs_home::helpers::random()?[..16]);
                let evidence = format!("action=preview;operator={operator}");
                let intent = layerfs_history::GcIntent::shared(
                    op,
                    layerfs_history::GcIntentKind::Mount,
                    layerfs_history::GcScopeKind::Branch,
                    branch.as_slice(),
                )
                .named(Some(project), Some(branch_name));
                catalog.gc_record_done(&intent, &evidence).map_err(failed)?;
                Ok(SdkValue::map([
                    ("holders", holder_rows(&holders)),
                    ("legacy", SdkValue::Bool(legacy)),
                    ("action", SdkValue::text("preview")),
                    ("op", SdkValue::Null),
                    ("audit", SdkValue::text("gc_intent")),
                ]))
            }
            "settle" | "release" => {
                let op = op.ok_or_else(|| {
                    refused("InvalidInput", format!("{action} needs the holder op"))
                })?;
                let found = holder(&op).ok_or_else(|| {
                    refused(
                        "NotFound",
                        format!("no mount holder {} at this Branch", hex(&op)),
                    )
                })?;
                if legacy && !MountHolderView::of(found).legacy() {
                    return Err(refused(
                        "InvalidInput",
                        "this holder has owner or place evidence; --legacy settles only legacy rows",
                    ));
                }
                if action == "settle" {
                    catalog.gc_mark_uncertain(&op).map_err(failed)?;
                    return Ok(SdkValue::map([
                        ("action", SdkValue::text(action)),
                        ("op", SdkValue::text(hex(&op))),
                        ("audit", SdkValue::text("call-log")),
                    ]));
                }
                if !found.uncertain {
                    return Err(refused(
                        "Conflict",
                        "settle the holder before release; preview and settle are the two explicit steps",
                    ));
                }
                // The operator audit is the retained row itself: release
                // keeps it in `done` with the action, operator and op in
                // `evidence`, and clears the owner columns. The call log
                // still records the call.
                let operator = operator_label(operator)?;
                let evidence = format!("action=release;operator={operator};op={}", hex(&op));
                if !catalog.gc_release_done(&op, &evidence).map_err(failed)? {
                    return Err(refused(
                        "NotFound",
                        format!("the mount holder {} was already released", hex(&op)),
                    ));
                }
                Ok(SdkValue::map([
                    ("action", SdkValue::text("release")),
                    ("op", SdkValue::text(hex(&op))),
                    ("audit", SdkValue::text("gc_intent")),
                ]))
            }
            "audit" => {
                let page = layerfs_store_service::history::admission::done_intents_branch(
                    catalog,
                    project,
                    branch_name,
                    page,
                    limit,
                )
                .map_err(failed)?;
                Ok(layerfs_store_service::history::admission::audit_value(
                    &page,
                ))
            }
            "settle_legacy" => {
                let mut settled = 0u64;
                for holder in &holders {
                    if holder.legacy() {
                        catalog.gc_mark_uncertain(&holder.op).map_err(failed)?;
                        settled += 1;
                    }
                }
                Ok(SdkValue::map([
                    ("action", SdkValue::text("settle_legacy")),
                    ("settled", SdkValue::Int(settled as i64)),
                    ("audit", SdkValue::text("call-log")),
                ]))
            }
            _ => Err(refused(
                "InvalidInput",
                "action must be preview, settle, release, settle_legacy or audit",
            )),
        }
    })
}

/// The operator label a release records. An absent label is the local
/// operator; a label is at most 128 characters without control characters.
/// It is not an authenticated principal, only the name the caller gives.
fn operator_label(operator: &str) -> Result<&str, SdkRefusal> {
    let operator = operator.trim();
    if operator.is_empty() {
        return Ok("local");
    }
    if operator.len() > 128 || operator.chars().any(char::is_control) {
        return Err(refused(
            "InvalidInput",
            "operator must be at most 128 characters without control characters",
        ));
    }
    Ok(operator)
}

fn holder_rows(holders: &[MountHolderView]) -> SdkValue {
    SdkValue::List(
        holders
            .iter()
            .map(|holder| {
                let workspace = holder.workspace.map(|ws| hex(&ws));
                SdkValue::map([
                    ("op", SdkValue::text(hex(&holder.op))),
                    ("branch", SdkValue::opt_text(holder.branch.as_deref())),
                    ("project", SdkValue::opt_text(holder.project.as_deref())),
                    ("workspace", SdkValue::opt_text(workspace.as_deref())),
                    ("sandbox", SdkValue::opt_text(holder.sandbox.as_deref())),
                    ("machine", SdkValue::opt_text(holder.machine.as_deref())),
                    (
                        "owner_runtime",
                        SdkValue::opt_text(holder.owner_runtime.as_deref()),
                    ),
                    (
                        "owner_boot",
                        SdkValue::opt_text(holder.owner_boot.as_deref()),
                    ),
                    (
                        "machine_incarnation",
                        SdkValue::opt_text(holder.machine_incarnation.as_deref()),
                    ),
                    (
                        "sandbox_generation",
                        SdkValue::opt_text(holder.sandbox_generation.as_deref()),
                    ),
                    ("state", SdkValue::text(holder.state())),
                    ("legacy", SdkValue::Bool(holder.legacy())),
                    ("safe", SdkValue::Bool(holder.uncertain)),
                    (
                        "reason",
                        SdkValue::text(if holder.uncertain {
                            "uncertain: the owner is already settled; release is available"
                        } else {
                            "live: the owner still holds the admission; settle it first"
                        }),
                    ),
                ])
            })
            .collect(),
    )
}
