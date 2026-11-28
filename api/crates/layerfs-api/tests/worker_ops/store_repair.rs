//! Operator repair of a Branch's mount holders on the Store a Docker worker
//! holds: preview, settle, release and `settle_legacy` run in that worker,
//! the preview answers the typed holder view, and the worker's read-only
//! diagnostic reports the same state without settling anything. The lean
//! schema keeps no Store-side audit table; release retains the done row and
//! the bounded audit read reports it.
use crate::worker_store;

use layerfs_history::records::{GcIntent, GcIntentKind, GcScopeKind};
use layerfs_history::{HistoryName, NamedSnapshot, PlaceAuthority};
use layerfs_home::names::StoreName;
use layerfs_machine::route;
use layerfs_protocol::sdk::hex;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::admin::open::args::OpenArgs;
use layerfs_store::admin::open::service::with_service;
use layerfs_store::admin::record::store::StoreRecord;
use layerfs_store::admin::with_users::RepairRequest;
use layerfs_store::data::branch::branch_named;
use layerfs_store::data::project::init::sqlite::{add_project, ProjectAdd};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A fresh folder to initialize the project from, on the test's own disk so
/// the kernel guard admits its Store.
fn source(tag: &str) -> PathBuf {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "layerfs-repair-worker-src-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("a.txt"), b"a\n").unwrap();
    directory
}

fn mount(stack: &[u8], branch: &[u8], op: u8) -> GcIntent {
    GcIntent::shared(
        [op; 16],
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.to_vec(),
    )
    .in_project(stack.to_vec())
    .named(Some("app"), Some("main"))
}

/// A project `app` in Store `shop` and two mount intents at its `main`:
/// `0x21` with owner and place evidence (live), `0x31` with none (legacy).
fn seed(home: &Path) {
    let shop = StoreName::parse("shop").unwrap();
    add_project(&ProjectAdd {
        home: home.to_path_buf(),
        store: shop,
        name: "app".to_owned(),
        source: source("seed"),
        timeout_ms: Some(60_000),
        store_record: None,
    })
    .unwrap();
    let record = StoreRecord::read(home, "shop").unwrap().unwrap();
    let open = OpenArgs::of_record(home, "shop", &record).unwrap();
    let branch = branch_named(&open, "app", "main").expect("the main Branch");
    // `main` is the project's line, not a Branch the public by-name rule
    // names (spec 4.4), yet `add_project` records a Branch `main` for the
    // stack and the operator repair route resolves the mount scope by that
    // name. Hold the lookup to the recorded row: a listing that drops main's
    // id fails here, beside the mount scope it would otherwise register at,
    // instead of at the first repair.
    let recorded = with_service(&open, |service| {
        let project = HistoryName::new("app").unwrap();
        let main = HistoryName::new("main").unwrap();
        Ok(
            match service
                .store
                .published(|history| history.branch_snapshot_named(&project, &main))
                .unwrap()
            {
                NamedSnapshot::Found(snapshot) => snapshot.branch.id,
                other => panic!("no recorded main Branch: {other:?}"),
            },
        )
    })
    .unwrap();
    assert_eq!(
        branch, recorded,
        "branch.list names the main Branch add_project recorded"
    );
    let stack = with_service(&open, |service| {
        Ok(layerfs_store_service::lookup::project(&service.store, "app")?.id)
    })
    .unwrap();
    let live = mount(stack.as_slice(), branch.as_slice(), 0x21)
        .placed(Some([0x22; 32]), Some("box"), Some("box"))
        .owned(Some("pid:1@0"), Some("boot:1"))
        .authorized(PlaceAuthority {
            machine_incarnation: Some("worker:1".to_owned()),
            sandbox_generation: Some("generation:1".to_owned()),
        });
    let legacy = mount(stack.as_slice(), branch.as_slice(), 0x31);
    with_service(&open, |service| {
        service.store.history().gc_register(&live).unwrap();
        service.store.history().gc_register(&legacy).unwrap();
        Ok(())
    })
    .unwrap();
}

fn serve(listener: std::net::TcpListener, root: PathBuf) {
    std::thread::spawn(move || {
        layerfs_api::worker::serve::serve(listener, worker_store::SECRET.to_owned(), root)
    });
}

fn request(action: &str, op: Option<u8>, legacy: bool) -> RepairRequest {
    RepairRequest {
        project: "shop/app".to_owned(),
        branch: "main".to_owned(),
        action: action.to_owned(),
        op: op.map(|op| hex(&[op; 16])),
        legacy,
        force: false,
        operator: Some("alice".to_owned()),
        limit: None,
        cursor: None,
    }
}

fn repair(placed: &worker_store::Placed, action: &str, op: Option<u8>, legacy: bool) -> SdkValue {
    layerfs_store::admin::with_users::repair(&placed.home, &request(action, op, legacy))
        .unwrap_or_else(|refusal| panic!("{action}: {refusal:?}"))
}

fn holders(value: &SdkValue) -> &[SdkValue] {
    match value.get("items") {
        Some(SdkValue::List(holders)) => holders,
        other => panic!("no holder list: {other:?}"),
    }
}

/// The worker's read-only diagnostic: `store.diagnose`, with the typed
/// `store.mount_holders` event it forwards from the machine.
fn diagnostic_holders(placed: &worker_store::Placed) -> Vec<SdkValue> {
    let record = StoreRecord::read(&placed.home, "shop").unwrap().unwrap();
    let bytes = |key: &str| match record.field(key) {
        Some(SdkValue::Bytes(bytes)) => SdkValue::Bytes(bytes.clone()),
        _ => panic!("the record holds {key} bytes"),
    };
    let value = route::call(
        &placed.endpoint,
        "store.diagnose",
        SdkValue::map([
            ("store", SdkValue::text("shop")),
            ("binding", bytes("binding")),
            ("cursor", bytes("cursor")),
            ("cut", SdkValue::text("schema")),
        ]),
        &mut |_event| {},
    )
    .expect("the worker diagnostics run");
    match value.get("mount_holders") {
        Some(SdkValue::List(holders)) => holders.clone(),
        other => panic!("no typed mount holders: {other:?}"),
    }
}

#[test]
fn repair_previews_settles_and_releases_through_the_worker() {
    let placed = worker_store::placed_seeded("repair", seed, serve);

    // The typed read-only view over the live worker path: owner, place and
    // state, and it settles nothing.
    let diagnosed = diagnostic_holders(&placed);
    assert_eq!(diagnosed.len(), 2, "{diagnosed:?}");
    let live = diagnosed
        .iter()
        .find(|row| row.get_text("operation") == Some("21".repeat(16).as_str()))
        .expect("the live holder");
    assert_eq!(live.get_text("owner_runtime"), Some("pid:1@0"));
    assert_eq!(live.get_text("machine_incarnation"), Some("worker:1"));
    assert_eq!(live.get_text("state"), Some("live"));

    // Preview over the worker answers the typed view and writes nothing.
    let preview = repair(&placed, "preview", None, false);
    assert_eq!(preview.get_text("action"), Some("preview"), "{preview:?}");
    let audit = repair(&placed, "audit", None, false);
    assert!(
        matches!(audit.get("items"), Some(SdkValue::List(rows)) if rows.is_empty()),
        "preview wrote a row: {audit:?}"
    );
    let rows = holders(&preview);
    assert_eq!(rows.len(), 2);
    let live = rows
        .iter()
        .find(|row| row.get_text("operation") == Some("21".repeat(16).as_str()))
        .expect("the live holder");
    assert_eq!(live.get_text("status"), Some("live"));
    // nothing proves its owner gone: it still looks live, so not safe
    assert_eq!(live.get_text("liveness"), Some("live"));
    assert_eq!(live.get("safe"), Some(&SdkValue::Bool(false)));
    assert_eq!(live.get_text("sandbox"), Some("box"));
    assert_eq!(live.get_text("machine_incarnation"), Some("worker:1"));
    assert_eq!(live.get_text("sandbox_generation"), Some("generation:1"));
    assert_eq!(live.get("legacy"), Some(&SdkValue::Bool(false)));
    let legacy = rows
        .iter()
        .find(|row| row.get_text("operation") == Some("31".repeat(16).as_str()))
        .expect("the legacy holder");
    assert_eq!(legacy.get("legacy"), Some(&SdkValue::Bool(true)));

    // Release without settle is refused; a settle of a holder that still
    // looks live is refused `InUse` unless forced, and marks the one op.
    let refused = layerfs_store::admin::with_users::repair(
        &placed.home,
        &request("release", Some(0x21), false),
    )
    .unwrap_err();
    assert_eq!(refused.code, "Conflict", "{refused:?}");
    let refused = layerfs_store::admin::with_users::repair(
        &placed.home,
        &request("settle", Some(0x21), false),
    )
    .unwrap_err();
    assert_eq!(refused.code, "InUse", "{refused:?}");
    assert!(
        refused
            .detail
            .get_text("fix")
            .is_some_and(|fix| fix.ends_with("--force")),
        "{refused:?}"
    );
    let forced = RepairRequest {
        force: true,
        ..request("settle", Some(0x21), false)
    };
    layerfs_store::admin::with_users::repair(&placed.home, &forced).expect("a forced settle");
    let preview = repair(&placed, "preview", None, false);
    let settled = holders(&preview)
        .iter()
        .find(|row| row.get_text("operation") == Some("21".repeat(16).as_str()))
        .expect("the settled holder");
    assert_eq!(settled.get_text("status"), Some("uncertain"));

    repair(&placed, "release", Some(0x21), false);
    let preview = repair(&placed, "preview", None, false);
    assert_eq!(holders(&preview).len(), 1, "the released holder is gone");

    // `settle_legacy` marks only the row with no owner or place evidence.
    let settled = repair(&placed, "settle_legacy", None, false);
    assert_eq!(settled.get_int("settled"), Some(1));
    repair(&placed, "release", Some(0x31), false);

    let preview = repair(&placed, "preview", None, false);
    assert_eq!(holders(&preview).len(), 0, "no holder remains");

    // Both releases retained their row on the worker host, and the same
    // repair route answers the bounded audit.
    let audit = repair(&placed, "audit", None, false);
    let Some(SdkValue::List(rows)) = audit.get("items") else {
        panic!("audit items: {audit:?}");
    };
    let releases: Vec<_> = rows
        .iter()
        .filter(|row| {
            row.get_text("evidence")
                .is_some_and(|evidence| evidence.starts_with("action=release;"))
        })
        .collect();
    assert_eq!(releases.len(), 2, "both releases: {audit:?}");
    for row in &releases {
        let evidence = row.get_text("evidence").expect("evidence");
        assert!(evidence.contains("operator=alice"), "{row:?}");
    }
    assert_eq!(rows.len(), 2, "no worker preview wrote a row: {audit:?}");
    placed.remove();
}

/// The worker judges with the view this home sends, not its own: once the
/// host home records the tombstone of the live holder's Sandbox generation
/// (and no Sandbox record of that name), the worker's preview calls the
/// holder `dead` and a `settle` without `--force` succeeds, where it was
/// `live` and refused `InUse` before. The audit cursor crosses to the worker
/// and back as the last row's `(updated_ms, op)`; one the call cannot read
/// is `InvalidInput`.
#[test]
fn the_worker_judges_with_the_sent_view_and_pages_the_audit_by_key() {
    let placed = worker_store::placed_seeded("repair-view", seed, serve);
    let liveness = |placed: &worker_store::Placed| {
        let preview = repair(placed, "preview", None, false);
        let row = holders(&preview)
            .iter()
            .find(|row| row.get_text("operation") == Some("21".repeat(16).as_str()))
            .expect("the placed holder")
            .clone();
        (
            row.get_text("liveness").map(str::to_owned),
            row.get("safe").cloned(),
        )
    };
    assert_eq!(
        liveness(&placed),
        (Some("live".to_owned()), Some(SdkValue::Bool(false)))
    );
    let refused = layerfs_store::admin::with_users::repair(
        &placed.home,
        &request("settle", Some(0x21), false),
    )
    .unwrap_err();
    assert_eq!(refused.code, "InUse", "{refused:?}");

    // The host home now holds the tombstone of generation `generation:1` of
    // Sandbox `box` on machine `box`, and no Sandbox record: dead.
    let retired = layerfs_home::disk::paths::sandbox_retired_dir(&placed.home, "box");
    std::fs::create_dir_all(&retired).unwrap();
    layerfs_home::disk::write_record(
        &retired.join("box.json"),
        &SdkValue::map([("generation", SdkValue::text("generation:1"))]),
    )
    .unwrap();
    assert_eq!(
        liveness(&placed),
        (Some("dead".to_owned()), Some(SdkValue::Bool(true)))
    );
    let settled = repair(&placed, "settle", Some(0x21), false);
    assert_eq!(settled.get_text("status"), Some("uncertain"), "{settled:?}");
    repair(&placed, "release", Some(0x21), false);
    repair(&placed, "settle_legacy", None, false);
    repair(&placed, "release", Some(0x31), false);

    // Two retained rows, one a page: the cursor names the first row's
    // place, the second page holds the other row and ends.
    let page = |cursor: Option<String>| {
        let request = RepairRequest {
            limit: Some(1),
            cursor,
            ..request("audit", None, false)
        };
        layerfs_store::admin::with_users::repair(&placed.home, &request)
    };
    let first = page(None).expect("the first audit page");
    let cursor = first
        .get_text("next_cursor")
        .map(str::to_owned)
        .unwrap_or_else(|| panic!("a next page: {first:?}"));
    let second = page(Some(cursor)).expect("the second audit page");
    let op = |page: &SdkValue| match page.get("items") {
        Some(SdkValue::List(rows)) if rows.len() == 1 => {
            rows[0].get_text("operation").map(str::to_owned)
        }
        other => panic!("one row: {other:?}"),
    };
    let mut ops = vec![op(&first), op(&second)];
    ops.sort();
    assert_eq!(
        ops,
        vec![Some("21".repeat(16)), Some("31".repeat(16))],
        "each row once"
    );
    assert_eq!(second.get("next_cursor"), Some(&SdkValue::Null));

    // A cursor this call made, around a place it cannot read.
    let filters = [
        ("project", SdkValue::text("shop/app")),
        ("branch", SdkValue::text("main")),
        ("action", SdkValue::text("audit")),
    ];
    for place in [
        SdkValue::text("nope"),
        SdkValue::text("12:abcd"),
        SdkValue::Int(1),
    ] {
        let made = layerfs_home::reply::cursor::make("store.repair_mount_holders", place, &filters);
        let refused = page(Some(made)).unwrap_err();
        assert_eq!(refused.code, "InvalidInput", "{refused:?}");
    }
    placed.remove();
}
