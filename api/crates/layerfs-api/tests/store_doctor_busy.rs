//! A sqlite Store whose file another process holds with an exclusive lock
//! (UI pass 2, LF4 item d): `store doctor` reaches the file, and its id and
//! schema are `not_checked` with reason `busy`, never a `store.schema` (or
//! `store.identity`) error; once the lock goes they pass. Records only plus
//! one real Store file on `native`; nothing is written to the Store.
use layerfs_api::home::{self, paths, write_entity};
use layerfs_bridge::control::SdkValue;
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{PersistenceConfig, SqliteHandles, SqlitePersistenceProfile};
use layerfs_storage::StoragePolicy;
use std::path::{Path, PathBuf};

const BINDING: &[u8] = b"doctor-busy";

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("layerfs-sdb-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn list<'a>(value: &'a SdkValue, key: &str) -> &'a [SdkValue] {
    match value.get(key) {
        Some(SdkValue::List(items)) => items,
        _ => &[],
    }
}

fn doctor(home: &Path, detail: bool) -> SdkValue {
    let args = SdkValue::map([
        ("store", SdkValue::text("held")),
        ("detail", SdkValue::Bool(detail)),
    ]);
    layerfs_api::call(Some(home), "store.doctor", args, &mut |_| {})
        .unwrap_or_else(|r| panic!("{}: {}", r.code, r.message))
}

/// `(check, outcome)` of each row of the Store's node.
fn rows(long: &SdkValue) -> Vec<(String, String)> {
    list(long.get("tree").unwrap(), "checks")
        .iter()
        .map(|row| {
            let text = |key: &str| row.get_text(key).unwrap_or("").to_owned();
            (text("check"), text("outcome"))
        })
        .collect()
}

#[test]
fn a_store_another_writer_holds_is_busy_not_a_schema_error() {
    let home = tempdir("home");
    layerfs_api::call(
        Some(&home),
        "home.init",
        SdkValue::Map(Vec::new()),
        &mut |_| {},
    )
    .unwrap();
    let file = paths::store_file(&home, "held");
    home::private_dir(file.parent().unwrap()).unwrap();
    drop(
        SqliteHandles::create(
            PersistenceConfig::sqlite(&file)
                .with_sqlite_profile(SqlitePersistenceProfile::Normal),
            StoragePolicy::frozen_default(),
            &HistoryCatalogConfig {
                binding_key: BINDING.to_vec(),
                cursor_key: [9; 32],
                incarnation: 1,
            },
        )
        .unwrap(),
    );
    write_entity(
        &paths::store_record(&home, "held"),
        &SdkValue::map([
            ("name", SdkValue::text("held")),
            ("kind", SdkValue::text("sqlite")),
            ("machine", SdkValue::text("native")),
            ("binding", SdkValue::Bytes(BINDING.to_vec())),
            ("cursor", SdkValue::Bytes(vec![9; 32])),
        ]),
    )
    .unwrap();

    // another writer holds the file exclusively
    let holder = rusqlite::Connection::open(&file).unwrap();
    holder
        .execute_batch("PRAGMA locking_mode=EXCLUSIVE; BEGIN EXCLUSIVE;")
        .unwrap();
    let long = doctor(&home, true);
    let short = doctor(&home, false);
    let checks = rows(&long);
    let outcome = |id: &str| {
        (checks.iter())
            .find(|(check, _)| check == id)
            .map(|(_, outcome)| outcome.as_str())
    };
    assert_eq!(outcome("store.connection"), Some("pass"), "{long:?}");
    for id in ["store.identity", "store.schema"] {
        assert_ne!(outcome(id), Some("error"), "{id}: {long:?}");
        let open = (list(&long, "not_checked").iter())
            .find(|entry| entry.get_text("check") == Some(id))
            .unwrap_or_else(|| panic!("{id} is not a not_checked entry: {long:?}"));
        assert_eq!(open.get_text("reason"), Some("busy"), "{open:?}");
        assert!(
            (list(&short, "not_checked").iter()).any(|entry| entry.get_text("check") == Some(id)
                && entry.get_text("reason") == Some("busy")),
            "{short:?}"
        );
    }
    assert!(list(&long, "findings").is_empty(), "{long:?}");

    // the lock goes: both pass
    drop(holder);
    let long = doctor(&home, true);
    let checks = rows(&long);
    for id in ["store.identity", "store.schema"] {
        assert!(
            checks.contains(&(id.to_owned(), "pass".to_owned())),
            "{id}: {long:?}"
        );
    }
    let _ = std::fs::remove_dir_all(home);
}
