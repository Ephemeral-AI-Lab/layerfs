//! Final cleanup acknowledgment, owner release and independent live custody.
#![cfg(target_os = "macos")]
mod support;
use layerfs_content::{inode_leaf::InodeKind, ObjectId};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{
    Handles, PersistenceConfig, SqliteAcquisitionSchema, SqlitePersistenceProfile,
};
use layerfs_storage::{port::acquisition::*, StoragePolicy};
use support::Temp;

fn create(temp: &Temp, profile: SqlitePersistenceProfile) -> Handles {
    Handles::create(
        PersistenceConfig::sqlite(temp.join("store.sqlite"))
            .with_sqlite_profile(profile)
            .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"acquisition-disposal".to_vec(),
            incarnation: 1,
            cursor_key: [89; 32],
        },
    )
    .unwrap()
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

fn root() -> NewEntry {
    NewEntry {
        key: EntryKey {
            parent: None,
            name: Vec::new(),
        },
        position: Some(0),
        kind: InodeKind::Directory,
        metadata_root: ObjectId::for_bytes(b"metadata"),
        target_root: None,
        native_path: Some(b"/source".to_vec()),
        native: None,
    }
}

fn file(position: u64) -> NewEntry {
    NewEntry {
        key: EntryKey {
            parent: Some(0),
            name: format!("f{position}").into_bytes(),
        },
        position: Some(position),
        kind: InodeKind::RegularFile,
        metadata_root: ObjectId::for_bytes(b"metadata"),
        target_root: None,
        native_path: Some(format!("/source/f{position}").into_bytes()),
        native: Some(NativeIdentity {
            device: 1,
            inode: position,
            evidence: [7; EVIDENCE_BYTES],
        }),
    }
}

#[test]
fn final_bounded_disposal_releases_in_one_ack_and_preserves_another_owner() {
    for profile in [
        SqlitePersistenceProfile::Durable,
        SqlitePersistenceProfile::Disposable,
    ] {
        let temp = Temp::new("acquisition-final-disposal");
        let handles = create(&temp, profile);
        let port = &handles.acquisition;
        let owner = begin(&handles);
        let live = begin(&handles);
        port.put_entries(owner, &[root(), file(1), file(2)])
            .unwrap();
        port.put_entries(live, &[root(), file(1)]).unwrap();
        port.complete_files(live, &[(1, ObjectId::for_bytes(b"file-root"))])
            .unwrap();
        let live_entries = port.entries(live, None, Limits::MAXIMUM).unwrap();
        let live_roots = port.file_roots(live, None, Limits::MAXIMUM).unwrap();
        let live_work = port.work(live).unwrap();
        let held = port.work(owner).unwrap();
        assert_eq!(held.held_rows, 5);
        let mut removed_rows = 0;
        let mut removed_bytes = 0;
        for remaining in [3, 1, 0] {
            let before = handles.diagnostics().unwrap();
            let disposal = port.dispose(owner, 2).unwrap();
            let after = handles.diagnostics().unwrap();
            assert_eq!(after.write_commits - before.write_commits, 1);
            assert!(disposal.discarded.rows <= 2);
            assert_eq!(disposal.discarded.remaining_rows, remaining);
            assert_eq!(disposal.released, remaining == 0);
            removed_rows += disposal.discarded.rows;
            removed_bytes += disposal.discarded.bytes;
            assert_eq!(port.work(live).unwrap(), live_work);
            assert_eq!(
                port.entries(live, None, Limits::MAXIMUM).unwrap(),
                live_entries
            );
            assert_eq!(
                port.file_roots(live, None, Limits::MAXIMUM).unwrap(),
                live_roots
            );
        }
        assert_eq!(
            (removed_rows, removed_bytes),
            (held.held_rows, held.held_bytes)
        );
        assert_eq!(port.work(owner), Err(AcquisitionError::Stale));
        assert_eq!(port.dispose(owner, 2), Err(AcquisitionError::Stale));
        assert_eq!(port.release(owner), Err(AcquisitionError::Stale));
        let disposal = port.dispose(live, WRITE_WINDOW_ROWS).unwrap();
        assert!(disposal.released);
        assert_eq!(disposal.discarded.remaining_rows, 0);
    }
}

#[test]
fn zero_budget_retains_work_and_can_release_an_empty_owner() {
    let temp = Temp::new("acquisition-zero-disposal");
    let handles = create(&temp, SqlitePersistenceProfile::Disposable);
    let port = &handles.acquisition;
    let owner = begin(&handles);
    port.put_entries(owner, &[root()]).unwrap();
    let held = port.work(owner).unwrap();
    let disposal = port.dispose(owner, 0).unwrap();
    assert_eq!(disposal.discarded.rows, 0);
    assert_eq!(disposal.discarded.remaining_rows, 1);
    assert!(!disposal.released);
    assert_eq!(port.work(owner).unwrap(), held);
    assert!(matches!(
        port.release(owner),
        Err(AcquisitionError::Persistence(_))
    ));
    assert_eq!(port.work(owner).unwrap(), held);
    let discarded = port.discard(owner, 1).unwrap();
    assert_eq!(discarded.remaining_rows, 0);
    let before = handles.diagnostics().unwrap();
    let disposal = port.dispose(owner, 0).unwrap();
    let after = handles.diagnostics().unwrap();
    assert_eq!(after.write_commits - before.write_commits, 1);
    assert_eq!(disposal.discarded, Discarded::default());
    assert!(disposal.released);
    assert_eq!(port.work(owner), Err(AcquisitionError::Stale));
}
