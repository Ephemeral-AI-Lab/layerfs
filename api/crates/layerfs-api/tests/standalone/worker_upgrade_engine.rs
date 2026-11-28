//! `store.upgrade` end to end through the engine (#804 §11 B, H, K, M).
//!
//! **Needs W1's engine** (`layerfs_persistence::upgrade`): on a branch where
//! its entry points are the skeleton's stubs ("not implemented") every test
//! here fails; they run at integration. The SQLite files of other formats
//! for the `format` display (M) are copies of a current Store whose header
//! `user_version` is rewritten: a display test, never an upgrade test.
#[path = "../support/root.rs"]
mod root;
#[path = "../support/worker_store.rs"]
mod worker_store;

use layerfs_home::disk::paths;
use layerfs_home::names::StoreName;
use layerfs_home::reply::events::Events;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_store::admin::record::store::StoreRecord;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use worker_store::{placed, scratch};

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

/// Rewrites the header `user_version` (4 bytes, big-endian, at offset 60)
/// of the closed SQLite file at `path`.
fn set_user_version(path: &Path, version: u32) {
    let mut bytes = std::fs::read(path).unwrap();
    bytes[60..64].copy_from_slice(&version.to_be_bytes());
    std::fs::write(path, bytes).unwrap();
}

fn upgrade_with(home: &Path, cancel: &AtomicBool) -> (Result<SdkValue, SdkRefusal>, Vec<String>) {
    let mut seen = Vec::new();
    let mut sink = |event: SdkValue| {
        if let Some(phase) = event.get("progress").and_then(|p| p.get_text("phase")) {
            seen.push(phase.to_owned());
        }
    };
    let outcome = layerfs_store::upgrade::upgrade_with(
        home,
        &mut Events::new(&mut sink),
        "shop",
        cancel,
        layerfs_store::StoreUsers::related(&layerfs_sandbox::users::STORE),
    );
    (outcome, seen)
}

fn get_format(home: &Path) -> SdkValue {
    let mut sink = |_| {};
    let with = layerfs_store::admin::status::get::With {
        live: false,
        stats: false,
        format: true,
    };
    let got = layerfs_store::admin::status::get::get(
        home,
        &mut Events::new(&mut sink),
        "shop",
        with,
        layerfs_sandbox::users::reads(),
    )
    .unwrap();
    got.get("format").cloned().unwrap()
}

/// B (current, needs W1): a current Store answers `current`, every key
/// present, nothing written to the file, the operation dropped.
#[test]
fn needs_w1_a_current_store_answers_current_and_changes_nothing() {
    let Some(home) = home_with_shop("current") else {
        return;
    };
    let before = std::fs::read(paths::store_file(&home, "shop")).unwrap();
    let (outcome, phases) = upgrade_with(&home, &AtomicBool::new(false));
    let answer = outcome.expect("a current Store is current");
    assert_eq!(answer.get_text("outcome"), Some("current"), "{answer:?}");
    assert_eq!(answer.get_text("store"), Some("shop"));
    assert_eq!(answer.get_text("engine"), Some("sqlite"));
    assert_eq!(answer.get_int("format_before"), Some(3));
    assert_eq!(answer.get_int("format_after"), Some(3));
    assert_eq!(answer.get("steps"), Some(&SdkValue::List(Vec::new())));
    assert_eq!(answer.get("warnings"), Some(&SdkValue::List(Vec::new())));
    assert!(phases.first().is_none_or(|p| p == "check"), "{phases:?}");
    assert!(StoreRecord::of(&home, "shop").unwrap().op().is_none());
    assert_eq!(
        std::fs::read(paths::store_file(&home, "shop")).unwrap(),
        before
    );
}

/// H (needs W1): a cancel asked for before the run is `Failed` with
/// `detail.code` `Cancelled` at the boundary (internal code `Cancelled`),
/// the file unchanged and the operation dropped.
#[test]
fn needs_w1_a_cancel_before_commit_changes_nothing() {
    let Some(home) = home_with_shop("cancel") else {
        return;
    };
    let file = paths::store_file(&home, "shop");
    // an outdated header, so there is work to cancel
    set_user_version(&file, 36);
    let before = std::fs::read(&file).unwrap();
    let (outcome, _) = upgrade_with(&home, &AtomicBool::new(true));
    let refusal = outcome.unwrap_err();
    assert_eq!(refusal.code, "Cancelled", "{refusal:?}");
    assert_eq!(refusal.message, "cancelled before COMMIT; nothing changed");
    assert_eq!(
        layerfs_home::reply::outcome::public_code(&refusal.code),
        "Failed"
    );
    assert!(StoreRecord::of(&home, "shop").unwrap().op().is_none());
    assert_eq!(std::fs::read(&file).unwrap(), before);
}

/// M (needs W1): `--with format` for a current, an outdated, an
/// unsupported and a newer Store, each a Store this build refuses to open
/// but the current one; `--with stats` (which opens it) fails on them.
#[test]
fn needs_w1_the_format_section_reads_every_status() {
    let Some(home) = home_with_shop("formats") else {
        return;
    };
    let file = paths::store_file(&home, "shop");
    let current = get_format(&home);
    assert_eq!(current.get_int("version"), Some(3), "{current:?}");
    assert_eq!(current.get_text("status"), Some("current"));
    assert_eq!(current.get("fix"), Some(&SdkValue::Null));
    for (raw, version, status, fix) in [
        (36, Some(2), "outdated", Some("layerfs store upgrade shop")),
        (
            16,
            Some(1),
            "unsupported",
            Some("layerfs store remove shop"),
        ),
        (200, None, "newer", None),
    ] {
        set_user_version(&file, raw);
        let format = get_format(&home);
        assert_eq!(format.get_text("status"), Some(status), "{raw}: {format:?}");
        assert_eq!(format.get_int("latest"), Some(3));
        if let Some(version) = version {
            assert_eq!(format.get_int("version"), Some(version), "{format:?}");
        }
        assert_eq!(format.get_text("fix"), fix, "{format:?}");
        // the open this build refuses
        let mut sink = |_| {};
        let with = layerfs_store::admin::status::get::With {
            live: false,
            stats: true,
            format: false,
        };
        let stats = layerfs_store::admin::status::get::get(
            &home,
            &mut Events::new(&mut sink),
            "shop",
            with,
            layerfs_sandbox::users::reads(),
        );
        assert!(stats.is_err(), "{raw}: {stats:?}");
        layerfs_store::close_idle(std::time::Duration::ZERO);
    }
}

/// M through a worker (needs W1): `store get --with format` reads the
/// file under the worker's root with `store.format`.
#[test]
fn needs_w1_the_format_of_a_store_on_a_machine_is_read_by_its_worker() {
    let fixture = placed("worker-format", |listener, root| {
        std::thread::spawn(move || {
            layerfs_api::worker::serve::serve(listener, worker_store::SECRET.into(), root)
        });
    });
    set_user_version(&paths::store_file(&fixture.root, "shop"), 36);
    let format = get_format(&fixture.home);
    assert_eq!(format.get_text("status"), Some("outdated"), "{format:?}");
    assert_eq!(format.get_int("version"), Some(2));
    fixture.remove();
}

/// The worker route (needs W1): a current Store on a machine is upgraded
/// by its worker, which sends its phases back; the caller's operation is
/// dropped and no lease is left.
#[test]
fn needs_w1_a_store_on_a_machine_is_upgraded_by_its_worker() {
    let fixture = placed("worker-upgrade", |listener, root| {
        std::thread::spawn(move || {
            layerfs_api::worker::serve::serve(listener, worker_store::SECRET.into(), root)
        });
    });
    let (outcome, phases) = upgrade_with(&fixture.home, &AtomicBool::new(false));
    let answer = outcome.expect("the worker upgrades the Store");
    assert_eq!(answer.get_text("outcome"), Some("current"), "{answer:?}");
    assert!(phases.first().is_none_or(|p| p == "check"), "{phases:?}");
    assert!(StoreRecord::of(&fixture.home, "shop")
        .unwrap()
        .op()
        .is_none());
    fixture.remove();
}

/// K (needs W1): a backup of a schema version 1 Store is refused by name,
/// in the engine's words, and nothing is installed.
#[test]
fn needs_w1_a_schema_version_1_backup_is_refused_by_name() {
    use layerfs_store::backup::bucket::{pieces, Limits, Metadata};
    use layerfs_store::backup::providers::local_folder::LocalFolder;
    let Some(home) = home_with_shop("restore-v1") else {
        return;
    };
    let record = StoreRecord::of(&home, "shop").unwrap();
    let identity = layerfs_store::admin::record::identity::identity_in(&record).unwrap();
    let copy = home.join("copy.sqlite");
    std::fs::copy(paths::store_file(&home, "shop"), &copy).unwrap();
    set_user_version(&copy, 16);
    let remote = home.join("remote");
    std::fs::create_dir_all(&remote).unwrap();
    let limits = Limits {
        max_file: 1 << 30,
        max_request: 1 << 30,
        piece_size: 1 << 30,
    };
    let driver = LocalFolder::new(&remote, limits).unwrap();
    let binding = layerfs_store::backup::bucket::pieces::hex(&identity.binding);
    let metadata = Metadata::from([(
        layerfs_store::backup::copy::object::BINDING.to_owned(),
        binding,
    )]);
    pieces::upload(&driver, "old/store.sqlite", &copy, &metadata).unwrap();
    let restored = home.join("restored");
    std::fs::create_dir_all(&restored).unwrap();
    std::fs::set_permissions(&restored, std::fs::Permissions::from_mode(0o700)).unwrap();
    let destination = restored.join("store.sqlite");
    let failure =
        layerfs_store::backup::copy::load(&driver, "old", &destination, identity.cursor_key)
            .unwrap_err();
    assert!(
        failure.cause.contains("schema version 1"),
        "{}",
        failure.cause
    );
    assert!(!failure.cause.contains("store_blocks"), "{}", failure.cause);
    assert!(!destination.exists());
}

/// PostgreSQL (needs W1 and `LAYERFS_TEST_POSTGRES_URL`, the URL form): a
/// current Store in its own schema answers `current` with the PostgreSQL
/// phases' first, its format section is `current` at 4, and the operation
/// is dropped.
#[test]
fn needs_w1_a_current_postgresql_store_answers_current() {
    use layerfs_history::HistoryCatalogConfig;
    use layerfs_persistence::{PostgresHandles, PostgresSelection};
    use layerfs_storage::StoragePolicy;
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
        return;
    };
    let schema = format!("upg_{}", std::process::id());
    let handles = PostgresHandles::create(
        &PostgresSelection::new(&url, &schema),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"w2-804-upgrade".to_vec(),
            cursor_key: [3; 32],
            incarnation: 1,
        },
    )
    .unwrap();
    drop(handles);
    let home = scratch("pg-current").join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    layerfs_home::disk::open(&home).unwrap();
    // a trust server: the URL's user is the login, any password passes
    let (scheme, after) = url.split_once("://").unwrap();
    let (user, address) = after.split_once('@').unwrap();
    let address = format!("{scheme}://{address}");
    layerfs_store::backend::postgresql::login::save(&home, "shop", "unused").unwrap();
    StoreRecord::connected("shop", &address, user, &schema, "off")
        .with("binding", SdkValue::Bytes(b"w2-804-upgrade".to_vec()))
        .with("cursor", SdkValue::Bytes([3; 32].to_vec()))
        .write(&home)
        .unwrap();
    let (outcome, phases) = upgrade_with(&home, &AtomicBool::new(false));
    let answer = outcome.expect("a current PostgreSQL Store is current");
    assert_eq!(answer.get_text("outcome"), Some("current"), "{answer:?}");
    assert_eq!(answer.get_text("engine"), Some("postgresql"));
    assert_eq!(answer.get_int("format_after"), Some(4));
    assert!(phases.first().is_none_or(|p| p == "check"), "{phases:?}");
    assert!(StoreRecord::of(&home, "shop").unwrap().op().is_none());
    let format = get_format(&home);
    assert_eq!(format.get_text("status"), Some("current"), "{format:?}");
    assert_eq!(format.get_int("version"), Some(4));
}

/// The captured schema 2 SQLite fixture (`fixtures/store-formats`), never
/// edited: tests copy it.
fn fixture_v2() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../fixtures/store-formats/sqlite/v2/store.sqlite")
}

/// The binding key the closed SQLite file at `path` records, read with the
/// system `sqlite3` (test-only; `None` where it is not installed).
fn binding_of(path: &Path) -> Option<Vec<u8>> {
    let uri = format!("file:{}?immutable=1", path.display());
    let out = std::process::Command::new("sqlite3")
        .args([&uri, "SELECT hex(binding_key) FROM history_meta"])
        .output()
        .ok()?;
    let hex = String::from_utf8(out.stdout).ok()?;
    let hex = hex.trim();
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}

/// M with the captured fixture (needs W1): a real schema 2 Store reads
/// `outdated`, version 2, fix `layerfs store upgrade shop`.
#[test]
fn needs_w1_the_captured_schema_2_store_reads_outdated() {
    let Some(home) = home_with_shop("fixture-format") else {
        return;
    };
    std::fs::copy(fixture_v2(), paths::store_file(&home, "shop")).unwrap();
    let format = get_format(&home);
    assert_eq!(format.get_text("status"), Some("outdated"), "{format:?}");
    assert_eq!(format.get_int("version"), Some(2));
    assert_eq!(format.get_text("fix"), Some("layerfs store upgrade shop"));
}

/// K (needs W1): a backup of the captured schema 2 Store restores through
/// the chain: the download is upgraded (`2->3`) before it opens, and the
/// installed Store is current.
#[test]
fn needs_w1_a_schema_2_backup_restores_through_the_chain() {
    use layerfs_store::backup::bucket::{pieces, Limits, Metadata};
    use layerfs_store::backup::providers::local_folder::LocalFolder;
    let scratch = scratch("restore-v2");
    let Some(binding) = binding_of(&fixture_v2()) else {
        eprintln!("NOT RUN: no sqlite3 to read the fixture's binding");
        return;
    };
    let copy = scratch.join("copy.sqlite");
    std::fs::copy(fixture_v2(), &copy).unwrap();
    let remote = scratch.join("remote");
    std::fs::create_dir_all(&remote).unwrap();
    let limits = Limits {
        max_file: 1 << 30,
        max_request: 1 << 30,
        piece_size: 1 << 30,
    };
    let driver = LocalFolder::new(&remote, limits).unwrap();
    let metadata = Metadata::from([(
        layerfs_store::backup::copy::object::BINDING.to_owned(),
        pieces::hex(&binding),
    )]);
    pieces::upload(&driver, "fx/store.sqlite", &copy, &metadata).unwrap();
    let restored = scratch.join("restored");
    std::fs::create_dir_all(&restored).unwrap();
    std::fs::set_permissions(&restored, std::fs::Permissions::from_mode(0o700)).unwrap();
    let destination = restored.join("store.sqlite");
    let loaded = layerfs_store::backup::copy::load(&driver, "fx", &destination, [5; 32])
        .unwrap_or_else(|failure| panic!("{failure}"));
    assert_eq!(loaded.upgrade.outcome.word(), "upgraded");
    assert_eq!(loaded.upgrade.steps, ["2->3"]);
    assert!(loaded.heads.is_some());
    let format = layerfs_persistence::upgrade::sqlite_format(&destination).unwrap();
    assert_eq!(format.version, Some(3));
}

/// `store list --with stats --with format` (needs W1): a Store whose size
/// cannot be read (a sidecar that is a symlink loop) does not blank the
/// format of the next Store on the same machine.
#[test]
fn needs_w1_a_failed_size_does_not_blank_later_formats() {
    let Some(home) = home_with_shop("list-size") else {
        return;
    };
    let name = StoreName::parse("a").unwrap();
    layerfs_store::backend::sqlite::create::create(&home, &name, None).unwrap();
    let wal = format!("{}-wal", paths::store_file(&home, "a").display());
    let _ = std::fs::remove_file(&wal);
    std::os::unix::fs::symlink(&wal, &wal).unwrap();
    let listed = layerfs_store::admin::status::list::list_with(
        &home,
        None,
        true,
        true,
        layerfs_sandbox::users::reads(),
    )
    .unwrap();
    let Some(SdkValue::List(items)) = listed.get("items") else {
        panic!("{listed:?}");
    };
    let item = |name: &str| {
        (items.iter())
            .find(|item| item.get_text("store") == Some(name))
            .unwrap()
    };
    // the size read of `a` failed (not an unreached machine)
    assert_eq!(item("a").get("size"), Some(&SdkValue::Null));
    let shop = item("shop");
    let format = shop.get("format").unwrap();
    assert_eq!(format.get_text("status"), Some("current"), "{format:?}");
}
