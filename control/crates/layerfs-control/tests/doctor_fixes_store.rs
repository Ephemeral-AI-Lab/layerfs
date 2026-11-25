//! Every finding of the Store and project doctors carries its fix, or no fix
//! with the reason, and redacted evidence (layerfs-project#159). Doctor stays
//! observational: these cases read the report and never run a fix.
use layerfs_bridge::control::SdkValue;
use layerfs_control::doctor::{self, Kind, Options, Thing};
use layerfs_history::HistoryCatalogConfig;
use layerfs_home::events::Events;
use layerfs_home::home::{self, paths, with_field, write_entity};
use layerfs_persistence::{PersistenceConfig, SqliteHandles, SqlitePersistenceProfile};
use layerfs_storage::StoragePolicy;
use std::path::{Path, PathBuf};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
const BINDING: &[u8] = b"doctor-store";
const PASSWORD: &str = "s3cret-pw-4711";

struct Fixture {
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("layerfs-store-fixes-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        home::private_dir(&root).unwrap();
        home::ensure(&root).unwrap();
        Self { root }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn fields(name: &str) -> SdkValue {
    SdkValue::map([
        ("name", SdkValue::text(name)),
        ("kind", SdkValue::text("sqlite")),
        ("machine", SdkValue::text("native")),
        ("binding", SdkValue::Bytes(BINDING.to_vec())),
        ("cursor", SdkValue::Bytes(vec![9; 32])),
    ])
}

fn store_file(path: &Path) -> SqliteHandles {
    home::private_dir(path.parent().unwrap()).unwrap();
    SqliteHandles::create(
        PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Normal),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: [9; 32],
            incarnation: 1,
        },
    )
    .unwrap()
}

fn report(root: &Path, kind: Kind, name: &str) -> SdkValue {
    let mut sink = |_| {};
    let mut events = Events::new(&mut sink);
    doctor::run(
        root,
        Thing::new(kind, name),
        &Options::default(),
        &mut events,
    )
    .unwrap()
    .to_sdk()
}

/// The first finding of `check`.
fn finding(out: &SdkValue, check: &str) -> SdkValue {
    match out.get("findings") {
        Some(SdkValue::List(items)) => items
            .iter()
            .find(|f| f.get_text("check") == Some(check))
            .cloned(),
        _ => None,
    }
    .unwrap_or_else(|| panic!("a finding of {check}: {out:?}"))
}

fn fix_of(finding: &SdkValue) -> &str {
    finding.get_text("fix").unwrap_or("")
}

fn ev_text<'a>(finding: &'a SdkValue, key: &str) -> Option<&'a str> {
    finding.get("evidence")?.get_text(key)
}

fn evidence<'a>(finding: &'a SdkValue, key: &str) -> Option<&'a SdkValue> {
    finding.get("evidence").and_then(|e| e.get(key))
}

#[test]
fn a_missing_file_fails_with_remove_only_when_there_is_no_backup() {
    let f = Fixture::new();
    write_entity(&paths::store_record(&f.root, "one"), &fields("one")).unwrap();
    let out = report(&f.root, Kind::Store, "one");
    let found = finding(&out, "store.connection");
    assert_eq!(fix_of(&found), "layerfs store remove one", "{found:?}");
    let message = found.get_text("message").unwrap_or("");
    assert!(
        message
            .contains("`layerfs store remove` deletes its record and every project record in it"),
        "{found:?}"
    );
    assert_eq!(ev_text(&found, "reason"), Some("missing"));
    assert_eq!(ev_text(&found, "machine"), Some("native"));
    // with a backup link there is no valid in-place restore command either:
    // backup restore protects the recorded Store from replacement it cannot
    // prove unchanged
    let link = SdkValue::map([("file", SdkValue::text("one")), ("heads", SdkValue::Null)]);
    let record = with_field(fields("one"), "backups", SdkValue::map([("r2-main", link)]));
    write_entity(&paths::store_record(&f.root, "one"), &record).unwrap();
    let out = report(&f.root, Kind::Store, "one");
    let found = finding(&out, "store.connection");
    assert_eq!(fix_of(&found), "", "{found:?}");
    assert!(
        found
            .get_text("message")
            .is_some_and(|m| m.contains("restore a backup under a new Store name")),
        "{found:?}"
    );
}

#[test]
fn an_unreadable_record_has_no_fix_and_never_a_store_remove() {
    let f = Fixture::new();
    write_entity(&paths::store_record(&f.root, "one"), &fields("one")).unwrap();
    std::fs::write(paths::store_record(&f.root, "one"), b"{ not json").unwrap();
    let out = report(&f.root, Kind::Store, "one");
    let found = finding(&out, "store.record");
    assert_eq!(found.get_text("severity"), Some("error"), "{found:?}");
    assert_eq!(fix_of(&found), "", "{found:?}");
    assert!(ev_text(&found, "code").is_some(), "{found:?}");
}

#[test]
fn a_file_of_another_store_has_no_in_place_restore_fix() {
    let f = Fixture::new();
    let writer = store_file(&paths::store_file(&f.root, "two"));
    let record = with_field(
        fields("two"),
        "binding",
        SdkValue::Bytes(b"another-store".to_vec()),
    );
    write_entity(&paths::store_record(&f.root, "two"), &record).unwrap();
    let out = report(&f.root, Kind::Store, "two");
    let found = finding(&out, "store.identity");
    assert_eq!(fix_of(&found), "", "{found:?}");
    assert_eq!(ev_text(&found, "reason"), Some("id_differs"));
    drop(writer);
}

#[test]
fn a_file_with_another_schema_has_no_in_place_restore_fix() {
    let f = Fixture::new();
    let path = paths::store_file(&f.root, "three");
    drop(store_file(&path));
    write_entity(&paths::store_record(&f.root, "three"), &fields("three")).unwrap();
    let broken = std::process::Command::new("python3")
        .arg("-I")
        .arg("-c")
        .arg("import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); c.execute('PRAGMA user_version=999'); c.commit()")
        .arg(&path)
        .status();
    if !broken.is_ok_and(|status| status.success()) {
        eprintln!("NOT_RUN: python3 with sqlite3 is not available");
        return;
    }
    let out = report(&f.root, Kind::Store, "three");
    let found = finding(&out, "store.schema");
    assert_eq!(fix_of(&found), "", "{found:?}");
    assert!(
        found
            .get_text("message")
            .is_some_and(|m| m.contains("build that wrote it")),
        "{found:?}"
    );
    assert_eq!(ev_text(&found, "reason"), Some("schema_unreadable"));
}

#[test]
fn a_record_of_another_name_has_no_fix_and_says_why() {
    let f = Fixture::new();
    write_entity(&paths::store_record(&f.root, "one"), &fields("two")).unwrap();
    let out = report(&f.root, Kind::Store, "one");
    let found = finding(&out, "store.record");
    assert_eq!(fix_of(&found), "", "{found:?}");
    assert_eq!(
        evidence(&found, "name_matches"),
        Some(&SdkValue::Bool(false))
    );
    assert_eq!(
        evidence(&found, "version_matches"),
        Some(&SdkValue::Bool(true))
    );
}

#[test]
fn an_unsupported_engine_keeps_its_fix_and_gains_evidence() {
    let f = Fixture::new();
    let record = with_field(fields("one"), "kind", SdkValue::text("duckdb"));
    write_entity(&paths::store_record(&f.root, "one"), &record).unwrap();
    let out = report(&f.root, Kind::Store, "one");
    let found = finding(&out, "store.record");
    assert_eq!(fix_of(&found), "layerfs store remove one", "{found:?}");
    assert_eq!(ev_text(&found, "reason"), Some("engine_unsupported"));
}

#[test]
fn a_project_record_that_cannot_be_read_says_why_without_a_fix() {
    let f = Fixture::new();
    let writer = store_file(&paths::store_file(&f.root, "one"));
    write_entity(&paths::store_record(&f.root, "one"), &fields("one")).unwrap();
    let record = paths::project_record(&f.root, "one", "app");
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    std::fs::write(&record, b"not json").unwrap();
    let out = report(&f.root, Kind::Project, "one/app");
    let found = finding(&out, "project.record");
    assert_eq!(fix_of(&found), "", "{found:?}");
    assert!(evidence(&found, "code").is_some(), "{found:?}");
    drop(writer);
}

#[test]
fn stale_leases_get_their_own_removal_command_not_a_read() {
    let f = Fixture::new();
    write_entity(&paths::store_record(&f.root, "one"), &fields("one")).unwrap();
    let lease_dir = paths::lease_dir(&f.root, "one");
    home::private_dir(&lease_dir).unwrap();
    let lease = lease_dir.join("99999999");
    std::fs::write(&lease, "1\n").unwrap();
    let out = report(&f.root, Kind::Store, "one");
    let found = finding(&out, "store.operation");
    assert_eq!(
        fix_of(&found),
        format!("rm {}", lease.display()),
        "{found:?}"
    );
    assert_eq!(
        evidence(&found, "stale_pids"),
        Some(&SdkValue::List(vec![SdkValue::Int(99999999)]))
    );
    assert!(
        !fix_of(&found).contains("store get"),
        "a read does not remove stale leases: {found:?}"
    );
}

#[test]
fn a_postgresql_store_that_is_not_reached_names_the_connect_fix_and_hides_its_password() {
    let f = Fixture::new();
    let record = SdkValue::map([
        ("name", SdkValue::text("pg")),
        ("kind", SdkValue::text("postgresql")),
        ("url", SdkValue::text("postgresql://127.0.0.1:1/layerfs")),
        ("user", SdkValue::text("layerfs_pg")),
        ("schema", SdkValue::text("layerfs")),
        ("synchronous_commit", SdkValue::text("local")),
        ("binding", SdkValue::Bytes(BINDING.to_vec())),
        ("cursor", SdkValue::Bytes(vec![9; 32])),
    ]);
    write_entity(&paths::store_record(&f.root, "pg"), &record).unwrap();
    layerfs_store::postgresql::login::save(&f.root, "pg", PASSWORD).unwrap();
    let out = report(&f.root, Kind::Store, "pg");
    let found = finding(&out, "store.connection");
    assert!(
        fix_of(&found).starts_with("layerfs store connect pg <url>"),
        "{found:?}"
    );
    assert!(evidence(&found, "step").is_some(), "{found:?}");
    let shown = format!("{out:?}");
    assert!(!shown.contains(PASSWORD), "{shown}");
    assert!(!shown.contains("127.0.0.1:1/layerfs"), "{shown}");
}
