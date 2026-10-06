//! Window execution costs, alias identity and atomic root completion.
#![cfg(target_os = "macos")]
mod support;
use layerfs_content::{inode_leaf::InodeKind, ObjectId};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{
    Handles, PersistenceConfig, SqliteAcquisitionSchema, SqlitePersistenceProfile,
};
use layerfs_storage::{port::acquisition::*, StoragePolicy};
use support::Temp;

fn object(label: &[u8]) -> ObjectId {
    ObjectId::for_bytes(label)
}
fn create(temp: &Temp, profile: SqlitePersistenceProfile) -> Handles {
    Handles::create(
        PersistenceConfig::sqlite(temp.join("store.sqlite"))
            .with_sqlite_profile(profile)
            .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"acquisition-batching".to_vec(),
            incarnation: 1,
            cursor_key: [83; 32],
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
            scope: object(b"scope"),
        })
        .unwrap()
}
fn directory(parent: Option<u64>, name: &[u8], position: u64) -> NewEntry {
    NewEntry {
        key: EntryKey {
            parent,
            name: name.to_vec(),
        },
        position: Some(position),
        kind: InodeKind::Directory,
        metadata_root: object(b"metadata"),
        target_root: None,
        native_path: Some(b"/source".to_vec()),
        native: None,
    }
}
fn file(position: u64, identity: u64) -> NewEntry {
    NewEntry {
        key: EntryKey {
            parent: Some(0),
            name: format!("f{position:04}").into_bytes(),
        },
        position: Some(position),
        kind: InodeKind::RegularFile,
        metadata_root: object(b"metadata"),
        target_root: None,
        native_path: Some(format!("/source/f{position:04}").into_bytes()),
        native: Some(NativeIdentity {
            device: 1,
            inode: identity,
            evidence: [7; EVIDENCE_BYTES],
        }),
    }
}

#[test]
fn windows_amortize_execution_without_losing_aliases_or_atomic_completion() {
    for profile in [
        SqlitePersistenceProfile::Durable,
        SqlitePersistenceProfile::Disposable,
    ] {
        let temp = Temp::new("acquisition-window-execution");
        let handles = create(&temp, profile);
        let port = &handles.acquisition;
        let owner = begin(&handles);
        port.put_entries(owner, &[directory(None, b"", 0)]).unwrap();
        let entries: Vec<_> = (1..=200)
            .map(|position| file(position, (position - 1) % 100 + 1))
            .collect();
        let before = handles.diagnostics().unwrap();
        let canonical = port.put_entries(owner, &entries).unwrap();
        let after = handles.diagnostics().unwrap();
        assert_eq!(
            canonical,
            (1..=200).map(|n| (n - 1) % 100 + 1).collect::<Vec<_>>()
        );
        // Preparation must be paid per window, not once per row execution.
        assert!(
            after.statement_phases[0].calls - before.statement_phases[0].calls <= 16,
            "entry window prepares per row: {}",
            after.statement_phases[0].calls - before.statement_phases[0].calls
        );
        let roots: Vec<_> = (1_u64..=100)
            .map(|n| (n, object(&n.to_be_bytes())))
            .collect();
        let before = handles.diagnostics().unwrap();
        port.complete_files(owner, &roots).unwrap();
        let after = handles.diagnostics().unwrap();
        assert!(
            after.statements - before.statements <= 32,
            "root window executes per row"
        );
        let actual = port.file_roots(owner, None, Limits::MAXIMUM).unwrap();
        assert_eq!(actual.len(), 100);
        for (row, (_, root)) in actual.iter().zip(&roots) {
            assert_eq!(row.root, Some(*root));
            assert_eq!(row.aliases, 1);
        }
        port.discard(owner, 4096).unwrap();
        port.release(owner).unwrap();
    }
}

#[test]
fn missing_and_repeated_roots_roll_back_the_window_and_report_first_failure() {
    let temp = Temp::new("acquisition-root-window-atomic");
    let handles = create(&temp, SqlitePersistenceProfile::Disposable);
    let port = &handles.acquisition;
    let owner = begin(&handles);
    port.put_entries(owner, &[directory(None, b"", 0)]).unwrap();
    let entries: Vec<_> = (1..=100).map(|n| file(n, n)).collect();
    port.put_entries(owner, &entries).unwrap();
    let before = port.work(owner).unwrap();
    let mut roots: Vec<_> = (1_u64..=100)
        .map(|n| (n, object(&n.to_be_bytes())))
        .collect();
    roots[63].0 = 999;
    assert_eq!(
        port.complete_files(owner, &roots),
        Err(AcquisitionError::Changed { position: 999 })
    );
    assert_eq!(port.work(owner).unwrap(), before);
    assert!(port
        .file_roots(owner, None, Limits::MAXIMUM)
        .unwrap()
        .iter()
        .all(|row| row.root.is_none()));
    roots[63].0 = 64;
    roots[77].0 = roots[76].0;
    assert_eq!(
        port.complete_files(owner, &roots),
        Err(AcquisitionError::Changed { position: 77 })
    );
    assert_eq!(port.work(owner).unwrap(), before);
    assert!(port
        .file_roots(owner, None, Limits::MAXIMUM)
        .unwrap()
        .iter()
        .all(|row| row.root.is_none()));
    // An earlier missing row wins over a later duplicate in the same SQL batch.
    let roots = [(999, object(b"a")), (1, object(b"b")), (1, object(b"c"))];
    assert_eq!(
        port.complete_files(owner, &roots),
        Err(AcquisitionError::Changed { position: 999 })
    );
    let roots = [(999, object(b"a")), (u64::MAX, object(b"b"))];
    assert_eq!(
        port.complete_files(owner, &roots),
        Err(AcquisitionError::Changed { position: 999 })
    );
    assert_eq!(
        port.complete_files(owner, &[(1, object(b"a")), (u64::MAX, object(b"b"))]),
        Err(AcquisitionError::Bounds)
    );
    assert_eq!(port.work(owner).unwrap(), before);
    assert!(port
        .file_roots(owner, None, Limits::MAXIMUM)
        .unwrap()
        .iter()
        .all(|row| row.root.is_none()));
    let roots: Vec<_> = (1_u64..=100)
        .rev()
        .map(|n| (n, object(&n.to_be_bytes())))
        .collect();
    port.complete_files(owner, &roots).unwrap();
    assert!(port
        .file_roots(owner, None, Limits::MAXIMUM)
        .unwrap()
        .iter()
        .all(|row| row.root.is_some()));
}

#[test]
fn directory_completion_uses_only_directory_positions_and_is_atomic() {
    let temp = Temp::new("acquisition-directory-window");
    let handles = create(&temp, SqlitePersistenceProfile::Disposable);
    let port = &handles.acquisition;
    let owner = begin(&handles);
    port.put_entries(
        owner,
        &[
            directory(None, b"", 0),
            directory(Some(0), b"d", 1),
            file(2, 2),
        ],
    )
    .unwrap();
    let before = port.work(owner).unwrap();
    assert_eq!(
        port.set_directory_roots(owner, &[(0, object(b"root")), (2, object(b"file"))]),
        Err(AcquisitionError::Changed { position: 2 })
    );
    assert_eq!(port.work(owner).unwrap(), before);
    assert!(port
        .entries(owner, None, Limits::MAXIMUM)
        .unwrap()
        .iter()
        .all(|row| row.content_root.is_none()));
    port.set_directory_roots(owner, &[(1, object(b"dir")), (0, object(b"root"))])
        .unwrap();
    assert_eq!(
        port.set_directory_roots(owner, &[(1, object(b"again"))]),
        Err(AcquisitionError::Changed { position: 1 })
    );
}

#[test]
fn root_window_index_work_ignores_unrelated_population() {
    let mut work = Vec::new();
    for population in [2000_u64, 20000] {
        let temp = Temp::new("acquisition-root-index-work");
        let handles = create(&temp, SqlitePersistenceProfile::Disposable);
        let port = &handles.acquisition;
        let owner = begin(&handles);
        port.put_entries(owner, &[directory(None, b"", 0)]).unwrap();
        let mut next = 1;
        while next <= population {
            let end = (next + WRITE_WINDOW_ROWS as u64 - 1).min(population);
            let entries: Vec<_> = (next..=end).map(|n| file(n, n)).collect();
            port.put_entries(owner, &entries).unwrap();
            next = end + 1;
        }
        let roots: Vec<_> = (population / 2..population / 2 + 32)
            .map(|n| (n, object(&n.to_be_bytes())))
            .collect();
        let before = handles.diagnostics().unwrap();
        port.complete_files(owner, &roots).unwrap();
        let after = handles.diagnostics().unwrap();
        let delta = [
            after.statements - before.statements,
            after.vm_steps - before.vm_steps,
            after.fullscan_steps - before.fullscan_steps,
            after.sorts - before.sorts,
            after.autoindex_rows - before.autoindex_rows,
            after.reprepares - before.reprepares,
        ];
        println!("ROOT_WINDOW_PROFILE population={population} [statements,vm_steps,fullscan,sorts,autoindex,reprepares]={delta:?}");
        assert!(delta[0] <= 8);
        assert_eq!(&delta[3..], &[0, 0, 0]);
        work.push(delta);
    }
    assert_eq!(work[0][0], work[1][0]);
    assert_eq!(work[0][2], work[1][2]);
    assert!(work[1][1] <= work[0][1] + work[0][1] / 10);
}
