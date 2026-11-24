//! Operator repair of a Branch's mount holders on the Store a Docker worker
//! holds: preview, settle, release and `settle_legacy` run in that worker,
//! the preview answers the typed holder view, and the worker's read-only
//! diagnostic reports the same state without settling anything. The lean
//! schema keeps no Store-side audit table; release retains the done row and
//! the bounded audit read reports it.
#[path = "support/worker_store.rs"]
mod worker_store;

use layerfs_bridge::control::SdkValue;
use layerfs_history::records::{GcIntent, GcIntentKind, GcScopeKind};
use layerfs_history::{HistoryName, NamedSnapshot, PlaceAuthority};
use layerfs_home::names::StoreName;
use layerfs_machine::worker;
use layerfs_store::open_args::OpenArgs;
use layerfs_store::project::add::{add_project, ProjectAdd};
use layerfs_store::record::StoreRecord;
use layerfs_store::service::{branch_named, with_service};
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

fn future_ms() -> i64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the clock is after the epoch")
        .as_millis();
    i64::try_from(now).expect("the clock fits an i64") + 60_000
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
        "project.list_branches names the main Branch add_project recorded"
    );
    let stack = with_service(&open, |service| {
        Ok(layerfs_store_service::history::lookup::project(&service.store, "app")?.id)
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
        layerfs_control::worker_server::serve(listener, worker_store::SECRET.to_owned(), root)
    });
}

fn repair(placed: &worker_store::Placed, action: &str, op: Option<u8>, legacy: bool) -> SdkValue {
    let name = StoreName::parse("shop").unwrap();
    layerfs_control::store::repair(
        &placed.home,
        &name,
        "app",
        "main",
        action,
        op.map(|op| [op; 16]),
        legacy,
        "alice",
        0,
        100,
    )
    .unwrap_or_else(|refusal| panic!("{action}: {refusal:?}"))
}

fn holders(value: &SdkValue) -> &[SdkValue] {
    match value.get("holders") {
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
    let value = worker::call(
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

    // Preview over the worker answers the typed view and settles nothing.
    let preview = repair(&placed, "preview", None, false);
    let rows = holders(&preview);
    assert_eq!(rows.len(), 2);
    let live = rows
        .iter()
        .find(|row| row.get_text("op") == Some("21".repeat(16).as_str()))
        .expect("the live holder");
    assert_eq!(live.get_text("state"), Some("live"));
    assert_eq!(live.get_text("sandbox"), Some("box"));
    assert_eq!(live.get_text("machine_incarnation"), Some("worker:1"));
    assert_eq!(live.get_text("sandbox_generation"), Some("generation:1"));
    assert_eq!(live.get("legacy"), Some(&SdkValue::Bool(false)));
    let legacy = rows
        .iter()
        .find(|row| row.get_text("op") == Some("31".repeat(16).as_str()))
        .expect("the legacy holder");
    assert_eq!(legacy.get("legacy"), Some(&SdkValue::Bool(true)));

    // Release without settle is refused; settle marks the one named op.
    let name = StoreName::parse("shop").unwrap();
    let refused = layerfs_control::store::repair(
        &placed.home,
        &name,
        "app",
        "main",
        "release",
        Some([0x21; 16]),
        false,
        "alice",
        0,
        100,
    )
    .unwrap_err();
    assert_eq!(refused.code, "Conflict", "{refused:?}");
    repair(&placed, "settle", Some(0x21), false);
    let preview = repair(&placed, "preview", None, false);
    let settled = holders(&preview)
        .iter()
        .find(|row| row.get_text("op") == Some("21".repeat(16).as_str()))
        .expect("the settled holder");
    assert_eq!(settled.get_text("state"), Some("uncertain"));

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
    let previews: Vec<_> = rows
        .iter()
        .filter(|row| row.get_text("evidence") == Some("action=preview;operator=alice"))
        .collect();
    assert_eq!(previews.len(), 4, "every worker preview: {audit:?}");
    placed.remove();
}
