//! Incremental change export: a replica built from a marked base and the
//! bundles exported from the live Store holds exactly the live Store's rows,
//! opens through the normal open path and answers the same history. Bundles
//! apply only in order, only to an unwritten replica, and only intact.
mod support;
use layerfs_content::{construct_bytes, ObjectId};
use layerfs_history::records::{
    BindingHint, CommitMeta, CommitOutcome, CommitRequest, CommitVia, ForkRequest, ForkSource,
    StackInitialization,
};
use layerfs_history::{BranchId, HistoryCatalog, HistoryCatalogConfig};
use layerfs_persistence::replication::{
    apply_changes, export_changes, mark_base, read_state, ChangeError,
};
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePackLayout, SqlitePersistenceProfile};
use layerfs_storage::port::PersistenceError;
use layerfs_storage::{Storage, StoragePolicy};
use layerfs_telemetry::timer::Timing;
use support::*;

/// Every logged table; `pack_unit` exists only in the group-row layouts.
const TABLES: [&str; 19] = [
    "pack",
    "pack_unit",
    "object_location",
    "inode_value",
    "content_signature",
    "store_policy",
    "store_totals",
    "history_meta",
    "layer_stack",
    "layer",
    "commit",
    "branch",
    "scope_allocator",
    "binding_hint",
    "layer_lineage",
    "commit_lineage",
    "absorbed",
    "reach",
    "file_digest",
];

fn config(path: &std::path::Path, layout: SqlitePackLayout) -> PersistenceConfig {
    PersistenceConfig::sqlite(path)
        .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
        .with_sqlite_pack_layout(layout)
}

fn created(path: &std::path::Path, layout: SqlitePackLayout) -> Handles {
    Handles::create(
        config(path, layout),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            cursor_key: [71; 32],
            binding_key: BINDING.to_vec(),
            incarnation: 7,
        },
    )
    .unwrap()
}

fn writable(path: &std::path::Path) -> Handles {
    writable_in(path, SqlitePackLayout::Monolithic)
}

fn writable_in(path: &std::path::Path, layout: SqlitePackLayout) -> Handles {
    Handles::open_writable(config(path, layout), BINDING, [71; 32]).unwrap()
}

fn save(storage: &Storage, bytes: &[u8]) -> ObjectId {
    let save = storage.begin_save().unwrap();
    let result = Timing::disabled("construct", |scope| {
        construct_bytes(
            storage.policy().construction(),
            &storage.policy().construction().capacities(),
            bytes,
            &mut save.sink(),
            scope.child("content"),
        )
    })
    .0
    .unwrap();
    save.finish().unwrap();
    result.root
}

fn commit(history: &dyn HistoryCatalog, branch: BranchId, tag: u8) {
    let record = history.branch(branch).unwrap().unwrap();
    let current = match record.head_commit {
        Some(head) => history.commit(head).unwrap().unwrap().root,
        None => history.layer(record.base_layer).unwrap().unwrap().root,
    };
    let outcome = history
        .commit_branch(&CommitRequest {
            branch,
            expected_head: record.head_commit,
            expected_base: record.base_layer,
            expected_root: current,
            candidate_root: root(tag),
            profile: root(201),
            scope: root(200),
            second_parent: None,
            meta: CommitMeta::new(CommitVia::Files),
            force: false,
        })
        .unwrap();
    assert!(
        matches!(outcome, CommitOutcome::Committed { .. }),
        "{outcome:?}"
    );
}

/// Every row of every logged table the Store has, each rendered as text,
/// sorted.
fn rows(path: &std::path::Path) -> Vec<(&'static str, Vec<String>)> {
    let sql =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let exists = |table: &str| {
        sql.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name=?1",
            [table],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
            == 1
    };
    TABLES
        .iter()
        .filter(|table| exists(table))
        .map(|table| {
            let mut statement = sql.prepare(&format!("SELECT * FROM \"{table}\"")).unwrap();
            let width = statement.column_count();
            let mut out: Vec<String> = statement
                .query_map([], |r| {
                    (0..width)
                        .map(|i| r.get::<_, rusqlite::types::Value>(i))
                        .collect::<Result<Vec<_>, _>>()
                })
                .unwrap()
                .map(|row| format!("{:?}", row.unwrap()))
                .collect();
            out.sort();
            (*table, out)
        })
        .collect()
}

fn log_entries(path: &std::path::Path) -> i64 {
    let sql = rusqlite::Connection::open(path).unwrap();
    sql.query_row("SELECT count(*) FROM change_log WHERE tbl>0", [], |r| {
        r.get(0)
    })
    .unwrap()
}

/// Creates a Store with one LayerStack and one saved object, closes it and
/// returns the open-ready path; the base is not yet marked.
fn initialized(t: &Temp, file: &str) -> std::path::PathBuf {
    initialized_in(t, file, SqlitePackLayout::Monolithic)
}

fn initialized_in(t: &Temp, file: &str, layout: SqlitePackLayout) -> std::path::PathBuf {
    let path = t.join(file);
    let handles = created(&path, layout);
    handles
        .history
        .initialize_layerstack(&StackInitialization {
            logical: 0,
            stack: stack(1),
            name: name("main"),
            scope: root(200),
            profile: root(201),
            genesis_root: root(0),
        })
        .unwrap();
    save(
        &Storage::new(handles.storage.clone()).unwrap(),
        b"genesis bytes",
    );
    drop(handles);
    path
}

#[test]
fn a_replica_from_the_base_and_its_bundles_holds_the_live_rows() {
    let t = Temp::new("change-export");
    let live = initialized(&t, "live");
    let base = mark_base(&live, [5; 32]).unwrap();
    assert_eq!(read_state(&live).unwrap(), Some(base));
    assert_eq!(
        log_entries(&live),
        0,
        "marking leaves only the sequence marker"
    );
    let replica = t.join("replica");
    std::fs::copy(&live, &replica).unwrap();

    // The live Store keeps its writer open across both exports.
    let handles = writable(&live);
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let history: &dyn HistoryCatalog = &handles.history;
    let genesis = history.layer_stack(stack(1)).unwrap().unwrap().head_layer;
    history
        .fork(&ForkRequest {
            stack: stack(1),
            branch: branch_id(1),
            name: name("b1"),
            source: ForkSource::Layer(genesis),
        })
        .unwrap();
    commit(history, branch_id(1), 1);
    commit(history, branch_id(1), 2);
    let small = save(&storage, b"small file");
    let large = save(&storage, &vec![7u8; 400_000]);
    history
        .record_bindings(&[BindingHint {
            child: 7,
            parent: 1,
            name: b"a.txt".to_vec(),
        }])
        .unwrap();
    let first = t.join("bundle-1");
    let one = export_changes(&live, base, &first).unwrap();
    assert_eq!(one.before, base);
    assert!(
        one.after.seq > base.seq && one.rows > 0 && one.bytes > 0,
        "{one:?}"
    );
    assert!(one.log_entries >= one.rows + one.deletes, "{one:?}");

    commit(history, branch_id(1), 3);
    let more = save(&storage, &vec![9u8; 300_000]);
    let second = t.join("bundle-2");
    let two = export_changes(&live, one.after, &second).unwrap();
    assert_eq!(two.before, one.after);
    assert!(two.after.seq > one.after.seq && two.rows > 0, "{two:?}");

    // Nothing changed: no file, the same cursor.
    let none = t.join("bundle-3");
    let three = export_changes(&live, two.after, &none).unwrap();
    assert_eq!((three.after, three.bytes), (two.after, 0));
    assert!(!none.exists());
    // A cursor ahead of the Store, or an existing bundle path, is refused.
    let ahead = layerfs_persistence::replication::ChangeCursor {
        seq: two.after.seq + 1,
        ..two.after
    };
    assert!(matches!(
        export_changes(&live, ahead, &none),
        Err(ChangeError::Cursor(_))
    ));
    let kept = std::fs::read(&second).unwrap();
    assert!(matches!(
        export_changes(&live, one.after, &second),
        Err(ChangeError::Exists)
    ));
    assert_eq!(std::fs::read(&second).unwrap(), kept);
    // A deletion alone: the bundle carries the table with its deleted key
    // and no rows; no other table changed.
    rusqlite::Connection::open(&live)
        .unwrap()
        .execute("DELETE FROM binding_hint WHERE child=7", [])
        .unwrap();
    let third = t.join("bundle-4");
    let four = export_changes(&live, two.after, &third).unwrap();
    assert_eq!((four.rows, four.deletes), (0, 1), "{four:?}");
    assert_eq!(std::fs::metadata(&third).unwrap().len(), four.bytes);
    // Every export linked its bundle into place and removed its temporary
    // file.
    let partial = std::fs::read_dir(t.join(""))
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".partial")
        })
        .count();
    assert_eq!(partial, 0);
    drop(handles);

    // Out of order is refused; in order reaches the live rows exactly.
    assert!(matches!(
        apply_changes(&replica, &second),
        Err(ChangeError::Cursor(_))
    ));
    assert_eq!(apply_changes(&replica, &first).unwrap().after, one.after);
    assert!(matches!(
        apply_changes(&replica, &first),
        Err(ChangeError::Cursor(_))
    ));
    let applied = apply_changes(&replica, &second).unwrap();
    assert_eq!((applied.before, applied.after), (one.after, two.after));
    apply_changes(&replica, &third).unwrap();
    assert_eq!(rows(&replica), rows(&live));
    assert_eq!(read_state(&replica).unwrap(), Some(four.after));

    // The replica opens through the normal path and answers the same.
    let opened = writable(&replica);
    let reader = Storage::new(opened.storage.clone()).unwrap();
    let reader = reader.reader().unwrap();
    assert_eq!(
        reader.file_lengths(&[small, large, more]).unwrap(),
        [10, 400_000, 300_000]
    );
    let source = writable(&live);
    assert_eq!(
        opened.history.branch(branch_id(1)).unwrap(),
        source.history.branch(branch_id(1)).unwrap()
    );
    assert_eq!(
        opened.history.bindings_of(7).unwrap(),
        source.history.bindings_of(7).unwrap()
    );
}

#[test]
fn applying_a_bundle_logs_nothing_and_leaves_no_free_pages_on_the_replica() {
    let t = Temp::new("change-export-quiet");
    let live = initialized(&t, "live");
    let base = mark_base(&live, [6; 32]).unwrap();
    let replica = t.join("replica");
    std::fs::copy(&live, &replica).unwrap();
    let handles = writable(&live);
    let storage = Storage::new(handles.storage.clone()).unwrap();
    // Enough rows for the log to span many pages.
    for i in 0..1500u32 {
        save(&storage, format!("object {i}").as_bytes());
    }
    drop(handles);
    let bundle = t.join("bundle");
    let one = export_changes(&live, base, &bundle).unwrap();
    assert!(one.log_entries >= 1500, "{one:?}");
    apply_changes(&replica, &bundle).unwrap();
    assert_eq!(rows(&replica), rows(&live));
    assert_eq!(read_state(&replica).unwrap(), Some(one.after));
    let sql = rusqlite::Connection::open(&replica).unwrap();
    let count = |q: &str| -> i64 { sql.query_row(q, [], |r| r.get(0)).unwrap() };
    assert_eq!(
        count("SELECT count(*) FROM change_log"),
        1,
        "only the marker"
    );
    assert_eq!(count("PRAGMA freelist_count"), 0, "no pages freed by a log");
}

#[test]
fn a_locked_replica_is_refused_at_once_and_left_unchanged() {
    let t = Temp::new("change-locked");
    let live = initialized(&t, "live");
    let base = mark_base(&live, [6; 32]).unwrap();
    let replica = t.join("replica");
    std::fs::copy(&live, &replica).unwrap();
    {
        let handles = writable(&live);
        save(&Storage::new(handles.storage.clone()).unwrap(), b"locked");
    }
    let bundle = t.join("bundle");
    export_changes(&live, base, &bundle).unwrap();
    // Another connection holds the replica's write lock.
    let holder = rusqlite::Connection::open(&replica).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    let started = std::time::Instant::now();
    let refused = apply_changes(&replica, &bundle);
    let waited = started.elapsed();
    assert!(
        matches!(&refused, Err(ChangeError::Sqlite(e))
            if e.sqlite_error_code() == Some(rusqlite::ErrorCode::DatabaseBusy)),
        "{refused:?}"
    );
    // One attempt: rusqlite's default busy handler would wait 5 s first.
    assert!(waited < std::time::Duration::from_secs(1), "{waited:?}");
    holder.execute_batch("ROLLBACK").unwrap();
    drop(holder);
    assert_eq!(read_state(&replica).unwrap(), Some(base));
    apply_changes(&replica, &bundle).unwrap();
    assert_eq!(rows(&replica), rows(&live));
}

#[test]
fn a_replica_outside_the_disposable_profile_is_refused_and_left_unchanged() {
    let t = Temp::new("change-profile");
    let live = initialized(&t, "live");
    let base = mark_base(&live, [6; 32]).unwrap();
    let replica = t.join("replica");
    std::fs::copy(&live, &replica).unwrap();
    {
        let handles = writable(&live);
        save(&Storage::new(handles.storage.clone()).unwrap(), b"profile");
    }
    let bundle = t.join("bundle");
    export_changes(&live, base, &bundle).unwrap();
    let journal = |mode: &str| {
        rusqlite::Connection::open(&replica)
            .unwrap()
            .query_row(&format!("PRAGMA journal_mode={mode}"), [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap()
    };
    assert_eq!(journal("DELETE"), "delete");
    let refused = apply_changes(&replica, &bundle);
    assert!(
        matches!(
            &refused,
            Err(ChangeError::Profile(PersistenceError::Malformed))
        ),
        "{refused:?}"
    );
    let source = std::error::Error::source(refused.as_ref().unwrap_err());
    assert_eq!(
        source.map(ToString::to_string),
        Some(PersistenceError::Malformed.to_string())
    );
    assert_eq!(journal("WAL"), "wal");
    assert_eq!(read_state(&replica).unwrap(), Some(base));
    apply_changes(&replica, &bundle).unwrap();
    assert_eq!(rows(&replica), rows(&live));
}

#[test]
fn a_written_replica_and_a_changed_bundle_are_refused() {
    let t = Temp::new("change-refusal");
    let live = initialized(&t, "live");
    let base = mark_base(&live, [6; 32]).unwrap();
    let replica = t.join("replica");
    std::fs::copy(&live, &replica).unwrap();
    {
        let handles = writable(&live);
        let genesis = handles
            .history
            .layer_stack(stack(1))
            .unwrap()
            .unwrap()
            .head_layer;
        handles
            .history
            .fork(&ForkRequest {
                stack: stack(1),
                branch: branch_id(1),
                name: name("b1"),
                source: ForkSource::Layer(genesis),
            })
            .unwrap();
        commit(&handles.history, branch_id(1), 1);
    }
    let bundle = t.join("bundle");
    export_changes(&live, base, &bundle).unwrap();

    // A changed bundle row fails the digest; nothing is installed.
    let changed = t.join("changed");
    std::fs::copy(&bundle, &changed).unwrap();
    rusqlite::Connection::open(&changed)
        .unwrap()
        .execute("UPDATE store_totals SET logical=logical+1", [])
        .unwrap();
    assert!(matches!(
        apply_changes(&replica, &changed),
        Err(ChangeError::Integrity("bundle digest"))
    ));
    assert_eq!(read_state(&replica).unwrap(), Some(base));

    // A replica written after its state has diverged from the chain.
    {
        let handles = writable(&replica);
        handles
            .history
            .record_bindings(&[BindingHint {
                child: 9,
                parent: 1,
                name: b"local".to_vec(),
            }])
            .unwrap();
    }
    assert!(matches!(
        apply_changes(&replica, &bundle),
        Err(ChangeError::Diverged)
    ));
}

#[test]
fn an_unmarked_store_logs_nothing_and_exports_nothing() {
    let t = Temp::new("change-unmarked");
    let live = initialized(&t, "live");
    {
        let handles = writable(&live);
        save(
            &Storage::new(handles.storage.clone()).unwrap(),
            &vec![3u8; 200_000],
        );
    }
    assert_eq!(log_entries(&live), 0);
    assert_eq!(read_state(&live).unwrap(), None);
    let cursor = layerfs_persistence::replication::ChangeCursor {
        state: [0; 32],
        seq: 0,
    };
    assert!(matches!(
        export_changes(&live, cursor, &t.join("bundle")),
        Err(ChangeError::NotBase)
    ));
}

#[test]
fn a_base_marked_under_a_reader_commits_and_reports_the_blocked_checkpoint() {
    let t = Temp::new("change-reader");
    let live = initialized(&t, "live");
    // A reader's snapshot keeps the checkpoint from copying the mark into the
    // Store file.
    let reader = rusqlite::Connection::open(&live).unwrap();
    reader.execute_batch("BEGIN").unwrap();
    let _: i64 = reader
        .query_row("SELECT count(*) FROM history_meta", [], |r| r.get(0))
        .unwrap();
    let marked = mark_base(&live, [8; 32]);
    assert!(
        matches!(marked, Err(ChangeError::NotCheckpointed(None))),
        "{marked:?}"
    );
    reader.execute_batch("COMMIT").unwrap();
    drop(reader);
    // The mark committed; without a reader, marking again checkpoints, and a
    // copy of the file alone is a replica.
    assert!(read_state(&live).unwrap().is_some());
    let base = mark_base(&live, [9; 32]).unwrap();
    let replica = t.join("replica");
    std::fs::copy(&live, &replica).unwrap();
    assert_eq!(read_state(&replica).unwrap(), Some(base));
}

/// Rebuilds `bundle_meta` without its constraints, so the cases below reach
/// apply's own checks.
const UNCHECKED_META: &str = "CREATE TABLE m AS SELECT * FROM bundle_meta; \
     DROP TABLE bundle_meta; ALTER TABLE m RENAME TO bundle_meta;";

#[test]
fn a_refused_bundle_leaves_the_replica_unchanged() {
    let t = Temp::new("change-refused");
    let live = initialized(&t, "live");
    let unmarked = t.join("unmarked");
    std::fs::copy(&live, &unmarked).unwrap();
    let base = mark_base(&live, [6; 32]).unwrap();
    let replica = t.join("replica");
    std::fs::copy(&live, &replica).unwrap();
    {
        let handles = writable(&live);
        save(&Storage::new(handles.storage.clone()).unwrap(), b"refused");
    }
    let bundle = t.join("bundle");
    export_changes(&live, base, &bundle).unwrap();

    let before = rows(&unmarked);
    let refused = apply_changes(&unmarked, &bundle);
    assert!(matches!(refused, Err(ChangeError::NotBase)), "{refused:?}");
    assert_eq!(rows(&unmarked), before);
    assert_eq!(read_state(&unmarked).unwrap(), None);

    let cases = [
        ("UPDATE bundle_meta SET format=2", "unknown bundle format"),
        (
            "UPDATE bundle_meta SET catalog_id=x'00'",
            "the bundle belongs to another catalog",
        ),
        (
            "UPDATE bundle_meta SET after_state=zeroblob(32)",
            "bundle ending state",
        ),
        (
            "INSERT INTO bundle_delete(tbl,k1) VALUES(1,1)",
            "a bundle deletes an immutable row",
        ),
        (
            "INSERT INTO bundle_meta SELECT * FROM bundle_meta",
            "the bundle meta is not one row",
        ),
        (
            "UPDATE bundle_meta SET after_seq=before_seq",
            "the bundle's sequences are out of order",
        ),
        (
            "UPDATE bundle_meta SET before_seq=-1",
            "the bundle meta is malformed",
        ),
        (
            "UPDATE bundle_meta SET digest=x'00'",
            "the bundle meta is malformed",
        ),
    ];
    let before = rows(&replica);
    for (i, (change, expected)) in cases.into_iter().enumerate() {
        let changed = t.join(&format!("changed-{i}"));
        std::fs::copy(&bundle, &changed).unwrap();
        rusqlite::Connection::open(&changed)
            .unwrap()
            .execute_batch(&format!("{UNCHECKED_META} {change};"))
            .unwrap();
        let refused = apply_changes(&replica, &changed);
        assert!(
            matches!(&refused, Err(ChangeError::Integrity(why)) if *why == expected),
            "{change}: {refused:?}"
        );
        assert_eq!(rows(&replica), before, "{change}");
        assert_eq!(read_state(&replica).unwrap(), Some(base), "{change}");
    }
    apply_changes(&replica, &bundle).unwrap();
    assert_eq!(rows(&replica), rows(&live));
}

#[test]
fn every_pack_layout_replicates_its_pack_rows() {
    for layout in [
        SqlitePackLayout::Monolithic,
        SqlitePackLayout::GroupRows,
        SqlitePackLayout::GroupRowsIndexed,
    ] {
        let t = Temp::new("change-layout");
        let live = initialized_in(&t, "live", layout);
        let base = mark_base(&live, [6; 32]).unwrap();
        let replica = t.join("replica");
        std::fs::copy(&live, &replica).unwrap();
        let (small, large) = {
            let handles = writable_in(&live, layout);
            let storage = Storage::new(handles.storage.clone()).unwrap();
            (
                save(&storage, b"small"),
                save(&storage, &vec![5u8; 400_000]),
            )
        };
        let bundle = t.join("bundle");
        export_changes(&live, base, &bundle).unwrap();
        apply_changes(&replica, &bundle).unwrap();
        let live_rows = rows(&live);
        let units = live_rows.iter().find(|(table, _)| *table == "pack_unit");
        assert_eq!(
            units.is_some_and(|(_, units)| !units.is_empty()),
            layout != SqlitePackLayout::Monolithic,
            "{layout:?}"
        );
        assert_eq!(rows(&replica), live_rows, "{layout:?}");
        let opened = writable_in(&replica, layout);
        let reader = Storage::new(opened.storage.clone()).unwrap();
        let reader = reader.reader().unwrap();
        assert_eq!(
            reader.file_lengths(&[small, large]).unwrap(),
            [5, 400_000],
            "{layout:?}"
        );
    }
}
