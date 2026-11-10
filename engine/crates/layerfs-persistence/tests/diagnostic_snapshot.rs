//! Real WAL snapshots across concurrent publication, removal and cancellation.
mod support;

use layerfs_content::{encode_whole_file_payload, FinalizedObject, ObjectRole};
use layerfs_history::{HistoryCatalog, HistoryCatalogConfig, StackInitialization};
use layerfs_persistence::{
    diagnostic::{Observation, Options, Outcome},
    PersistenceConfig, SqliteHandles, SqlitePersistenceProfile,
};
use layerfs_storage::{Storage, StoragePolicy};
use std::sync::atomic::{AtomicBool, Ordering};

fn authority() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: b"doctor-snapshot".to_vec(),
        cursor_key: [71; 32],
        incarnation: 7,
    }
}
fn create(path: &std::path::Path) -> SqliteHandles {
    SqliteHandles::create(
        PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        StoragePolicy::frozen_default(),
        &authority(),
    )
    .unwrap()
}
fn reader(path: &std::path::Path) -> SqliteHandles {
    let a = authority();
    SqliteHandles::open_diagnostic(path, &a.binding_key, a.cursor_key).unwrap()
}
fn put(writer: &SqliteHandles, n: usize) {
    let storage = Storage::new(writer.storage.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    save.accept(
        FinalizedObject::new(
            ObjectRole::WholeFile,
            encode_whole_file_payload(format!("distinct retained object {n}").as_bytes()).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    save.finish().unwrap();
}
fn clean(result: &[Observation]) {
    assert!(
        !result
            .iter()
            .any(|o| matches!(o.outcome, Outcome::Fail | Outcome::Unknown)),
        "{result:?}"
    );
}
fn audit(reader: &SqliteHandles) -> Vec<Observation> {
    let mut result = Vec::new();
    reader
        .diagnose(Options::default(), &mut |o| result.push(o))
        .unwrap();
    clean(&result);
    result
}
fn project(writer: &SqliteHandles, n: u8) {
    use layerfs_content::{
        filesystem::{
            attributes::PortableMetadata,
            directory::{encode_directory_page, DirectoryPage},
            inode::{encode_inode_page, InodePage},
            scope_for_seed, FilesystemRoot,
        },
        inode_leaf::{InodeKind, InodeMetadata, InodeValue},
    };
    let storage = Storage::new(writer.storage.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    let directory = FinalizedObject::new(
        ObjectRole::DirectoryLeaf,
        encode_directory_page(&DirectoryPage::Leaf { entries: vec![] }).unwrap(),
    )
    .unwrap();
    let directory_id = directory.id();
    save.accept(directory).unwrap();
    let inode = FinalizedObject::new(
        ObjectRole::InodeLeaf,
        encode_inode_page(&InodePage::Leaf {
            entries: vec![(
                1,
                InodeValue {
                    kind: InodeKind::Directory,
                    namespace_ref_count: 0,
                    content_root: directory_id,
                    metadata_root: InodeMetadata::Inline(PortableMetadata {
                        mode: 0o755,
                        mtime_seconds: 0,
                        mtime_nanoseconds: 0,
                    }),
                },
            )],
        })
        .unwrap(),
    )
    .unwrap()
    .with_references(vec![directory_id]);
    let inode_id = inode.id();
    save.accept(inode).unwrap();
    let scope = scope_for_seed([n; 32]);
    let root = FilesystemRoot::new(
        layerfs_content::filesystem::root::profile_id(),
        scope,
        1,
        inode_id,
    )
    .unwrap();
    let object = FinalizedObject::new(ObjectRole::FilesystemRoot, root.encode().unwrap())
        .unwrap()
        .with_references(vec![inode_id]);
    let root_id = object.id();
    save.accept(object).unwrap();
    save.finish().unwrap();
    writer
        .history
        .initialize_layerstack(&StackInitialization {
            logical: 0,
            stack: support::stack(n),
            name: support::name(&format!("project-{n}")),
            scope: scope.object(),
            profile: root.profile(),
            genesis_root: root_id,
        })
        .unwrap();
}
fn summary(result: &[Observation], id: &str, prefix: &str) {
    assert!(
        result
            .iter()
            .any(|o| o.id == id && o.message.starts_with(prefix)),
        "{result:?}"
    );
}

#[test]
fn concurrent_project_removal_preserves_pinned_history_and_retained_content() {
    let temp = support::Temp::new("doctor-remove-snapshot");
    let path = temp.join("store.sqlite");
    let writer = create(&path);
    project(&writer, 1);
    project(&writer, 2);
    let reader = reader(&path);
    let before = reader.diagnostics().unwrap();
    let mut result = Vec::new();
    reader
        .diagnose(Options::default(), &mut |o| {
            if o.id == "store.snapshot" {
                writer.history.remove_layerstack(support::stack(1)).unwrap();
            }
            result.push(o);
        })
        .unwrap();
    clean(&result);
    summary(&result, "store.namespace.summary", "2 historical");
    assert!(writer
        .history
        .layer_stack(support::stack(1))
        .unwrap()
        .is_none());
    summary(&audit(&reader), "store.namespace.summary", "1 historical");
    let after = reader.diagnostics().unwrap();
    assert_eq!(after.write_transactions, before.write_transactions);
    assert_eq!(after.write_commits, before.write_commits);
}

#[test]
fn publication_between_inventory_windows_is_excluded_from_both_scans() {
    let temp = support::Temp::new("doctor-paging-snapshot");
    let path = temp.join("store.sqlite");
    let writer = create(&path);
    for n in 0..70 {
        put(&writer, n);
    }
    let reader = reader(&path);
    let mut windows = 0;
    let mut result = Vec::new();
    reader
        .diagnose(Options::default(), &mut |o| {
            if o.id == "store.packs.progress" {
                windows += 1;
                if windows == 2 {
                    put(&writer, 70);
                }
            }
            result.push(o);
        })
        .unwrap();
    assert_eq!(windows, 2);
    clean(&result);
    summary(&result, "store.packs.summary", "70 packs");
    summary(&result, "store.objects.summary", "70 winning");
    summary(&audit(&reader), "store.objects.summary", "71 winning");
}

fn checkpoint(connection: &rusqlite::Connection) -> (i64, i64) {
    connection
        .query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |row| {
            Ok((row.get(1)?, row.get(2)?))
        })
        .unwrap()
}
fn cancellation_releases_snapshot(phase: &'static str) {
    let temp = support::Temp::new("doctor-cancel-snapshot");
    let path = temp.join("store.sqlite");
    let writer = create(&path);
    for n in 0..70 {
        put(&writer, n);
    }
    let reader = reader(&path);
    let checkpointer = rusqlite::Connection::open(&path).unwrap();
    let before = reader.diagnostics().unwrap();
    let cancelled = AtomicBool::new(false);
    let mut windows = 0;
    let mut result = Vec::new();
    let error = reader.diagnose(Options { cancelled: Some(&cancelled), ..Options::default() }, &mut |o| {
        if o.id == phase {
            windows += 1;
            if windows == 2 {
                put(&writer, 70);
                let (frames, done) = checkpoint(&checkpointer);
                assert!(frames > done, "snapshot must prevent checkpoint of concurrent publication: {frames}/{done}");
                cancelled.store(true, Ordering::Relaxed);
            }
        }
        result.push(o);
    });
    assert!(error.is_err());
    assert_eq!(windows, 2);
    let summary_id = if phase == "store.packs.progress" {
        "store.packs.summary"
    } else {
        "store.objects.summary"
    };
    assert!(!result.iter().any(|o| o.id == summary_id));
    let (frames, done) = checkpoint(&checkpointer);
    assert_eq!(
        frames, done,
        "interrupted audit must release its original WAL snapshot"
    );
    let after = reader.diagnostics().unwrap();
    assert_eq!(after.write_transactions, before.write_transactions);
    assert_eq!(after.write_commits, before.write_commits);
    summary(&audit(&reader), "store.objects.summary", "71 winning");
}
#[test]
fn cancellation_during_pack_authentication_releases_snapshot() {
    cancellation_releases_snapshot("store.packs.progress");
}
#[test]
fn cancellation_during_object_traversal_releases_snapshot() {
    cancellation_releases_snapshot("store.objects.progress");
}
