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

fn symlink(position: u64) -> NewEntry {
    NewEntry {
        key: EntryKey {
            parent: Some(0),
            name: format!("s{position:04}").into_bytes(),
        },
        position: Some(position),
        kind: InodeKind::Symlink,
        metadata_root: object(b"link-metadata"),
        target_root: Some(object(b"opaque-link-target")),
        native_path: None,
        native: None,
    }
}

#[test]
fn non_native_windows_skip_the_native_insert_without_losing_targets_or_charges() {
    let temp = Temp::new("acquisition-non-native-window");
    let handles = create(&temp, SqlitePersistenceProfile::Disposable);
    let port = &handles.acquisition;
    let owner = begin(&handles);
    port.put_entries(owner, &[directory(None, b"", 0)]).unwrap();
    let links: Vec<_> = (1..=32).map(symlink).collect();
    let before = handles.diagnostics().unwrap();
    assert!(port.put_entries(owner, &links).unwrap().is_empty());
    let after = handles.diagnostics().unwrap();
    // Framing, owner, dependency check, entry insertion and accounting are the
    // complete unit. No empty native-file INSERT is executed for this window.
    assert_eq!(after.statements - before.statements, 6);
    let rows = port.entries(owner, None, Limits::MAXIMUM).unwrap();
    assert_eq!(rows.len(), 33);
    for row in rows.iter().skip(1) {
        assert_eq!(row.kind, InodeKind::Symlink);
        assert_eq!(row.metadata_root, object(b"link-metadata"));
        assert_eq!(row.content_root, Some(object(b"opaque-link-target")));
        assert_eq!(row.canonical, None);
    }
    assert!(port.jobs(owner, None, Limits::MAXIMUM).unwrap().is_empty());
    let held = port.work(owner).unwrap();
    let removed = port.discard(owner, 4096).unwrap();
    assert_eq!(
        (removed.rows, removed.bytes),
        (held.held_rows, held.held_bytes)
    );
    assert_eq!(port.work(owner).unwrap().held_bytes, 0);
    port.release(owner).unwrap();
}

#[test]
fn narrow_bindings_preserve_mixed_payloads_after_an_existing_alias_boundary() {
    for profile in DEVELOPMENT_PROFILES {
        let temp = Temp::new("acquisition-mixed-window-prefix");
        let handles = create(&temp, profile);
        let port = &handles.acquisition;
        let owner = begin(&handles);
        port.put_entries(owner, &[directory(None, b"", 0), file(1, 1)])
            .unwrap();
        let mut entries: Vec<_> = (2..=16).map(symlink).collect();
        entries.push(file(17, 1));
        let mut child_directory = directory(Some(0), b"d0018", 18);
        let directory_path = b"/source/\xffdirectory".to_vec();
        child_directory.native_path = Some(directory_path.clone());
        entries.push(child_directory);
        let mut unplaced = file(19, 19);
        unplaced.position = None;
        let native = unplaced.native.unwrap();
        let native_path = unplaced.native_path.clone().unwrap();
        entries.push(unplaced);
        entries.push(symlink(20));
        // The existing alias truncates the independent prefix in the middle
        // of one SQL input; directory/native detail fields then share a suffix.
        assert_eq!(port.put_entries(owner, &entries).unwrap(), vec![1]);
        let jobs = port.jobs(owner, None, Limits::MAXIMUM).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].position, 1);
        assert_eq!(jobs[0].native_path, b"/source/f0001");
        assert_eq!(
            port.file_roots(owner, None, Limits::MAXIMUM).unwrap()[0].aliases,
            1
        );
        let directories = port.directories(owner, None, Limits::MAXIMUM).unwrap();
        assert_eq!(directories.len(), 2);
        assert_eq!(directories[1].native_path, directory_path);
        let children = port
            .unplaced_children(owner, 0, None, Limits::MAXIMUM)
            .unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, b"f0019");
        assert_eq!(children[0].native, Some(native));
        let held = port.work(owner).unwrap();
        assert_eq!(held.held_rows, 22);
        assert_eq!(
            port.place_children(
                owner,
                0,
                &[Placed {
                    name: children[0].name.clone(),
                    position: 19,
                    native: Some((native, native_path.clone())),
                }]
            )
            .unwrap(),
            vec![19]
        );
        let rows = port.entries(owner, None, Limits::MAXIMUM).unwrap();
        assert_eq!(rows.len(), 21);
        let regular = rows.iter().find(|row| row.key.name == b"f0019").unwrap();
        assert_eq!((regular.position, regular.canonical), (19, Some(19)));
        for row in rows.iter().filter(|row| row.kind == InodeKind::Symlink) {
            assert_eq!(row.metadata_root, object(b"link-metadata"));
            assert_eq!(row.content_root, Some(object(b"opaque-link-target")));
        }
        let after_placement = port.work(owner).unwrap();
        assert_eq!(after_placement.held_rows, held.held_rows + 1);
        assert_eq!(
            after_placement.held_bytes,
            held.held_bytes + native_path.len() as u64
        );
        let removed = port.discard(owner, 4096).unwrap();
        assert_eq!(
            (removed.rows, removed.bytes),
            (after_placement.held_rows, after_placement.held_bytes)
        );
        assert_eq!(port.work(owner).unwrap().held_bytes, 0);
        port.release(owner).unwrap();
    }
}

#[test]
fn directory_uniqueness_and_native_evidence_keep_the_first_refusal_after_a_prefix() {
    use layerfs_storage::port::PersistenceError;
    let temp = Temp::new("acquisition-mixed-refusal-order");
    let handles = create(&temp, SqlitePersistenceProfile::Disposable);
    let port = &handles.acquisition;
    let owner = begin(&handles);
    port.put_entries(
        owner,
        &[
            directory(None, b"", 0),
            directory(Some(0), b"d0001", 1),
            file(2, 2),
        ],
    )
    .unwrap();
    let before = port.work(owner).unwrap();
    let mut changed = file(19, 2);
    changed.native.as_mut().unwrap().evidence[0] ^= 1;
    let duplicate = directory(Some(0), b"another-name", 1);
    let malformed = AcquisitionError::Persistence(PersistenceError::Malformed);
    for (tail, expected) in [
        (vec![duplicate.clone(), changed.clone()], malformed.clone()),
        (
            vec![changed.clone(), duplicate],
            AcquisitionError::Changed { position: 19 },
        ),
        (
            vec![
                directory(Some(0), b"first", 30),
                directory(Some(0), b"second", 30),
                changed,
            ],
            malformed,
        ),
    ] {
        let mut entries: Vec<_> = (3..=18).map(symlink).collect();
        entries.extend(tail);
        assert_eq!(port.put_entries(owner, &entries), Err(expected));
        assert_eq!(port.work(owner).unwrap(), before);
        assert_eq!(port.entries(owner, None, Limits::MAXIMUM).unwrap().len(), 3);
        assert_eq!(
            port.file_roots(owner, None, Limits::MAXIMUM).unwrap()[0].aliases,
            0
        );
    }
}

#[test]
fn windows_amortize_execution_without_losing_aliases_or_atomic_completion() {
    for profile in DEVELOPMENT_PROFILES {
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

#[test]
fn independent_entries_share_executions_and_population_work_stays_indexed() {
    let mut work = Vec::new();
    for population in [2000_u64, 20000] {
        let temp = Temp::new("acquisition-entry-index-work");
        let handles = create(&temp, SqlitePersistenceProfile::Disposable);
        let port = &handles.acquisition;
        let owner = begin(&handles);
        port.put_entries(owner, &[directory(None, b"", 0)]).unwrap();
        let mut next = 1;
        while next <= population {
            let end = (next + 999).min(population);
            let entries: Vec<_> = (next..=end).map(|n| file(n, n)).collect();
            port.put_entries(owner, &entries).unwrap();
            next = end + 1;
        }
        let entries: Vec<_> = (population + 1..=population + 32)
            .map(|n| file(n, n))
            .collect();
        let before = handles.diagnostics().unwrap();
        assert_eq!(
            port.put_entries(owner, &entries).unwrap(),
            (population + 1..=population + 32).collect::<Vec<_>>()
        );
        let after = handles.diagnostics().unwrap();
        let delta = [
            after.statements - before.statements,
            after.vm_steps - before.vm_steps,
            after.fullscan_steps - before.fullscan_steps,
            after.sorts - before.sorts,
            after.autoindex_rows - before.autoindex_rows,
            after.reprepares - before.reprepares,
        ];
        println!("ENTRY_WINDOW_PROFILE population={population} [statements,vm_steps,fullscan,sorts,autoindex,reprepares]={delta:?} bound_bytes={}", after.bound_bytes - before.bound_bytes);
        assert!(
            delta[0] <= 8,
            "independent entries execute per row: {delta:?}"
        );
        assert_eq!(&delta[3..], &[0, 0, 0]);
        work.push(delta);
    }
    assert_eq!(work[0][0], work[1][0]);
    assert_eq!(work[0][2], work[1][2]);
    assert!(work[1][1] <= work[0][1] + work[0][1] / 10);
}

#[test]
fn entry_refusal_order_and_full_unit_rollback_survive_execution_windows() {
    use layerfs_storage::port::PersistenceError;
    let temp = Temp::new("acquisition-entry-refusal-order");
    let handles = create(&temp, SqlitePersistenceProfile::Disposable);
    let port = &handles.acquisition;
    let owner = begin(&handles);
    port.put_entries(owner, &[directory(None, b"", 0), file(1, 1)])
        .unwrap();
    let before = port.work(owner).unwrap();
    let prefix: Vec<_> = (2..=40).map(|n| file(n, n)).collect();
    let mut changed = file(41, 1);
    changed.native.as_mut().unwrap().evidence[0] ^= 1;
    let mut invalid = file(42, 42);
    invalid.kind = InodeKind::Directory;
    let duplicate = file(1, 43);
    let malformed = AcquisitionError::Persistence(PersistenceError::Malformed);
    for (tail, expected) in [
        (
            vec![changed.clone(), invalid.clone()],
            AcquisitionError::Changed { position: 41 },
        ),
        (
            vec![invalid.clone(), changed.clone()],
            AcquisitionError::Bounds,
        ),
        (vec![duplicate.clone(), changed.clone()], malformed.clone()),
        (
            vec![changed.clone(), duplicate.clone()],
            AcquisitionError::Changed { position: 41 },
        ),
        (
            vec![file(50, 50), file(50, 51), invalid.clone()],
            malformed.clone(),
        ),
        (
            vec![file(51, 51), file(52, 51), changed.clone()],
            AcquisitionError::Changed { position: 41 },
        ),
    ] {
        let mut entries = prefix.clone();
        entries.extend(tail);
        assert_eq!(port.put_entries(owner, &entries), Err(expected));
        assert_eq!(port.work(owner).unwrap(), before);
        assert_eq!(port.entries(owner, None, Limits::MAXIMUM).unwrap().len(), 2);
        assert_eq!(
            port.file_roots(owner, None, Limits::MAXIMUM).unwrap()[0].aliases,
            0
        );
    }
    // Fresh and existing aliases cross SQL windows, preserving the first path.
    let mut entries = prefix;
    entries.push(file(41, 2));
    entries.push(file(42, 1));
    let result = port.put_entries(owner, &entries).unwrap();
    assert_eq!(&result[result.len() - 2..], &[2, 1]);
    assert_eq!(
        port.file_roots(owner, None, Limits::MAXIMUM).unwrap()[0].aliases,
        1
    );
}

/// Owner direction 2026-10-07: Disposable is the sole active development
/// verification profile. Durable execution of these bodies is NOT_RUN —
/// deferred by owner for Disposable-only development. Durable support and its
/// retained historical receipts are unchanged.
const DEVELOPMENT_PROFILES: [SqlitePersistenceProfile; 1] = [SqlitePersistenceProfile::Disposable];
