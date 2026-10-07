//! Physical reclamation preserves live operations and pays bounded jobs.
#![cfg(target_os = "macos")]
mod support;
use layerfs_content::{inode_leaf::InodeKind, ObjectId};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{
    Handles, PersistenceConfig, SqliteAcquisitionSchema, SqlitePersistenceProfile,
    RECLAMATION_PAGE_LIMIT,
};
use layerfs_storage::{
    port::acquisition::{Acquisition, Begin, EntryKey, Limits, NativeIdentity, NewEntry, Owner},
    port::PersistenceError,
    StoragePolicy,
};
use support::Temp;

fn config(path: &std::path::Path, profile: SqlitePersistenceProfile) -> PersistenceConfig {
    PersistenceConfig::sqlite(path)
        .with_sqlite_profile(profile)
        .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables)
}
fn history() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: b"space-reclamation".to_vec(),
        incarnation: 1,
        cursor_key: [82; 32],
    }
}
fn begin(handles: &Handles) -> Owner {
    handles
        .acquisition
        .begin(&Begin {
            source_device: 1,
            source_inode: 2,
            stack: [3; 16],
            scope: ObjectId::for_bytes(b"scope"),
        })
        .unwrap()
}
fn file(position: u64, path_bytes: usize) -> NewEntry {
    NewEntry {
        key: EntryKey {
            parent: Some(0),
            name: format!("f{position:08}").into_bytes(),
        },
        position: Some(position),
        kind: InodeKind::RegularFile,
        metadata_root: ObjectId::for_bytes(b"metadata"),
        target_root: None,
        native_path: Some(vec![b'p'; path_bytes]),
        native: Some(NativeIdentity {
            device: 1,
            inode: position,
            evidence: [0; 44],
        }),
    }
}
fn populate(handles: &Handles, owner: Owner, count: u64, path_bytes: usize) {
    for start in (1..=count).step_by(128) {
        let rows: Vec<_> = (start..=(start + 127).min(count))
            .map(|position| file(position, path_bytes))
            .collect();
        handles.acquisition.put_entries(owner, &rows).unwrap();
    }
}
fn number(path: &std::path::Path, pragma: &str) -> u64 {
    let connection =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let value: i64 = connection.query_row(pragma, [], |row| row.get(0)).unwrap();
    u64::try_from(value).unwrap()
}

#[test]
fn bounded_reclamation_preserves_another_live_operation_and_survives_reopen() {
    for profile in DEVELOPMENT_PROFILES {
        let temp = Temp::new("bounded-reclamation");
        let path = temp.join("store.sqlite");
        let handles = Handles::create(
            config(&path, profile),
            StoragePolicy::frozen_default(),
            &history(),
        )
        .unwrap();
        assert_eq!(handles.profile().auto_vacuum, 2);
        let owner = begin(&handles);
        populate(&handles, owner, 2300, 3500);
        let live = begin(&handles);
        populate(&handles, live, 10, 20);
        let retained = handles.acquisition.work(live).unwrap();
        let before = number(&path, "PRAGMA page_count");
        for _ in 0..2 {
            let removed = handles.acquisition.discard(owner, 4096).unwrap();
            if removed.remaining_rows == 0 {
                break;
            }
        }
        handles.acquisition.release(owner).unwrap();
        assert!(number(&path, "PRAGMA freelist_count") > 1);
        let one = handles.reclaim_space(1).unwrap();
        assert_eq!(one.reclaimed_pages, 1);
        assert_eq!(one.free_pages_before - one.remaining_free_pages, 1);
        for _ in 0..32 {
            let job = handles.reclaim_space(RECLAMATION_PAGE_LIMIT).unwrap();
            assert!(job.reclaimed_pages <= u64::from(RECLAMATION_PAGE_LIMIT));
            if job.remaining_free_pages == 0 {
                break;
            }
            assert!(job.reclaimed_pages > 0);
        }
        assert_eq!(number(&path, "PRAGMA freelist_count"), 0);
        assert!(number(&path, "PRAGMA page_count") < before);
        assert_eq!(handles.acquisition.work(live).unwrap(), retained);
        assert_eq!(
            handles
                .acquisition
                .jobs(live, None, Limits::MAXIMUM)
                .unwrap()
                .len(),
            10
        );
        handles.checkpoint().unwrap();
        drop(handles);
        let opened = Handles::open_read_only(
            config(&path, profile),
            &history().binding_key,
            history().cursor_key,
        )
        .unwrap();
        assert_eq!(opened.profile().auto_vacuum, 2);
        assert!(matches!(
            opened.reclaim_space(1),
            Err(PersistenceError::Refused { .. })
        ));
        drop(opened);
        let opened = Handles::open_writable(
            config(&path, profile),
            &history().binding_key,
            history().cursor_key,
        )
        .unwrap();
        assert_eq!(
            opened.acquisition.abandoned(None, 8).unwrap()[0].held_rows,
            retained.held_rows
        );
    }
}

#[test]
fn old_non_reclaimable_stores_are_opened_without_implicit_conversion() {
    let temp = Temp::new("old-reclamation-mode");
    let path = temp.join("store.sqlite");
    let profile = SqlitePersistenceProfile::Disposable;
    let handles = Handles::create(
        config(&path, profile),
        StoragePolicy::frozen_default(),
        &history(),
    )
    .unwrap();
    drop(handles);
    // Construct the retained physical header mode externally; schema and
    // authority are exactly the supported acquisition variant.
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch("PRAGMA auto_vacuum=NONE; VACUUM")
        .unwrap();
    drop(connection);
    let handles = Handles::open_writable(
        config(&path, profile),
        &history().binding_key,
        history().cursor_key,
    )
    .unwrap();
    assert_eq!(handles.profile().auto_vacuum, 0);
    let before = handles.diagnostics().unwrap().statements;
    assert_eq!(handles.reclaim_space(1), Err(PersistenceError::Malformed));
    assert_eq!(handles.diagnostics().unwrap().statements, before);
    assert_eq!(number(&path, "PRAGMA auto_vacuum"), 0);
}

#[test]
fn path_windows_use_actual_lengths_without_exceeding_column_or_row_budgets() {
    let temp = Temp::new("path-windows");
    let path = temp.join("store.sqlite");
    let handles = Handles::create(
        config(&path, SqlitePersistenceProfile::Disposable),
        StoragePolicy::frozen_default(),
        &history(),
    )
    .unwrap();
    let short = begin(&handles);
    populate(&handles, short, 600, 20);
    let rows = handles
        .acquisition
        .jobs(short, None, Limits::MAXIMUM)
        .unwrap();
    assert_eq!(rows.len(), 512);
    assert_eq!(
        handles
            .acquisition
            .jobs(short, Some(512), Limits::MAXIMUM)
            .unwrap()
            .len(),
        88
    );
    let long = begin(&handles);
    populate(&handles, long, 600, 4096);
    let rows = handles
        .acquisition
        .jobs(long, None, Limits::MAXIMUM)
        .unwrap();
    assert_eq!(rows.len(), (256 * 1024) / (4096 + 68));
    assert!(
        rows.iter()
            .map(|row| 68 + row.native_path.len())
            .sum::<usize>()
            <= 256 * 1024
    );
    let one = handles
        .acquisition
        .jobs(
            long,
            Some(rows.last().unwrap().position),
            Limits {
                rows: 512,
                bytes: 1,
            },
        )
        .unwrap();
    assert_eq!(one.len(), 1);
    for (name, plan) in handles.explain_acquisition().unwrap() {
        if matches!(name, "job_sizes" | "directory_sizes") {
            println!("SIZING_PLAN {name}: {plan:?}");
            assert!(plan
                .iter()
                .all(|line| !line.starts_with("SCAN") && !line.contains("TEMP B-TREE")));
        }
    }
}

/// Owner direction 2026-10-07: Disposable is the sole active development
/// verification profile. Durable execution of these bodies is NOT_RUN —
/// deferred by owner for Disposable-only development. Durable support and its
/// retained historical receipts are unchanged.
const DEVELOPMENT_PROFILES: [SqlitePersistenceProfile; 1] = [SqlitePersistenceProfile::Disposable];
