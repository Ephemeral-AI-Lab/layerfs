//! `store.upgrade`'s control half (#804 §2, §5.1 step 1, §11 F, H, M, N):
//! the exclusive operation it records, the refusals made before the engine
//! is reached (each one changes nothing), the adoption of an earlier
//! upgrade left unresolved, the worker's `store.upgrade` refusals and its
//! `store.cancel` plumbing, and the `format` section's shape. Nothing here
//! needs the engine's conversion (`store_upgrade_engine.rs` does).
#[path = "../support/root.rs"]
mod root;
#[path = "../support/worker_store.rs"]
mod worker_store;

use layerfs_home::disk::{self, paths, write_record};
use layerfs_home::names::StoreName;
use layerfs_home::record::intent::{Op, Standing};
use layerfs_home::reply::events::Events;
use layerfs_machine::route;
use layerfs_persistence::upgrade::{Format, FormatStatus};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_store::admin::record::intent;
use layerfs_store::admin::record::store::StoreRecord;
use layerfs_store::upgrade::{Running, CANCEL_OP, FORMAT_OP, WORKER_OP};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use worker_store::{placed, scratch, store_record};

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

/// A home with sqlite Store `shop` on native, or `None` where this
/// filesystem cannot hold a directly opened Store.
fn home_with_shop(tag: &str) -> Option<PathBuf> {
    let home = scratch(tag).join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    layerfs_home::disk::open(&home).unwrap();
    match layerfs_store::backend::sqlite::create::create(
        &home,
        &StoreName::parse("shop").unwrap(),
        None,
    ) {
        Ok(_) => Some(home),
        Err(refusal) if refusal.message.contains("does not carry SQLite locks") => {
            eprintln!("NOT RUN: this filesystem cannot hold a directly opened Store: {refusal:?}");
            None
        }
        Err(refusal) => panic!("store.create: {refusal:?}"),
    }
}

/// The Store's file, byte for byte, and whether a sidecar is beside it.
fn file_state(root: &Path) -> (Vec<u8>, bool) {
    let file = paths::store_file(root, "shop");
    let wal = PathBuf::from(format!("{}-wal", file.display()));
    (std::fs::read(&file).unwrap(), wal.exists())
}

/// `store upgrade shop` through the control call, with its events.
fn upgrade(home: &Path) -> (Result<SdkValue, SdkRefusal>, Vec<SdkValue>) {
    let mut seen = Vec::new();
    let mut sink = |event| seen.push(event);
    let name = StoreName::parse("shop").unwrap();
    let outcome = layerfs_store::admin::with_users::upgrade(
        home,
        &mut Events::new(&mut sink),
        &name,
        &layerfs_sandbox::users::STORE,
    );
    (outcome, seen)
}

fn op_of(home: &Path) -> Option<Op> {
    StoreRecord::of(home, "shop").unwrap().op()
}

/// A live process other than this one, holding nothing.
fn other_process() -> std::process::Child {
    std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .unwrap()
}

/// N: the upgrade is exclusive, its own call clears it, a read never
/// settles it, and it shows as `upgrade`.
#[test]
fn the_upgrade_operation_is_exclusive_and_cleared_by_the_next_upgrade() {
    assert!(intent::exclusive(intent::UPGRADE));
    assert!(!intent::settled_by_read(intent::UPGRADE));
    assert_eq!(intent::word(intent::UPGRADE), "upgrade");
    let op = Op::begin(intent::UPGRADE, "convert", SdkValue::Null).unwrap();
    assert_eq!(intent::clearing(&op, "shop"), "store upgrade shop");
}

/// F: another process's lease is `InUse`, naming it; nothing is written
/// and the file is unchanged.
#[test]
fn another_process_s_lease_refuses_in_use_and_changes_nothing() {
    let Some(home) = home_with_shop("lease") else {
        return;
    };
    let before = file_state(&home);
    let mut holder = other_process();
    disk::private_dir(&paths::lease_dir(&home, "shop")).unwrap();
    let lease = paths::lease_dir(&home, "shop").join(holder.id().to_string());
    std::fs::write(&lease, "1\n").unwrap();
    let (outcome, events) = upgrade(&home);
    let _ = holder.kill();
    let _ = holder.wait();
    let refusal = outcome.unwrap_err();
    assert_eq!(refusal.code, "InUse", "{refusal:?}");
    assert!(
        refusal.message.contains(&holder.id().to_string()),
        "{refusal:?}"
    );
    assert!(events.is_empty(), "{events:?}");
    assert!(op_of(&home).is_none(), "nothing is recorded");
    assert_eq!(file_state(&home), before);
}

/// F: this process's own lease (a call holding the Store) is `InUse`
/// once the operation is written, and the operation is dropped again.
#[test]
fn this_process_s_lease_refuses_in_use_and_drops_the_operation() {
    let Some(home) = home_with_shop("own") else {
        return;
    };
    let before = file_state(&home);
    let held = layerfs_store::admin::open::lease::hold(&home, "shop").unwrap();
    let (outcome, _) = upgrade(&home);
    drop(held);
    let refusal = outcome.unwrap_err();
    assert_eq!(refusal.code, "InUse", "{refusal:?}");
    assert!(op_of(&home).is_none(), "the operation is dropped");
    assert_eq!(file_state(&home), before);
}

/// F: a Sandbox of one of the Store's projects is `InUse` (the control
/// call reads the Sandbox records).
#[test]
fn a_sandbox_of_its_projects_refuses_in_use() {
    let Some(home) = home_with_shop("sandbox") else {
        return;
    };
    let before = file_state(&home);
    let sandbox = SdkValue::map([
        ("v", SdkValue::Int(1)),
        ("name", t("s1")),
        ("project", t("shop/web")),
        ("machine", t(layerfs_home::names::HOST_MACHINE)),
    ]);
    write_record(&paths::sandbox_record(&home, "s1"), &sandbox).unwrap();
    let (outcome, _) = upgrade(&home);
    let refusal = outcome.unwrap_err();
    assert_eq!(refusal.code, "InUse", "{refusal:?}");
    assert!(refusal.message.contains("s1"), "{refusal:?}");
    assert!(op_of(&home).is_none());
    assert_eq!(file_state(&home), before);
}

/// F: another operation running on the Store is `Busy`, naming it, and
/// stays the Store's operation.
#[test]
fn another_running_operation_refuses_busy() {
    let Some(home) = home_with_shop("busy") else {
        return;
    };
    let before = file_state(&home);
    let backup = intent::begin(&home, "shop", intent::BACKUP, "upload", SdkValue::Null).unwrap();
    let (outcome, _) = upgrade(&home);
    let refusal = outcome.unwrap_err();
    assert_eq!(refusal.code, "Busy", "{refusal:?}");
    assert_eq!(op_of(&home).unwrap().id(), backup.id());
    assert_eq!(file_state(&home), before);
}

/// F: another call's operation left unresolved is `Unresolved`, with that
/// call as the fix; the upgrade does not adopt it.
#[test]
fn another_call_s_unresolved_operation_refuses_unresolved() {
    let Some(home) = home_with_shop("unresolved") else {
        return;
    };
    let compact = intent::begin(&home, "shop", intent::COMPACT, "build", SdkValue::Null).unwrap();
    intent::leave(&home, "shop", &compact, "lost");
    let (outcome, _) = upgrade(&home);
    let refusal = outcome.unwrap_err();
    assert_eq!(refusal.code, "Unresolved", "{refusal:?}");
    assert_eq!(
        refusal.detail.get_text("fix"),
        Some("layerfs store compact shop")
    );
    assert_eq!(op_of(&home).unwrap().id(), compact.id());
}

/// F: an upgrade left unresolved reads `broken` `unresolved` with
/// `layerfs store upgrade shop` as its fix, refuses every other operation,
/// and is adopted by the next upgrade, which is not refused `Unresolved`.
/// A run that does not settle it (a refusal) puts it back; one that
/// answers drops it.
#[test]
fn an_unresolved_upgrade_is_adopted_by_the_next_upgrade() {
    let Some(home) = home_with_shop("adopt") else {
        return;
    };
    let earlier = intent::begin(&home, "shop", intent::UPGRADE, "convert", SdkValue::Null).unwrap();
    intent::leave(&home, "shop", &earlier, "the worker's reply was lost");
    let record = StoreRecord::of(&home, "shop").unwrap();
    assert_eq!(record.op().unwrap().standing(), Standing::Unresolved);
    let state = layerfs_store::admin::status::state::held(&record).unwrap();
    let fix = format!("{:?}", state.fields());
    assert!(fix.contains("layerfs store upgrade shop"), "{fix}");
    // a compaction is refused by it, naming the upgrade as the fix
    let refusal = layerfs_store::admin::open::lease::hold(&home, "shop")
        .err()
        .unwrap();
    assert_eq!(refusal.code, "Unresolved", "{refusal:?}");
    assert_eq!(
        refusal.detail.get_text("fix"),
        Some("layerfs store upgrade shop")
    );
    let (outcome, _) = upgrade(&home);
    match &outcome {
        Ok(answer) => {
            assert!(op_of(&home).is_none(), "{answer:?}");
        }
        Err(refusal) => {
            assert_ne!(refusal.code, "Unresolved", "{refusal:?}");
            assert_ne!(refusal.code, "Unknown", "{refusal:?}");
            assert_eq!(
                op_of(&home).map(|op| op.id().to_owned()),
                Some(earlier.id().to_owned()),
                "a run that did not settle it puts it back: {refusal:?}"
            );
        }
    }
}

/// H: the worker's `store.cancel` sets the flag of the upgrade of that id
/// running there, on a connection of its own, and an id with no running
/// upgrade is `NotFound`; the flag goes when the run ends.
#[test]
fn the_worker_s_cancel_sets_the_running_upgrade_s_flag() {
    let fixture = placed("cancel", |listener, root| {
        std::thread::spawn(move || {
            layerfs_api::worker::serve::serve(listener, worker_store::SECRET.into(), root)
        });
    });
    let running = Running::register("op-cancel-1").unwrap();
    assert!(!running.flag().load(Ordering::SeqCst));
    // one id runs once
    assert_eq!(Running::register("op-cancel-1").err().unwrap().code, "Busy");
    let args = SdkValue::map([("op", t("op-cancel-1"))]);
    let answer = route::call(&fixture.endpoint, CANCEL_OP, args.clone(), &mut |_| {}).unwrap();
    assert_eq!(answer, SdkValue::Map(Vec::new()));
    assert!(running.flag().load(Ordering::SeqCst));
    drop(running);
    let refusal = route::call(&fixture.endpoint, CANCEL_OP, args, &mut |_| {}).unwrap_err();
    assert_eq!(refusal.code, "NotFound", "{refusal:?}");
    let refusal =
        route::call(&fixture.endpoint, CANCEL_OP, SdkValue::Null, &mut |_| {}).unwrap_err();
    assert_eq!(refusal.code, "InvalidInput", "{refusal:?}");
    fixture.remove();
}

/// The worker's `store.upgrade` is refused `InUse` while another process
/// holds the Store's lease under its root, before the engine is reached,
/// and the call through the home passes that refusal on, drops its
/// operation and leaves the file as it was.
#[test]
fn the_worker_refuses_an_upgrade_of_a_store_another_process_holds() {
    let fixture = placed("worker-lease", |listener, root| {
        std::thread::spawn(move || {
            layerfs_api::worker::serve::serve(listener, worker_store::SECRET.into(), root)
        });
    });
    let before = file_state(&fixture.root);
    let mut holder = other_process();
    disk::private_dir(&paths::lease_dir(&fixture.root, "shop")).unwrap();
    let lease = paths::lease_dir(&fixture.root, "shop").join(holder.id().to_string());
    std::fs::write(&lease, "1\n").unwrap();
    // the worker op alone
    let args = SdkValue::map([
        ("store", t("shop")),
        ("store_record", store_record(&fixture.home)),
        ("op", t("op-lease-1")),
    ]);
    let refusal = route::call(&fixture.endpoint, WORKER_OP, args, &mut |_| {}).unwrap_err();
    assert_eq!(refusal.code, "InUse", "{refusal:?}");
    // and through the home
    let (outcome, events) = upgrade(&fixture.home);
    let _ = holder.kill();
    let _ = holder.wait();
    let refusal = outcome.unwrap_err();
    assert_eq!(refusal.code, "InUse", "{refusal:?}");
    assert!(events.is_empty(), "{events:?}");
    assert!(op_of(&fixture.home).is_none());
    assert_eq!(file_state(&fixture.root), before);
    fixture.remove();
}

/// The worker's `store.upgrade` takes a sqlite Store's record and its op.
#[test]
fn the_worker_operation_needs_a_sqlite_record_and_an_op() {
    let fixture = placed("worker-args", |listener, root| {
        std::thread::spawn(move || {
            layerfs_api::worker::serve::serve(listener, worker_store::SECRET.into(), root)
        });
    });
    for args in [
        SdkValue::map([
            ("store", t("shop")),
            ("store_record", store_record(&fixture.home)),
        ]),
        SdkValue::map([("store", t("shop")), ("op", t("x"))]),
        SdkValue::map([
            ("store", t("shop")),
            ("op", t("x")),
            (
                "store_record",
                SdkValue::map([("name", t("shop")), ("kind", t("postgresql"))]),
            ),
        ]),
    ] {
        let refusal = route::call(&fixture.endpoint, WORKER_OP, args, &mut |_| {}).unwrap_err();
        assert_eq!(refusal.code, "InvalidInput", "{refusal:?}");
    }
    fixture.remove();
}

/// A stopped machine: the upgrade is `Unavailable` with the start as its
/// fix and records nothing; the format section says the format is not
/// known, starting nothing.
#[test]
fn a_stopped_machine_is_unavailable_and_its_format_unknown() {
    let fixture = placed("stopped", |_, _| {});
    std::fs::remove_file(paths::worker_record(&fixture.home, "box")).unwrap();
    let (outcome, _) = upgrade(&fixture.home);
    let refusal = outcome.unwrap_err();
    assert_eq!(refusal.code, "Unavailable", "{refusal:?}");
    assert_eq!(
        refusal.detail.get_text("fix"),
        Some("layerfs machine start box")
    );
    assert!(op_of(&fixture.home).is_none());
    let format = get_format(&fixture.home).unwrap();
    assert_eq!(format.get_text("status"), Some("unknown"), "{format:?}");
    assert_eq!(format.get("version"), Some(&SdkValue::Null));
    assert_eq!(format.get_int("latest"), Some(3));
    assert_eq!(format.get("fix"), Some(&SdkValue::Null));
    let items = list_formats(&fixture.home);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].get_text("status"), Some("unknown"));
    fixture.remove();
}

/// The worker's `store.format` is read-only and answers a section for a
/// Store it has (whatever the engine makes of the file) or `unknown` for
/// one it has not.
#[test]
fn the_worker_answers_a_format_section() {
    let fixture = placed("format-op", |listener, root| {
        std::thread::spawn(move || {
            layerfs_api::worker::serve::serve(listener, worker_store::SECRET.into(), root)
        });
    });
    let args = SdkValue::map([("store", t("shop"))]);
    let section = route::call(&fixture.endpoint, FORMAT_OP, args, &mut |_| {}).unwrap();
    assert!(section.get_text("status").is_some(), "{section:?}");
    assert_eq!(section.get_int("latest"), Some(3));
    let args = SdkValue::map([("store", t("other"))]);
    let section = route::call(&fixture.endpoint, FORMAT_OP, args, &mut |_| {}).unwrap();
    assert_eq!(section.get_text("status"), Some("unknown"));
    fixture.remove();
}

/// Plain `store get` and `store list` carry no `format`; `--with format`
/// adds it to the get and to every list item.
#[test]
fn only_with_format_adds_the_section() {
    let Some(home) = home_with_shop("plain") else {
        return;
    };
    let plain = get(&home, false).unwrap();
    assert!(plain.get("format").is_none(), "{plain:?}");
    let listed = layerfs_store::admin::status::list::list(
        &home,
        None,
        false,
        layerfs_sandbox::users::reads(),
    )
    .unwrap();
    let SdkValue::List(items) = listed.get("items").unwrap() else {
        panic!("{listed:?}");
    };
    assert!(items.iter().all(|item| item.get("format").is_none()));
    let format = get_format(&home).unwrap();
    for key in ["version", "latest", "status", "fix"] {
        assert!(format.get(key).is_some(), "{key}: {format:?}");
    }
    assert_eq!(list_formats(&home).len(), 1);
}

/// M (shape): each status with its fix; `latest` beside the version.
#[test]
fn the_format_section_names_each_status_and_its_fix() {
    let format = |version, status| Format {
        version,
        latest: 3,
        status,
    };
    let section = layerfs_store::admin::status::formats::section;
    let current = section(&format(Some(3), FormatStatus::Current), "shop", false);
    assert_eq!(
        current,
        SdkValue::map([
            ("version", SdkValue::Int(3)),
            ("latest", SdkValue::Int(3)),
            ("status", t("current")),
            ("fix", SdkValue::Null),
        ])
    );
    let outdated = section(&format(Some(2), FormatStatus::Outdated), "my shop", false);
    assert_eq!(outdated.get_text("status"), Some("outdated"));
    assert_eq!(
        outdated.get_text("fix"),
        Some("layerfs store upgrade 'my shop'")
    );
    let unsupported = section(&format(Some(1), FormatStatus::Unsupported), "shop", false);
    assert_eq!(
        unsupported.get_text("fix"),
        Some("layerfs store remove shop")
    );
    let unsupported = section(&format(None, FormatStatus::Unsupported), "pg", true);
    assert_eq!(unsupported.get("version"), Some(&SdkValue::Null));
    assert_eq!(
        unsupported.get_text("fix"),
        Some("layerfs store disconnect pg")
    );
    let newer = section(&format(Some(4), FormatStatus::Newer), "shop", false);
    assert_eq!(newer.get_text("status"), Some("newer"));
    assert_eq!(newer.get("fix"), Some(&SdkValue::Null));
}

/// A cancel is the internal `Cancelled`, folded to the public `Failed`.
#[test]
fn a_cancel_folds_to_failed() {
    assert_eq!(
        layerfs_home::reply::outcome::public_code("Cancelled"),
        "Failed"
    );
    assert_eq!(
        layerfs_store::upgrade::CANCELLED,
        "cancelled before COMMIT; nothing changed"
    );
}

fn get(home: &Path, format: bool) -> Result<SdkValue, SdkRefusal> {
    let mut sink = |_| {};
    let with = layerfs_store::admin::status::get::With {
        live: false,
        stats: false,
        format,
    };
    layerfs_store::admin::status::get::get(
        home,
        &mut Events::new(&mut sink),
        "shop",
        with,
        layerfs_sandbox::users::reads(),
    )
}

/// `store get shop --with format`'s section.
fn get_format(home: &Path) -> Option<SdkValue> {
    get(home, true).unwrap().get("format").cloned()
}

/// Each `store list --with format` item's section.
fn list_formats(home: &Path) -> Vec<SdkValue> {
    let listed = layerfs_store::admin::status::list::list_with(
        home,
        None,
        false,
        true,
        layerfs_sandbox::users::reads(),
    )
    .unwrap();
    let Some(SdkValue::List(items)) = listed.get("items") else {
        panic!("{listed:?}");
    };
    items
        .iter()
        .map(|item| item.get("format").cloned().expect("a format section"))
        .collect()
}

/// A home with PostgreSQL Store `shop` recorded at `url` (nothing dialled).
fn home_with_pg(tag: &str, url: &str) -> PathBuf {
    let home = scratch(tag).join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    layerfs_home::disk::open(&home).unwrap();
    layerfs_store::backend::postgresql::login::save(&home, "shop", "unused").unwrap();
    StoreRecord::connected("shop", url, "postgres", "layerfs", "off")
        .write(&home)
        .unwrap();
    home
}

/// F (PostgreSQL): the PostgreSQL family's op writer refuses as the
/// sqlite one does, before anything is dialled: another process's lease
/// and this process's own are `InUse`, another operation `Busy`; nothing
/// is left recorded.
#[test]
fn a_postgresql_store_s_upgrade_is_refused_before_anything_is_dialled() {
    // a port nothing listens on: any dial would fail, not refuse InUse
    let home = home_with_pg("pg-refusals", "postgresql://127.0.0.1:9/postgres");
    let mut holder = other_process();
    disk::private_dir(&paths::lease_dir(&home, "shop")).unwrap();
    let lease = paths::lease_dir(&home, "shop").join(holder.id().to_string());
    std::fs::write(&lease, "1\n").unwrap();
    let (outcome, _) = upgrade(&home);
    let _ = holder.kill();
    let _ = holder.wait();
    assert_eq!(outcome.unwrap_err().code, "InUse");
    assert!(op_of(&home).is_none());
    let _ = std::fs::remove_file(&lease);

    let held = layerfs_store::admin::open::lease::hold(&home, "shop").unwrap();
    let (outcome, _) = upgrade(&home);
    drop(held);
    assert_eq!(outcome.unwrap_err().code, "InUse");
    assert!(op_of(&home).is_none());

    let backup = intent::begin(&home, "shop", intent::BACKUP, "upload", SdkValue::Null).unwrap();
    let (outcome, _) = upgrade(&home);
    assert_eq!(outcome.unwrap_err().code, "Busy");
    assert_eq!(op_of(&home).unwrap().id(), backup.id());
    intent::clear(&home, "shop", &backup).unwrap();

    // past the refusals the run reaches the server, which does not answer;
    // whatever it answers, a known outcome leaves no operation and a lost
    // one leaves it unresolved
    let (outcome, _) = upgrade(&home);
    match outcome {
        Err(refusal) if refusal.code == "Unknown" => {
            assert_eq!(op_of(&home).unwrap().standing(), Standing::Unresolved)
        }
        _ => assert!(op_of(&home).is_none()),
    }
    // the format of a server that does not answer is not known
    let format = get_format(&home).unwrap();
    assert_eq!(format.get_text("status"), Some("unknown"), "{format:?}");
    assert_eq!(format.get_int("latest"), Some(4));
}

/// A compaction record beside the Store's path under the home does not
/// refuse an upgrade routed to the Store's machine: the live file is
/// under the worker's root, which the worker checks itself.
#[test]
fn a_compaction_record_under_the_home_does_not_refuse_a_worker_upgrade() {
    let fixture = placed("worker-compact-op", |listener, root| {
        std::thread::spawn(move || {
            layerfs_api::worker::serve::serve(listener, worker_store::SECRET.into(), root)
        });
    });
    let file = paths::store_file(&fixture.home, "shop");
    let record = format!(
        "version=1\nop_id={}\ngeneration=1\nphase=build\nstarted_ms=1\nowner_pid={}\n\
         owner_pid_start=\nowner_pid_boot=\ncatalog_id=00\nnext_history_serial=1\n\
         next_pack_id=1\nnext_ordinal=1\nmark_totals=0,0,0,0\nmark_stamp=0\nhistory_rows=0\n\
         live_heads_digest={}\n",
        "00".repeat(16),
        std::process::id(),
        "00".repeat(32)
    );
    std::fs::write(format!("{}.compact-op", file.display()), record).unwrap();
    // the record reads as a compaction live on this machine
    let busy = layerfs_store::admin::maintenance::compact::refuse_while_running(&file, "shop")
        .unwrap_err();
    assert_eq!(busy.code, "Busy", "{busy:?}");
    let (outcome, _) = upgrade(&fixture.home);
    if let Err(refusal) = &outcome {
        assert!(!refusal.message.contains("being compacted"), "{refusal:?}");
    }
    assert!(op_of(&fixture.home).is_none());
    fixture.remove();
}
