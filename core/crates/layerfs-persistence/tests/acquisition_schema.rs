//! Acquisition tables are a creation-time schema selection with exact validation.
#![cfg(target_os = "macos")]
mod support;
use layerfs_content::ObjectId;
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{
    Handles, PersistenceConfig, SqliteAcquisitionSchema, SqlitePackLayout, SqlitePersistenceProfile,
};
use layerfs_storage::{
    port::{
        acquisition::{Acquisition, AcquisitionError, Begin},
        PersistenceError,
    },
    StoragePolicy,
};
use support::Temp;

const BINDING: &[u8] = b"acquisition-schema";

fn history() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: BINDING.to_vec(),
        cursor_key: [61; 32],
        incarnation: 1,
    }
}

fn config(path: &std::path::Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable)
}

fn begin() -> Begin {
    Begin {
        source_device: u64::MAX,
        source_inode: 1 << 63,
        stack: [7; 16],
        scope: ObjectId::for_bytes(b"scope"),
    }
}

fn tables(path: &std::path::Path) -> (i64, Vec<String>) {
    let db = rusqlite::Connection::open(path).unwrap();
    let version = db
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .unwrap();
    let mut statement = db
        .prepare("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name")
        .unwrap();
    let names = statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    (version, names)
}

const ACQUISITION_TABLES: [&str; 4] = [
    "init_entry",
    "init_native_file",
    "init_operation",
    "sqlite_sequence",
];

#[test]
fn each_pack_layout_has_one_version_with_and_one_without_the_tables() {
    for (layout, base) in [
        (SqlitePackLayout::Monolithic, 1),
        (SqlitePackLayout::GroupRows, 2),
        (SqlitePackLayout::GroupRowsIndexed, 3),
    ] {
        let temp = Temp::new("acquisition-versions");
        let (plain, selected) = (temp.join("plain.sqlite"), temp.join("selected.sqlite"));
        let created = Handles::create(
            config(&plain).with_sqlite_pack_layout(layout),
            StoragePolicy::frozen_default(),
            &history(),
        )
        .unwrap();
        assert_eq!(
            created.profile().acquisition,
            SqliteAcquisitionSchema::Absent
        );
        // No unit runs, and no SQL is issued, on a Store without the tables.
        let before = created.diagnostics().unwrap().statements;
        assert_eq!(
            created.acquisition.begin(&begin()),
            Err(AcquisitionError::Persistence(
                PersistenceError::BackendUnavailable
            ))
        );
        assert_eq!(
            created.explain_acquisition().unwrap_err(),
            PersistenceError::BackendUnavailable
        );
        assert_eq!(created.diagnostics().unwrap().statements, before);
        drop(created);
        let (version, names) = tables(&plain);
        assert_eq!(version, base);
        assert!(!names.iter().any(|name| name.starts_with("init_")));
        assert!(!names.iter().any(|name| name == "sqlite_sequence"));

        let created = Handles::create(
            config(&selected)
                .with_sqlite_pack_layout(layout)
                .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
            StoragePolicy::frozen_default(),
            &history(),
        )
        .unwrap();
        assert_eq!(
            created.profile().acquisition,
            SqliteAcquisitionSchema::Tables
        );
        let owner = created.acquisition.begin(&begin()).unwrap();
        assert_eq!((owner.operation, owner.epoch), (1, 1));
        created.acquisition.release(owner).unwrap();
        drop(created);
        let (version, names) = tables(&selected);
        assert_eq!(version, base + 3);
        for table in ACQUISITION_TABLES {
            assert!(
                names.iter().any(|name| name == table),
                "{table} in {names:?}"
            );
        }

        // Reopen follows the stored version whatever the configuration selects.
        let reopened = Handles::open_writable(config(&selected), BINDING, [61; 32]).unwrap();
        assert_eq!(reopened.profile().pack_layout, layout);
        assert_eq!(
            reopened.profile().acquisition,
            SqliteAcquisitionSchema::Tables
        );
        let owner = reopened.acquisition.begin(&begin()).unwrap();
        assert_eq!(
            owner.operation, 2,
            "a released identity is not issued again"
        );
        assert_eq!(owner.epoch, 2, "a new session is a new epoch");
        reopened.acquisition.release(owner).unwrap();
        drop(reopened);
        let plain_again = Handles::open_writable(
            config(&plain).with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
            BINDING,
            [61; 32],
        )
        .unwrap();
        assert_eq!(
            plain_again.profile().acquisition,
            SqliteAcquisitionSchema::Absent,
            "opening never adds the tables"
        );
        drop(plain_again);
        assert_eq!(tables(&plain).0, base);
    }
}

#[test]
fn a_read_only_store_refuses_to_begin() {
    let temp = Temp::new("acquisition-read-only");
    let path = temp.join("store.sqlite");
    drop(
        Handles::create(
            config(&path).with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
            StoragePolicy::frozen_default(),
            &history(),
        )
        .unwrap(),
    );
    let read_only = Handles::open_read_only(config(&path), BINDING, [61; 32]).unwrap();
    let refused = read_only.acquisition.begin(&begin()).unwrap_err();
    assert!(
        matches!(
            &refused,
            AcquisitionError::Persistence(PersistenceError::Refused { status }) if status == "ReadOnly"
        ),
        "{refused:?}"
    );
    assert!(read_only.acquisition.abandoned(None, 8).unwrap().is_empty());
}

#[test]
fn a_changed_acquisition_definition_is_refused_at_open() {
    for change in [
        "DROP INDEX init_native_file_identity",
        "CREATE TABLE init_extra (id INTEGER PRIMARY KEY) STRICT",
        "PRAGMA user_version=1",
    ] {
        let temp = Temp::new("acquisition-tampered");
        let path = temp.join("store.sqlite");
        drop(
            Handles::create(
                config(&path).with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
                StoragePolicy::frozen_default(),
                &history(),
            )
            .unwrap(),
        );
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute_batch(change)
            .unwrap();
        let refused = Handles::open_writable(config(&path), BINDING, [61; 32])
            .err()
            .unwrap_or_else(|| panic!("{change} must be refused"));
        assert_eq!(refused, PersistenceError::Malformed, "{change}");
    }
}
