//! The live 6/331 sequence on a Store a Docker worker holds: six Branches are
//! blocked by live, uncertain or legacy mount admissions while 331 Branches
//! are already retired; the read-only typed view names the six, the operator
//! settles and releases them on the worker, `branch.remove` ends the six, and
//! `store.compact` reclaims the retired rows.
//!
//! The Store is seeded on the caller's machine, before its file moves under
//! the worker's root, with the place/lease/fence evidence each mount records.
//! The lean schema keeps no Store-side machine-authority row. The test never
//! touches the live Store.
#[path = "support/worker_store.rs"]
mod worker_store;

use layerfs_bridge::control::{SdkCall, SdkRefusal, SdkValue};
use layerfs_history::error::HistoryError;
use layerfs_history::records::{GcIntent, GcIntentKind, GcScopeKind};
use layerfs_history::PlaceAuthority;
use layerfs_home::events::Events;
use layerfs_home::home::{paths, write_entity};
use layerfs_home::names::StoreName;
use layerfs_machine::record::MachineRecord;
use layerfs_machine::worker;
use layerfs_store::calls::branch::{BranchFork, BranchRemove};
use layerfs_store::calls::file::FileRead;
use layerfs_store::open_args::OpenArgs;
use layerfs_store::project::add::{add_project, ProjectAdd};
use layerfs_store::record::StoreRecord;
use layerfs_store::service::with_service;
use layerfs_store_service::history::lookup;

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// How many Branches the live Store had already retired behind the six.
const RETIRED: u16 = 331;
/// The six blocked Branches' operation identities start here.
const FIRST: u8 = 0x10;

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A fresh folder to initialize the project from, on the test's own disk so
/// the kernel guard admits its Store.
fn source(tag: &str) -> PathBuf {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "layerfs-live-src-{tag}-{}-{}",
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
    // An hour: seeding the 331 retired Branches must never let a lease the
    // test calls live expire underneath it.
    i64::try_from(now).expect("the clock fits an i64") + 3_600_000
}

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

/// The hex operation identity a Branch tag stands for.
fn op_hex(tag: u8) -> String {
    format!("{tag:02x}").repeat(16)
}

fn serve(listener: TcpListener, root: PathBuf) {
    std::thread::spawn(move || {
        layerfs_control::worker_server::serve(listener, worker_store::SECRET.to_owned(), root)
    });
}

/// One Store method on the Open service `service`.
fn engine(
    service: &Arc<layerfs_store_service::StoreService>,
    method: &str,
    args: SdkValue,
) -> SdkValue {
    let call = SdkCall {
        method: method.to_owned(),
        args,
    };
    layerfs_store_service::methods::call(service, &call)
        .unwrap_or_else(|refusal| panic!("{method}: {refusal:?}"))
}

/// Seeds the live shape on `home`: project `app`, six Branches blocked by
/// mount admissions (three live with a lease and a fence, one with an expired
/// lease, one with owner evidence only, one with no evidence) and 331
/// Branches retired behind a wholly dead pack each.
fn seed(home: &Path) {
    let shop = StoreName::parse("shop").unwrap();
    add_project(&ProjectAdd {
        home: home.to_path_buf(),
        store: shop,
        name: "app".to_owned(),
        source: source("live"),
        timeout_ms: Some(60_000),
        store_record: None,
    })
    .unwrap();
    let record = StoreRecord::read(home, "shop").unwrap().unwrap();
    let open = OpenArgs::of_record(home, "shop", &record).unwrap();
    with_service(&open, |service| {
        let history = service.store.history();
        let stack = lookup::project(&service.store, "app")?;

        for tag in FIRST..FIRST + 6 {
            let name = format!("stuck-{}", tag - FIRST);
            engine(
                service,
                "branch.fork",
                SdkValue::map([
                    ("project", text("app")),
                    ("name", text(&name)),
                    ("from", text("main")),
                ]),
            );
            let branch = lookup::branch(&service.store, &stack, &name)?;
            let op = [tag; 16];
            let intent = match tag - FIRST {
                0..=2 => GcIntent::shared(
                    op,
                    GcIntentKind::Mount,
                    GcScopeKind::Branch,
                    branch.id.as_slice(),
                )
                .in_project(stack.id.as_slice())
                .named(Some("app"), Some(&name))
                .placed(
                    Some([tag; 32]),
                    Some(&format!("sandbox-{}", tag - FIRST)),
                    Some("box"),
                )
                .owned(Some("pid:1@0"), Some(&format!("boot:{tag}")))
                .authorized(PlaceAuthority {
                    machine_incarnation: Some("worker:1".to_owned()),
                    sandbox_generation: Some("generation:1".to_owned()),
                }),
                // A live place whose lease has expired: uncertain, not dead.
                3 => GcIntent::shared(
                    op,
                    GcIntentKind::Mount,
                    GcScopeKind::Branch,
                    branch.id.as_slice(),
                )
                .in_project(stack.id.as_slice())
                .named(Some("app"), Some(&name))
                .placed(Some([tag; 32]), Some("sandbox-expired"), Some("box"))
                .owned(Some("pid:1@0"), Some("boot:3"))
                .authorized(PlaceAuthority {
                    machine_incarnation: Some("worker:1".to_owned()),
                    sandbox_generation: None,
                }),
                // Owner evidence only: legacy, and a PID namespace this
                // process cannot soundly probe, so conservatively live.
                4 => GcIntent::shared(
                    op,
                    GcIntentKind::Mount,
                    GcScopeKind::Branch,
                    branch.id.as_slice(),
                )
                .in_project(stack.id.as_slice())
                .named(Some("app"), Some(&name))
                .owned(Some("pid:1@0"), Some("boot:4")),
                // No owner and no place: a legacy row and still a holder.
                _ => GcIntent::shared(
                    op,
                    GcIntentKind::Mount,
                    GcScopeKind::Branch,
                    branch.id.as_slice(),
                )
                .in_project(stack.id.as_slice())
                .named(Some("app"), Some(&name)),
            };
            history.gc_register(&intent).unwrap();

        }

        // The 331 already retired behind the six: fork, a wholly dead pack,
        // then the marker. The names stay taken until the compaction.
        for tag in 0..RETIRED {
            let name = format!("retired-{tag}");
            engine(
                service,
                "branch.fork",
                SdkValue::map([
                    ("project", text("app")),
                    ("name", text(&name)),
                    ("from", text("main")),
                ]),
            );
            let bytes: Vec<u8> = (0..4096u32)
                .map(|index| ((index + u32::from(tag) * 7) % 251) as u8)
                .collect();
            engine(
                service,
                "fs.write",
                SdkValue::map([
                    ("project", text("app")),
                    ("branch", text(&name)),
                    ("path", text("/dead.bin")),
                    ("bytes", SdkValue::Bytes(bytes)),
                    ("message", text("retired content")),
                ]),
            );
            engine(
                service,
                "branch.remove",
                SdkValue::map([("project", text("app")), ("branch", text(&name))]),
            );
        }
        Ok(())
    })
    .unwrap();
}

fn repair(
    placed: &worker_store::Placed,
    branch: &str,
    action: &str,
    op: Option<u8>,
    legacy: bool,
) -> SdkValue {
    let name = StoreName::parse("shop").unwrap();
    layerfs_control::store::repair(
        &placed.home,
        &name,
        "app",
        branch,
        action,
        op.map(|op| [op; 16]),
        legacy,
        "alice",
        0,
        100,
    )
    .unwrap_or_else(|refusal| panic!("{action} {branch}: {refusal:?}"))
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

/// The worker's read-only done-audit event from `store.diagnose`: the same
/// doctor route the operator uses, never a release.
fn diagnostic_audit(placed: &worker_store::Placed) -> SdkValue {
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
    value
        .get("gc_audit")
        .cloned()
        .expect("the diagnostic carries the audit page")
}

fn branch_remove(placed: &worker_store::Placed, branch: &str) -> Result<SdkValue, SdkRefusal> {
    let mut sink = |_event: SdkValue| {};
    let mut events = Events::new(&mut sink);
    layerfs_store::calls::branch::remove(
        &placed.home,
        &mut events,
        &BranchRemove {
            project: "shop/app".to_owned(),
            branch: branch.to_owned(),
        },
    )
}

fn branch_fork(
    placed: &worker_store::Placed,
    name: &str,
    from: &str,
) -> Result<SdkValue, SdkRefusal> {
    let mut sink = |_event: SdkValue| {};
    let mut events = Events::new(&mut sink);
    layerfs_store::calls::branch::fork(
        &placed.home,
        &mut events,
        &BranchFork {
            project: "shop/app".to_owned(),
            name: name.to_owned(),
            from: Some(from.to_owned()),
            at: None,
        },
    )
}

fn read_file(
    placed: &worker_store::Placed,
    branch: &str,
    path: &str,
) -> Result<SdkValue, SdkRefusal> {
    let mut sink = |_event: SdkValue| {};
    let mut events = Events::new(&mut sink);
    layerfs_store::calls::file::read(
        &placed.home,
        &mut events,
        &FileRead {
            project: "shop/app".to_owned(),
            branch: branch.to_owned(),
            path: path.to_owned(),
            ..FileRead::default()
        },
    )
}

fn compact(placed: &worker_store::Placed) -> Result<SdkValue, SdkRefusal> {
    let mut sink = |_event: SdkValue| {};
    let mut events = Events::new(&mut sink);
    layerfs_control::store::compact(
        &placed.home,
        &mut events,
        &StoreName::parse("shop").unwrap(),
    )
}

fn diagnosed_row(rows: &[SdkValue], tag: u8) -> &SdkValue {
    rows.iter()
        .find(|row| row.get_text("operation") == Some(op_hex(tag).as_str()))
        .unwrap_or_else(|| panic!("no diagnostic holder {tag:02x}"))
}

fn previewed_row(rows: &[SdkValue], tag: u8) -> &SdkValue {
    rows.iter()
        .find(|row| row.get_text("op") == Some(op_hex(tag).as_str()))
        .unwrap_or_else(|| panic!("no previewed holder {tag:02x}"))
}

/// Seeds a Store a worker will take with three mount intents at one Branch,
/// all placed on machine `box`: one recorded under an older incarnation than
/// the home's current record (dead), one under a newer one (a restored home:
/// uncertain, never dead), and one whose recorded Sandbox generation is the
/// retired generation the home tombstone names (dead). The current machine
/// record and the tombstone are the only place authority the worker receives.
fn seed_reconcile(home: &Path) {
    let shop = StoreName::parse("shop").unwrap();
    add_project(&ProjectAdd {
        home: home.to_path_buf(),
        store: shop,
        name: "app".to_owned(),
        source: source("reconcile"),
        timeout_ms: Some(60_000),
        store_record: None,
    })
    .unwrap();
    // The start records the ordered incarnation before the worker serves, and
    // a Sandbox retired before it leaves the home tombstone control reads
    // outside the Sandbox namespace. Both are written after the seed below:
    // admission consults its caller's home, so the mounts must exist before
    // the new incarnation does for the worker start to be the first caller
    // that can settle the older placement.
    let current = "p1:worker:0000000000000020:aa";
    let retired = "p1:sandbox:0000000000000010:aa";
    let record = StoreRecord::read(home, "shop").unwrap().unwrap();
    let open = OpenArgs::of_record(home, "shop", &record).unwrap();
    with_service(&open, |service| {
        let history = service.store.history();
        let stack = lookup::project(&service.store, "app")?;
        engine(
            service,
            "branch.fork",
            SdkValue::map([
                ("project", text("app")),
                ("name", text("stuck-reconcile")),
                ("from", text("main")),
            ]),
        );
        let branch = lookup::branch(&service.store, &stack, "stuck-reconcile")?;
        let mount = |op: u8, sandbox: &str, recorded: &str, generation: &str, boot: &str| {
            GcIntent::shared(
                [op; 16],
                GcIntentKind::Mount,
                GcScopeKind::Branch,
                branch.id.as_slice(),
            )
            .in_project(stack.id.as_slice())
            .named(Some("app"), Some("stuck-reconcile"))
            .placed(Some([op; 32]), Some(sandbox), Some("box"))
            .owned(Some("pid:1@0"), Some(boot))
            .with_phase(Some("bind"))
            .authorized(PlaceAuthority {
                machine_incarnation: Some(recorded.to_owned()),
                sandbox_generation: Some(generation.to_owned()),
            })
        };
        // The restored (newer) mount and the retired Sandbox come first; the
        // older incarnation last, before the home names the current one. A
        // registration settles an older row from the home it can see, so an
        // older mount seeded before the new incarnation survives admission
        // for the worker start to settle.
        history
            .gc_register(&mount(
                0x82,
                "live",
                "p1:worker:0000000000000030:aa",
                "p1:sandbox:0000000000000020:aa",
                "boot:82",
            ))
            .unwrap();
        history
            .gc_register(&mount(0x83, "retired", current, retired, "boot:83"))
            .unwrap();
        history
            .gc_register(&mount(
                0x81,
                "live",
                "p1:worker:0000000000000010:aa",
                "p1:sandbox:0000000000000020:aa",
                "boot:81",
            ))
            .unwrap();
        Ok(())
    })
    .unwrap();
    MachineRecord::of(home, "box")
        .unwrap()
        .with_incarnation(current)
        .write(home)
        .unwrap();
    write_entity(
        &paths::sandbox_tombstone(home, "box", "retired"),
        &SdkValue::map([
            ("name", text("retired")),
            ("machine", text("box")),
            ("generation", text(retired)),
            ("retired_ms", SdkValue::Int(1)),
        ]),
    )
    .unwrap();
}

/// §10.5/§10.12 gap 17: a worker start reconciles only the intents its new
/// ordered incarnation (and the home's retired tombstone) proves dead. The
/// control layer builds the bounded place view from its home, hands it to the
/// worker with the reconcile, and the restored home's newer recorded value is
/// never death. The preview still lists that one holder.
#[test]
fn a_worker_start_reconciles_the_incarnations_its_home_records() {
    let placed = worker_store::placed_seeded("reconcile", seed_reconcile, serve);
    let outcome = layerfs_store::service::reconcile_machine(
        &placed.home,
        "shop",
        "box",
        "p1:worker:0000000000000020:aa",
    )
    .expect("the start reconciles the worker's Store");
    assert_eq!(outcome.get_text("machine"), Some("box"), "{outcome:?}");
    assert_eq!(
        outcome.get_text("incarnation"),
        Some("p1:worker:0000000000000020:aa"),
        "{outcome:?}"
    );
    let mut settled = Vec::new();
    match outcome.get("settled") {
        Some(SdkValue::List(items)) => {
            for item in items {
                match item {
                    SdkValue::Text(op) => settled.push(op.clone()),
                    other => panic!("a settled op is text: {other:?}"),
                }
            }
        }
        other => panic!("settled: {other:?}"),
    }
    settled.sort();
    assert_eq!(
        settled,
        vec![op_hex(0x81), op_hex(0x83)],
        "the older incarnation and the retired generation die; the restore does not"
    );
    assert_eq!(outcome.get_int("held"), Some(1), "{outcome:?}");
    assert_eq!(outcome.get_int("uncertain"), Some(1), "{outcome:?}");

    // The restored home's newer recorded incarnation is the only holder left.
    let preview = repair(&placed, "stuck-reconcile", "preview", None, false);
    let rows = holders(&preview);
    assert_eq!(rows.len(), 1, "{preview:?}");
    assert_eq!(previewed_row(rows, 0x82).get_text("state"), Some("live"));
    placed.remove();
}

#[test]
fn the_live_six_of_three_hundred_thirty_one_is_repaired_removed_and_reclaimed() {
    let placed = worker_store::placed_seeded("live-s6", seed, serve);

    // 1. The typed read-only view over the worker path: owner, place, lease
    // and state. It releases nothing.
    let diagnosed = diagnostic_holders(&placed);
    assert_eq!(diagnosed.len(), 6, "{diagnosed:?}");
    let live = diagnosed_row(&diagnosed, FIRST);
    assert_eq!(live.get_text("owner_runtime"), Some("pid:1@0"));
    assert_eq!(live.get_text("machine_incarnation"), Some("worker:1"));
    assert_eq!(live.get_text("sandbox_generation"), Some("generation:1"));
    assert_eq!(live.get_text("state"), Some("live"));
    assert_eq!(
        diagnosed_row(&diagnosed, FIRST + 4).get("machine_incarnation"),
        Some(&SdkValue::Null)
    );
    assert_eq!(
        diagnosed_row(&diagnosed, FIRST + 5).get("machine_incarnation"),
        Some(&SdkValue::Null)
    );

    // The operator's per-Branch preview answers the same typed view and
    // settles nothing.
    for tag in FIRST..FIRST + 6 {
        let name = format!("stuck-{}", tag - FIRST);
        let preview = repair(&placed, &name, "preview", None, false);
        let rows = holders(&preview);
        assert_eq!(rows.len(), 1, "{name}: {preview:?}");
        let row = previewed_row(rows, tag);
        match tag - FIRST {
            0..=2 => {
                assert_eq!(
                    row.get_text("sandbox"),
                    Some(format!("sandbox-{}", tag - FIRST).as_str())
                );
                assert_eq!(row.get_text("machine"), Some("box"));
                assert_eq!(row.get_text("machine_incarnation"), Some("worker:1"));
                assert_eq!(row.get_text("sandbox_generation"), Some("generation:1"));
                assert_eq!(row.get("legacy"), Some(&SdkValue::Bool(false)));
            }
            3 => assert_eq!(
                row.get("legacy"),
                Some(&SdkValue::Bool(false)),
                "a current place with owner evidence is not a legacy row"
            ),
            _ => assert_eq!(row.get("legacy"), Some(&SdkValue::Bool(true))),
        }
    }

    // 2. Each Branch is still held and the Store-wide compact barrier stands:
    // every owner is `MountsActive`, never adopted.
    for tag in FIRST..FIRST + 6 {
        let name = format!("stuck-{}", tag - FIRST);
        let refusal = branch_remove(&placed, &name).expect_err("the mount holds the Branch");
        assert_eq!(refusal.code, "MountsActive", "{refusal:?}");
    }
    let refused = compact(&placed).expect_err("the six mounts hold the Store");
    assert_eq!(refused.code, "MountsActive", "{refused:?}");

    // 3. The operator repairs one Branch at a time on the worker: settle is
    // explicit, then release; each legacy row goes through `settle_legacy`.
    for tag in FIRST..FIRST + 4 {
        let name = format!("stuck-{}", tag - FIRST);
        repair(&placed, &name, "settle", Some(tag), false);
    }
    for tag in FIRST + 4..FIRST + 6 {
        let name = format!("stuck-{}", tag - FIRST);
        let settled = repair(&placed, &name, "settle_legacy", None, false);
        assert_eq!(settled.get_int("settled"), Some(1), "{settled:?}");
    }
    for tag in FIRST..FIRST + 6 {
        let name = format!("stuck-{}", tag - FIRST);
        repair(&placed, &name, "release", Some(tag), false);
    }

    // 3b. Release retained the six rows as the durable done audit: preview
    // and the live queries filter them, and the bounded audit read reports
    // the operator/action/op evidence. Compaction must keep them.
    for tag in FIRST..FIRST + 6 {
        let name = format!("stuck-{}", tag - FIRST);
        let preview = repair(&placed, &name, "preview", None, false);
        assert_eq!(holders(&preview).len(), 0, "{name}: {preview:?}");
    }
    // The audit is scoped to the Branch it names: each Branch's page holds
    // exactly its own two preview rows and one release row.
    for tag in FIRST..FIRST + 6 {
        let name = format!("stuck-{}", tag - FIRST);
        let audit = repair(&placed, &name, "audit", None, false);
        let Some(SdkValue::List(rows)) = audit.get("items") else {
            panic!("audit items for {name}: {audit:?}");
        };
        for row in rows {
            assert_eq!(
                row.get_text("branch"),
                Some(name.as_str()),
                "a Branch audit never reports another Branch's row: {audit:?}"
            );
        }
        let releases = rows
            .iter()
            .filter(|row| {
                row.get_text("evidence")
                    .is_some_and(|evidence| evidence.contains("action=release;operator=alice;op="))
            })
            .count();
        let previews = rows
            .iter()
            .filter(|row| row.get_text("evidence") == Some("action=preview;operator=alice"))
            .count();
        assert_eq!(releases, 1, "{name}: {audit:?}");
        assert_eq!(previews, 2, "{name}: {audit:?}");
    }

    // 3c. The doctor's worker-side audit is the same bounded page (16 rows at
    // limit 16) over the same retained rows, as evidence; it releases nothing.
    let doctor = diagnostic_audit(&placed);
    let Some(SdkValue::List(rows)) = doctor.get("items") else {
        panic!("doctor audit items: {doctor:?}");
    };
    assert_eq!(
        rows.len(),
        16,
        "the worker audit page is bounded: {doctor:?}"
    );
    assert_eq!(
        doctor.get("has_more"),
        Some(&SdkValue::Bool(true)),
        "{doctor:?}"
    );
    let releases = rows
        .iter()
        .filter(|row| {
            row.get_text("evidence")
                .is_some_and(|evidence| evidence.starts_with("action=release;"))
        })
        .count();
    let previews = rows
        .iter()
        .filter(|row| row.get_text("evidence") == Some("action=preview;operator=alice"))
        .count();
    assert_eq!(releases, 6, "{doctor:?}");
    assert_eq!(previews, 10, "{doctor:?}");

    // 4. The six now remove, and their names stay retired until compaction.
    for tag in FIRST..FIRST + 6 {
        let name = format!("stuck-{}", tag - FIRST);
        let removed = branch_remove(&placed, &name).expect("the repaired Branch removes");
        assert_eq!(removed.get_text("outcome"), Some("removed"), "{removed:?}");
        assert_eq!(removed.get_text("branch"), Some(name.as_str()));
        assert!(removed.get_text("removed_at").is_some(), "{removed:?}");
    }
    let held = branch_fork(&placed, "stuck-0", "main").expect_err("the retired name is held");
    assert_eq!(held.code, "NameTaken", "{held:?}");
    match held.detail.get("holder") {
        Some(holder) => assert_eq!(holder.get_text("state"), Some("retired")),
        None => panic!("the held name names its holder: {held:?}"),
    }

    // 5. The compaction reclaims the 331 retired rows (and the six): it is
    // admitted now, rebuilds at the next generation, and frees the names.
    let report = compact(&placed).expect("the repaired Store compacts");
    assert_eq!(report.get_text("outcome"), Some("compacted"), "{report:?}");
    let before = report.get_int("generation_before").expect("before");
    assert_eq!(
        report.get_int("generation_after"),
        Some(before + 1),
        "{report:?}"
    );
    assert!(
        report.get_int("discarded_bytes").unwrap_or(-1) > 0,
        "{report:?}"
    );
    branch_fork(&placed, "retired-0", "main").expect("compaction frees a retired name");
    branch_fork(&placed, "stuck-0", "main").expect("compaction frees one of the six");
    let read = read_file(&placed, "main", "/a.txt").expect("main still reads");
    assert_eq!(read.get_text("text"), Some("a\n"));

    // 6. No holder remains, and the six done rows survived the rebuild as the
    // durable operator audit. Preview still filters them.
    let preview = repair(&placed, "main", "preview", None, false);
    assert_eq!(holders(&preview).len(), 0, "no holder remains");
    // The audit is scoped: main has only the preview this step wrote, and
    // each repaired Branch keeps its own six rows through the rebuild.
    let audit = repair(&placed, "main", "audit", None, false);
    let Some(SdkValue::List(rows)) = audit.get("items") else {
        panic!("audit items: {audit:?}");
    };
    assert_eq!(rows.len(), 1, "main's audit is the preview it wrote: {audit:?}");
    assert_eq!(
        rows[0].get_text("evidence"),
        Some("action=preview;operator=alice")
    );
    for tag in FIRST..FIRST + 6 {
        let name = format!("stuck-{}", tag - FIRST);
        let audit = repair(&placed, &name, "audit", None, false);
        let Some(SdkValue::List(rows)) = audit.get("items") else {
            panic!("audit items for {name}: {audit:?}");
        };
        let releases = rows
            .iter()
            .filter(|row| {
                row.get_text("evidence")
                    .is_some_and(|evidence| evidence.starts_with("action=release;"))
            })
            .count();
        let previews = rows
            .iter()
            .filter(|row| row.get_text("evidence") == Some("action=preview;operator=alice"))
            .count();
        assert_eq!(releases, 1, "the compaction kept {name}'s release: {audit:?}");
        assert_eq!(previews, 2, "the compaction kept {name}'s previews: {audit:?}");
    }

    placed.remove();
}
