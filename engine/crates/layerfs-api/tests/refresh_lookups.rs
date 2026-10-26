//! Kernel lookups under `node_modules` on a replay Store (#364 item 3).
//!
//! Set `LAYERFS_364_RELINK_STORE`, `_BINDING` and `_FROM` as for
//! `relink_commit`, then run with `--ignored`. The Store is copied first. A
//! private Workspace on a Branch forked from that Commit gets a kernel mount,
//! and every name under a `node_modules` directory (and every directory on
//! the way), in walk order, is looked up as the FUSE layer does: an owner visit, the base facts it names read
//! outside the owner, and the visit again, until it decides. The names are
//! listed first through the SDK view, which also fills the Store's shared
//! object cache, as earlier mounts of a long-lived daemon do. Prints one
//! `REFRESH_LOOKUPS` line and one `REFRESH_CACHE` line: the shared cache's
//! counters before and after the lookups. `LAYERFS_364_LOOKUPS` bounds the
//! lookups (6,000).
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;

use layerfs_bridge::control::{SdkCall, SdkValue};
use layerfs_content::filesystem::PathName;
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_history::WorkspaceId;
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_store_service::write::mutation::Mutation;
use layerfs_store_service::{bootstrap::open_store_observed, Owner, OwnerConfig, StoreService};
use layerfs_workspace::{KernelReadDecision, KernelReadOperation, VisitFacts};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

fn call(service: &StoreService, method: &str, fields: Vec<(&str, SdkValue)>) -> SdkValue {
    let mut fields: Vec<(String, SdkValue)> = fields
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect();
    fields.push(("project".into(), SdkValue::text("dsh")));
    let call = SdkCall {
        method: method.into(),
        args: SdkValue::Map(fields),
    };
    layerfs_api::call(service, &call).unwrap()
}

/// (parent serial, name) of every entry under a `node_modules` directory and
/// of every directory on the way, in depth-first walk order, up to `limit`.
fn names(m: &Mutation<'_>, limit: usize) -> Vec<(u64, PathName)> {
    let mut out = Vec::new();
    let mut stack = vec![(m.resolve("/").unwrap().serial, false)];
    while let Some((serial, inside)) = stack.pop() {
        let mut after: Option<Vec<u8>> = None;
        loop {
            let page = m
                .view
                .list(m.op.overlay(), serial, after.as_deref())
                .unwrap();
            for (name, child) in page.entries {
                let text = String::from_utf8(name).unwrap();
                if text == ".git" {
                    continue;
                }
                let below = inside || text == "node_modules";
                let directory =
                    m.view.stat(m.op.overlay(), child).unwrap().kind == InodeKind::Directory;
                // The kernel looks a parent up before any name in it.
                if inside || directory {
                    out.push((serial, PathName::new(&text).unwrap()));
                    if out.len() == limit {
                        return out;
                    }
                }
                if directory {
                    stack.push((child, below));
                }
            }
            after = page.continuation;
            if after.is_none() {
                break;
            }
        }
    }
    out
}

#[test]
#[ignore = "needs a replay Store: LAYERFS_364_RELINK_STORE, _BINDING and _FROM"]
fn refresh_lookups() {
    let source = std::env::var("LAYERFS_364_RELINK_STORE").expect("replay Store");
    let binding = unhex(&std::env::var("LAYERFS_364_RELINK_BINDING").expect("binding key"));
    let from = std::env::var("LAYERFS_364_RELINK_FROM").expect("Commit to start from");
    let limit = std::env::var("LAYERFS_364_LOOKUPS").map_or(6_000, |n| n.parse().unwrap());
    let directory =
        std::env::temp_dir().join(format!("layerfs-364-lookups-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("store.sqlite");
    std::fs::copy(&source, &path).unwrap();
    // A Store kept by `relink_commit` may hold its last Commits in the WAL.
    let wal = std::path::Path::new(&source).with_extension("sqlite-wal");
    if wal.exists() {
        std::fs::copy(&wal, path.with_extension("sqlite-wal")).unwrap();
    }
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    std::fs::set_permissions(&path, permissions).unwrap();
    let open = |overlay: &str| {
        let opened = open_store_observed(
            PersistenceConfig::sqlite(&path)
                .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
            &binding,
            installed::CURSOR,
            OwnerConfig::default().namespaces,
            // The daemon's shared object cache (`install_types.rs`).
            64 * 1024 * 1024,
            layerfs_storage::ReservationBlocks::default(),
        )
        .unwrap();
        // Each owner's overlays live in their own folder.
        let folder = directory.join(overlay);
        std::fs::create_dir(&folder).unwrap();
        let owner = Owner::start(
            &folder.join("overlay.sqlite"),
            layerfs_overlay::ProfileConfig::default(),
            OwnerConfig::default(),
        )
        .unwrap();
        (opened, owner)
    };
    let bind = |service: &StoreService, id: u8| {
        let store = &service.store;
        let stack = layerfs_store_service::history::lookup::project(store, "dsh").unwrap();
        let branch =
            layerfs_store_service::history::lookup::branch(store, &stack, "refresh").unwrap();
        layerfs_store_service::telemetry::timed("bind", |timing| {
            service.bind(
                WorkspaceId::from_authority([id; 32]).unwrap(),
                branch.id,
                timing,
            )
        })
        .unwrap()
    };
    // The names, through the SDK view; this fills the Store's object cache.
    let (opened, owner) = open("names");
    let service = StoreService::new(opened, &owner);
    call(
        &service,
        "branch.fork",
        vec![
            ("name", SdkValue::text("refresh")),
            ("from", SdkValue::text(&from)),
        ],
    );
    let workspace = bind(&service, 65);
    let (list, root) = {
        let m = Mutation::new(&workspace).unwrap();
        let root = m.resolve("/").unwrap().serial;
        (names(&m, limit), root)
    };
    // `LAYERFS_364_COLD` reopens the Store: an empty object cache, as on a
    // daemon's first mount of this tree.
    let cold = std::env::var_os("LAYERFS_364_COLD").is_some();
    let (service, workspace, owner) = if cold {
        drop(workspace);
        drop(service);
        owner.stop().unwrap();
        let (opened, owner) = open("lookups");
        let service = StoreService::new(opened, &owner);
        let workspace = bind(&service, 66);
        (service, workspace, owner)
    } else {
        (service, workspace, owner)
    };
    let op = workspace.operation().unwrap();
    let route = workspace.route();
    let mount = op.overlay().kernel_mount(route, root).unwrap();
    let (mut visits, mut supplies, mut missing) = (0_u64, 0_u64, 0_u64);
    let (mut visit_time, mut supply_time) = (Duration::ZERO, Duration::ZERO);
    let cache = service.store.cache_work().unwrap();
    let started = Instant::now();
    for (parent, name) in &list {
        let mut facts = Arc::new(VisitFacts::default());
        loop {
            let at = Instant::now();
            let visit = op
                .workspace()
                .kernel_read_visit(
                    op.resident(),
                    mount,
                    *parent,
                    None,
                    KernelReadOperation::Lookup {
                        parent: *parent,
                        name: name.clone(),
                    },
                    facts.clone(),
                )
                .unwrap();
            let outcome = op
                .overlay()
                .kernel_observe_visit(route, Box::new(visit))
                .unwrap();
            visit_time += at.elapsed();
            visits += 1;
            match &outcome.decision {
                Some(KernelReadDecision::Value(_)) => break,
                Some(KernelReadDecision::Refused(_)) => {
                    missing += 1;
                    break;
                }
                Some(KernelReadDecision::Needs(needs)) => {
                    let at = Instant::now();
                    let base = op.workspace().base().unwrap();
                    Arc::make_mut(&mut facts)
                        .supply(&base, needs, None)
                        .unwrap();
                    supply_time += at.elapsed();
                    supplies += 1;
                }
                other => panic!("undecided visit: {other:?} {:?}", outcome.result),
            }
        }
    }
    let ms = started.elapsed().as_secs_f64() * 1e3;
    println!(
        "REFRESH_LOOKUPS cold={cold} lookups={} ms={ms:.1} us_per_lookup={:.1} visits={visits} visit_ms={:.1} supplies={supplies} supply_ms={:.1} missing={missing}",
        list.len(),
        ms * 1e3 / list.len() as f64,
        visit_time.as_secs_f64() * 1e3,
        supply_time.as_secs_f64() * 1e3,
    );
    println!(
        "REFRESH_CACHE before={cache:?} after={:?}",
        service.store.cache_work().unwrap()
    );
    drop(op);
    drop(workspace);
    drop(service);
    owner.stop().unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}
