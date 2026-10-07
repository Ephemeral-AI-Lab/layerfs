//! Owned read snapshots preserve empty/stale distinction and typed windows.
#![cfg(target_os = "macos")]
mod support;
use layerfs_content::{inode_leaf::InodeKind, ObjectId};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{
    Handles, PersistenceConfig, SqliteAcquisitionSchema, SqlitePersistenceProfile,
};
use layerfs_storage::{
    port::acquisition::{
        Acquisition, AcquisitionError, Begin, Binding, EntryKey, Limits, NativeIdentity, NewEntry,
        Owner,
    },
    StoragePolicy,
};
use support::Temp;

const BINDING_KEY: &[u8] = b"owned-acquisition-reads";

fn config(path: &std::path::Path, profile: SqlitePersistenceProfile) -> PersistenceConfig {
    PersistenceConfig::sqlite(path)
        .with_sqlite_profile(profile)
        .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables)
}

fn create(path: &std::path::Path, profile: SqlitePersistenceProfile) -> Handles {
    Handles::create(
        config(path, profile),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING_KEY.to_vec(),
            cursor_key: [91; 32],
            incarnation: 1,
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

fn empty(port: &dyn Acquisition, owner: Owner) {
    assert!(port
        .entries(owner, None, Limits::MAXIMUM)
        .unwrap()
        .is_empty());
    assert!(port
        .bindings(owner, None, Limits::MAXIMUM)
        .unwrap()
        .is_empty());
    assert!(port
        .file_roots(owner, None, Limits::MAXIMUM)
        .unwrap()
        .is_empty());
    assert!(port
        .unplaced_children(owner, 0, None, Limits::MAXIMUM)
        .unwrap()
        .is_empty());
    assert_eq!(port.job(owner, 1).unwrap(), None);
    assert_eq!(port.directory_path(owner, 0).unwrap(), None);
    // Two-pass path windows remain valid empty reads as well.
    assert!(port.jobs(owner, None, Limits::MAXIMUM).unwrap().is_empty());
    assert!(port
        .directories(owner, None, Limits::MAXIMUM)
        .unwrap()
        .is_empty());
}

fn stale(port: &dyn Acquisition, owner: Owner) {
    assert_eq!(
        port.entries(owner, None, Limits::MAXIMUM),
        Err(AcquisitionError::Stale)
    );
    assert_eq!(
        port.bindings(owner, None, Limits::MAXIMUM),
        Err(AcquisitionError::Stale)
    );
    assert_eq!(
        port.file_roots(owner, None, Limits::MAXIMUM),
        Err(AcquisitionError::Stale)
    );
    assert_eq!(
        port.unplaced_children(owner, 0, None, Limits::MAXIMUM),
        Err(AcquisitionError::Stale)
    );
    assert_eq!(port.job(owner, 1), Err(AcquisitionError::Stale));
    assert_eq!(port.directory_path(owner, 0), Err(AcquisitionError::Stale));
    assert_eq!(
        port.jobs(owner, None, Limits::MAXIMUM),
        Err(AcquisitionError::Stale)
    );
    assert_eq!(
        port.directories(owner, None, Limits::MAXIMUM),
        Err(AcquisitionError::Stale)
    );
    // A malformed cursor cannot disguise the more fundamental stale owner.
    assert_eq!(port.job(owner, u64::MAX), Err(AcquisitionError::Stale));
}

#[test]
fn valid_empty_missing_released_and_foreign_owners_remain_distinct() {
    for profile in DEVELOPMENT_PROFILES {
        let temp = Temp::new("owned-empty-reads");
        let path = temp.join("store.sqlite");
        let handles = create(&path, profile);
        let owner = begin(&handles);
        empty(&handles.acquisition, owner);
        stale(
            &handles.acquisition,
            Owner {
                operation: owner.operation + 9,
                ..owner
            },
        );
        stale(
            &handles.acquisition,
            Owner {
                epoch: owner.epoch + 1,
                ..owner
            },
        );
        assert_eq!(
            handles.acquisition.job(owner, u64::MAX),
            Err(AcquisitionError::Bounds)
        );
        // A definite input refusal leaves the session available.
        empty(&handles.acquisition, owner);
        handles.acquisition.release(owner).unwrap();
        stale(&handles.acquisition, owner);
        let next = begin(&handles);
        empty(&handles.acquisition, next);
        drop(handles);
        // Reopening grants no authority over the previous session's owner.
        let readonly =
            Handles::open_read_only(config(&path, profile), BINDING_KEY, [91; 32]).unwrap();
        stale(&readonly.acquisition, next);
    }
}

fn entry(parent: Option<u64>, name: Vec<u8>, position: u64, kind: InodeKind) -> NewEntry {
    NewEntry {
        key: EntryKey { parent, name },
        position: Some(position),
        kind,
        metadata_root: ObjectId::for_bytes(&position.to_be_bytes()),
        target_root: (kind == InodeKind::Symlink).then(|| ObjectId::for_bytes(b"target")),
        native_path: match kind {
            InodeKind::Symlink => None,
            _ => Some(format!("/source/{position}").into_bytes()),
        },
        native: (kind == InodeKind::RegularFile).then_some(NativeIdentity {
            device: u64::MAX,
            inode: position,
            evidence: [47; 44],
        }),
    }
}

#[test]
fn typed_reads_and_narrow_bindings_preserve_operation_isolation_order_and_bounds() {
    let temp = Temp::new("owned-window-reads");
    let handles = create(
        &temp.join("store.sqlite"),
        SqlitePersistenceProfile::Disposable,
    );
    let port = &handles.acquisition;
    let first = begin(&handles);
    let other = begin(&handles);
    let mut inputs = vec![entry(None, Vec::new(), 0, InodeKind::Directory)];
    for position in 1..=600_u64 {
        let mut name = format!("n{position:08}").into_bytes();
        name.resize(255, b'x');
        inputs.push(entry(
            Some(0),
            name,
            position,
            match position % 3 {
                0 => InodeKind::RegularFile,
                1 => InodeKind::Symlink,
                _ => InodeKind::Directory,
            },
        ));
    }
    port.put_entries(first, &inputs).unwrap();
    let distinct = entry(Some(0), b"other".to_vec(), 1, InodeKind::Symlink);
    port.put_entries(other, std::slice::from_ref(&distinct))
        .unwrap();
    let roots: Vec<_> = (3..=600_u64)
        .step_by(3)
        .map(|position| (position, ObjectId::for_bytes(&position.to_le_bytes())))
        .collect();
    port.complete_files(first, &roots).unwrap();

    let initial = port.entries(first, None, Limits::MAXIMUM).unwrap();
    assert_eq!(initial.len(), 512);
    assert_eq!(initial[0].key.parent, None);
    assert_eq!(initial[0].position, 0);
    let bindings = port.bindings(first, None, Limits::MAXIMUM).unwrap();
    let expected: Vec<_> = initial
        .iter()
        .map(|entry| Binding {
            key: entry.key.clone(),
            position: entry.position,
            canonical: entry.canonical,
        })
        .collect();
    assert_eq!(bindings, expected);
    let after = &initial.last().unwrap().key;
    let remaining = port.entries(first, Some(after), Limits::MAXIMUM).unwrap();
    assert_eq!(remaining.len(), 89);
    assert_eq!(remaining[0].position, 512);
    assert!(port
        .entries(first, Some(&remaining.last().unwrap().key), Limits::MAXIMUM)
        .unwrap()
        .is_empty());
    assert!(port
        .bindings(first, Some(&remaining.last().unwrap().key), Limits::MAXIMUM)
        .unwrap()
        .is_empty());
    for limits in [
        Limits {
            rows: 7,
            bytes: 1024,
        },
        Limits { rows: 0, bytes: 0 },
    ] {
        let rows = port.entries(first, Some(&initial[0].key), limits).unwrap();
        assert!(!rows.is_empty());
        assert!(rows.len() <= limits.clamped().rows);
        let bytes: usize = rows
            .iter()
            .map(|row| row.key.name.len() + 40 + 32 + usize::from(row.content_root.is_some()) * 32)
            .sum();
        assert!(rows.len() == 1 || bytes <= limits.clamped().bytes);
        let rows = port.bindings(first, Some(&initial[0].key), limits).unwrap();
        let bytes: usize = rows.iter().map(|row| row.key.name.len() + 24).sum();
        assert!(rows.len() == 1 || bytes <= limits.clamped().bytes);
    }
    let files = port.file_roots(first, None, Limits::MAXIMUM).unwrap();
    assert_eq!(files.len(), roots.len());
    for (file, (position, root)) in files.iter().zip(&roots) {
        assert_eq!(
            (file.position, file.aliases, file.root),
            (*position, 0, Some(*root))
        );
        let job = port.job(first, *position).unwrap().unwrap();
        assert_eq!(job.position, *position);
        assert_eq!(job.native.device, u64::MAX);
        assert_eq!(job.native.evidence, [47; 44]);
    }
    assert!(port
        .file_roots(first, Some(600), Limits::MAXIMUM)
        .unwrap()
        .is_empty());
    assert!(port
        .unplaced_children(first, 0, None, Limits::MAXIMUM)
        .unwrap()
        .is_empty());
    assert_eq!(
        port.directory_path(first, 2).unwrap(),
        Some(b"/source/2".to_vec())
    );
    assert_eq!(port.directory_path(first, 3).unwrap(), None);
    let other_rows = port.entries(other, None, Limits::MAXIMUM).unwrap();
    assert_eq!(other_rows.len(), 1);
    assert_eq!(other_rows[0].key, distinct.key);
    assert!(port
        .file_roots(other, None, Limits::MAXIMUM)
        .unwrap()
        .is_empty());
    assert_eq!(port.job(other, 3).unwrap(), None);
}

/// Owner direction 2026-10-07: Disposable is the sole active development
/// verification profile. Durable execution of these bodies is NOT_RUN —
/// deferred by owner for Disposable-only development. Durable support and its
/// retained historical receipts are unchanged.
const DEVELOPMENT_PROFILES: [SqlitePersistenceProfile; 1] = [SqlitePersistenceProfile::Disposable];
