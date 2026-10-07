//! Acquisition units over a real Store: order, identity, charges, cleanup, fencing.
#![cfg(target_os = "macos")]
mod support;
use layerfs_content::{inode_leaf::InodeKind, ObjectId};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{
    Handles, PersistenceConfig, SqlWork, SqliteAcquisitionSchema, SqlitePersistenceProfile,
};
use layerfs_storage::{
    port::{
        acquisition::{
            Acquisition, AcquisitionError, Begin, EntryKey, Limits, NativeIdentity, NewEntry,
            Owner, Phase, Placed, EVIDENCE_BYTES, WRITE_WINDOW_ROWS,
        },
        PersistenceError,
    },
    StoragePolicy,
};
use std::path::Path;
use support::Temp;

const BINDING: &[u8] = b"acquisition-units";

fn config(path: &Path) -> PersistenceConfig {
    selected(path, SqlitePersistenceProfile::Disposable)
}

fn selected(path: &Path, profile: SqlitePersistenceProfile) -> PersistenceConfig {
    PersistenceConfig::sqlite(path)
        .with_sqlite_profile(profile)
        .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables)
}

fn create(path: &Path) -> Handles {
    create_selected(path, SqlitePersistenceProfile::Disposable)
}

fn create_selected(path: &Path, profile: SqlitePersistenceProfile) -> Handles {
    Handles::create(
        selected(path, profile),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: [67; 32],
            incarnation: 1,
        },
    )
    .unwrap()
}

fn begin(handles: &Handles) -> Owner {
    handles
        .acquisition
        .begin(&Begin {
            source_device: u64::MAX,
            source_inode: 1 << 63,
            stack: [7; 16],
            scope: ObjectId::for_bytes(b"scope"),
        })
        .unwrap()
}

fn root(label: &str) -> ObjectId {
    ObjectId::for_bytes(label.as_bytes())
}

fn key(parent: Option<u64>, name: &str) -> EntryKey {
    EntryKey {
        parent,
        name: name.as_bytes().to_vec(),
    }
}

fn native(device: u64, inode: u64, evidence: u8) -> NativeIdentity {
    NativeIdentity {
        device,
        inode,
        evidence: [evidence; EVIDENCE_BYTES],
    }
}

fn directory(parent: Option<u64>, name: &str, position: Option<u64>, path: &str) -> NewEntry {
    NewEntry {
        key: key(parent, name),
        position,
        kind: InodeKind::Directory,
        metadata_root: root("directory-attributes"),
        target_root: None,
        native_path: Some(path.as_bytes().to_vec()),
        native: None,
    }
}

fn regular(
    parent: u64,
    name: &str,
    position: Option<u64>,
    path: &str,
    identity: NativeIdentity,
) -> NewEntry {
    NewEntry {
        key: key(Some(parent), name),
        position,
        kind: InodeKind::RegularFile,
        metadata_root: root("file-attributes"),
        target_root: None,
        native_path: Some(path.as_bytes().to_vec()),
        native: Some(identity),
    }
}

fn symlink(parent: u64, name: &str, position: Option<u64>) -> NewEntry {
    NewEntry {
        key: key(Some(parent), name),
        position,
        kind: InodeKind::Symlink,
        metadata_root: root("link-attributes"),
        target_root: Some(root(name)),
        native_path: None,
        native: None,
    }
}

/// Rows and payload bytes one operation holds, summed by the engine itself.
fn stored(path: &Path, operation: u64) -> (u64, u64) {
    let db = rusqlite::Connection::open(path).unwrap();
    let entries: (i64, i64) = db
        .query_row(
            "SELECT count(*),coalesce(sum(length(name)+coalesce(length(native_path),0)\
             +coalesce(length(native),0)+length(metadata_root)+coalesce(length(content_root),0)),0)\
             FROM init_entry WHERE operation_id=?1",
            [operation as i64],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let files: (i64, i64) = db
        .query_row(
            "SELECT count(*),coalesce(sum(length(native_path)+length(device)+length(inode)\
             +length(evidence)+coalesce(length(file_root),0)),0)\
             FROM init_native_file WHERE operation_id=?1",
            [operation as i64],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    ((entries.0 + files.0) as u64, (entries.1 + files.1) as u64)
}

fn refused(error: AcquisitionError) -> String {
    match error {
        AcquisitionError::Persistence(PersistenceError::Refused { status }) => status,
        other => panic!("expected a refusal, found {other:?}"),
    }
}

#[test]
fn a_tree_streams_in_acquisition_order_with_exact_charges_and_bounded_cleanup() {
    // Working rows follow the Store's selected profile; both behave the same.
    for profile in DEVELOPMENT_PROFILES {
        tree_under(profile);
    }
}

fn tree_under(profile: SqlitePersistenceProfile) {
    let temp = Temp::new("acquisition-tree");
    let path = temp.join("store.sqlite");
    let handles = create_selected(&path, profile);
    assert_eq!(handles.profile().persistence, profile);
    let port = &handles.acquisition;
    assert_eq!(port.placement().as_deref(), Some(temp.path()));
    let owner = begin(&handles);
    assert_eq!((owner.operation, owner.epoch), (1, 1));

    let shared = native(u64::MAX, 7, 1);
    let own = native(1, (1 << 63) + 5, 1);
    let bound = port
        .put_entries(
            owner,
            &[
                directory(None, "", Some(0), "/s"),
                directory(Some(0), "a", Some(1), "/s/a"),
                regular(0, "b", Some(2), "/s/b", shared),
                symlink(0, "c", Some(3)),
            ],
        )
        .unwrap();
    assert_eq!(bound, [2], "the first path is its own canonical position");
    let bound = port
        .put_entries(
            owner,
            &[
                regular(1, "x", Some(4), "/s/a/x", shared),
                regular(1, "y", Some(5), "/s/a/y", own),
            ],
        )
        .unwrap();
    assert_eq!(bound, [2, 5], "a later path shares the first position");

    // Different evidence for a known identity changes nothing at all.
    let before = (port.work(owner).unwrap(), stored(&path, 1));
    let changed = regular(1, "z", Some(6), "/s/a/z", native(u64::MAX, 7, 2));
    assert_eq!(
        port.put_entries(owner, &[symlink(1, "w", Some(7)), changed]),
        Err(AcquisitionError::Changed { position: 6 })
    );
    assert_eq!((port.work(owner).unwrap(), stored(&path, 1)), before);
    // A second binding of one name in one parent is an integrity refusal.
    assert_eq!(
        port.put_entries(owner, &[symlink(0, "c", Some(6))]),
        Err(AcquisitionError::Persistence(PersistenceError::Malformed))
    );
    assert_eq!(port.work(owner).unwrap(), before.0);

    let entries = port.entries(owner, None, Limits::MAXIMUM).unwrap();
    let order: Vec<_> = entries
        .iter()
        .map(|entry| {
            (
                entry.key.parent,
                String::from_utf8(entry.key.name.clone()).unwrap(),
                entry.position,
                entry.kind,
                entry.canonical,
            )
        })
        .collect();
    use InodeKind::{Directory, RegularFile, Symlink};
    assert_eq!(
        order,
        [
            (None, String::new(), 0, Directory, None),
            (Some(0), "a".into(), 1, Directory, None),
            (Some(0), "b".into(), 2, RegularFile, Some(2)),
            (Some(0), "c".into(), 3, Symlink, None),
            (Some(1), "x".into(), 4, RegularFile, Some(2)),
            (Some(1), "y".into(), 5, RegularFile, Some(5)),
        ]
    );
    assert_eq!(entries[3].content_root, Some(root("c")));
    assert_eq!(entries[1].content_root, None, "no directory root yet");
    // A window resumes after the last key it returned.
    let limits = Limits { rows: 2, bytes: 1 };
    let next = port.entries(owner, Some(&entries[2].key), limits).unwrap();
    assert_eq!(next.len(), 1, "the byte limit still returns one row");
    assert_eq!(next[0].position, 3);
    let limits = Limits {
        rows: 2,
        ..Limits::MAXIMUM
    };
    let next = port.entries(owner, Some(&entries[2].key), limits).unwrap();
    assert_eq!(
        next.iter().map(|entry| entry.position).collect::<Vec<_>>(),
        [3, 4]
    );

    let directories = port.directories(owner, None, Limits::MAXIMUM).unwrap();
    assert_eq!(
        directories
            .iter()
            .map(|d| (d.position, d.native_path.as_slice()))
            .collect::<Vec<_>>(),
        [(0, b"/s".as_slice()), (1, b"/s/a".as_slice())]
    );
    assert!(port
        .directories(owner, Some(1), Limits::MAXIMUM)
        .unwrap()
        .is_empty());
    assert_eq!(port.directory_path(owner, 1).unwrap().unwrap(), b"/s/a");
    assert_eq!(port.directory_path(owner, 2).unwrap(), None);

    let jobs = port.jobs(owner, None, Limits::MAXIMUM).unwrap();
    assert_eq!(jobs.len(), 2);
    assert_eq!((jobs[0].position, jobs[0].native), (2, shared));
    assert_eq!(jobs[0].native_path, b"/s/b");
    assert_eq!((jobs[1].position, jobs[1].native), (5, own));
    assert_eq!(port.job(owner, 5).unwrap().unwrap().native_path, b"/s/a/y");
    assert_eq!(port.job(owner, 4).unwrap(), None, "a later path is no job");

    let roots = port.file_roots(owner, None, Limits::MAXIMUM).unwrap();
    assert_eq!(
        roots
            .iter()
            .map(|r| (r.position, r.aliases, r.root))
            .collect::<Vec<_>>(),
        [(2, 1, None), (5, 0, None)]
    );
    port.complete_files(owner, &[(5, root("y")), (2, root("b"))])
        .unwrap();
    assert_eq!(
        port.complete_files(owner, &[(2, root("again"))]),
        Err(AcquisitionError::Changed { position: 2 }),
        "a root is recorded once"
    );
    port.set_directory_roots(owner, &[(0, root("d0")), (1, root("d1"))])
        .unwrap();
    assert_eq!(
        port.set_directory_roots(owner, &[(3, root("not a directory"))]),
        Err(AcquisitionError::Changed { position: 3 })
    );
    let roots = port.file_roots(owner, Some(2), Limits::MAXIMUM).unwrap();
    assert_eq!(roots[0].root, Some(root("y")));
    let entries = port.entries(owner, None, Limits::MAXIMUM).unwrap();
    assert_eq!(entries[1].content_root, Some(root("d1")));
    port.advance(owner, Phase::Tree).unwrap();

    // The charge equals what the engine itself sums for this operation.
    let work = port.work(owner).unwrap();
    assert_eq!((work.held_rows, work.held_bytes), stored(&path, 1));
    assert_eq!(work.held_rows, 8, "six entries and two native identities");
    assert_eq!((work.peak_rows, work.peak_bytes), (8, work.held_bytes));

    assert_eq!(
        refused(port.release(owner).unwrap_err()),
        "acquisition working rows remain"
    );
    port.advance(owner, Phase::Discarding).unwrap();
    let mut removed = (0, 0);
    let mut jobs = 0;
    loop {
        let job = port.discard(owner, 3).unwrap();
        assert!(job.rows <= 3);
        removed = (removed.0 + job.rows, removed.1 + job.bytes);
        jobs += 1;
        assert_eq!(job.remaining_rows, stored(&path, 1).0);
        if job.remaining_rows == 0 {
            break;
        }
    }
    assert_eq!(jobs, 3, "eight rows leave in budgeted jobs of three");
    assert_eq!(removed, (8, work.held_bytes));
    let after = port.work(owner).unwrap();
    assert_eq!((after.held_rows, after.held_bytes), (0, 0));
    assert_eq!(
        (after.removed_rows, after.removed_bytes),
        (8, work.held_bytes)
    );
    assert_eq!(port.discard(owner, 3).unwrap().rows, 0);
    port.release(owner).unwrap();
    assert_eq!(port.work(owner), Err(AcquisitionError::Stale));
    assert_eq!(port.release(owner), Err(AcquisitionError::Stale));

    // The shared tables stay, and the next operation gets a new identity.
    let next = begin(&handles);
    assert_eq!((next.operation, next.epoch), (2, 1));
    port.release(next).unwrap();
}

#[test]
fn a_wide_directory_is_placed_through_name_windows() {
    const CHILDREN: usize = 1500;
    let temp = Temp::new("acquisition-wide");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    let port = &handles.acquisition;
    let owner = begin(&handles);
    let mut window = vec![
        directory(None, "", Some(0), "/s"),
        directory(Some(0), "w", Some(1), "/s/w"),
    ];
    // Observed in descending order, as an unsorted host enumeration would be.
    for n in (0..CHILDREN).rev() {
        let name = format!("n{n:05}");
        window.push(match n % 3 {
            // Two names share one native identity; every other file is its own.
            0 => {
                let inode = if n == 3 { 0 } else { n as u64 };
                regular(1, &name, None, "unused", native(9, inode, 4))
            }
            1 => symlink(1, &name, None),
            _ => directory(Some(1), &name, None, &format!("/s/w/{name}")),
        });
    }
    assert!(port.put_entries(owner, &window).unwrap().is_empty());
    let unplaced = port.work(owner).unwrap();
    assert_eq!((unplaced.held_rows, unplaced.held_bytes), stored(&path, 1));

    let limits = Limits {
        rows: 400,
        ..Limits::MAXIMUM
    };
    let (mut after, mut position, mut windows) = (None::<Vec<u8>>, 2_u64, 0);
    let mut canonical = Vec::new();
    loop {
        let children = port
            .unplaced_children(owner, 1, after.as_deref(), limits)
            .unwrap();
        let Some(last) = children.last() else { break };
        assert!(children.len() <= 400);
        after = Some(last.name.clone());
        let placed: Vec<Placed> = children
            .into_iter()
            .map(|child| {
                let own = position;
                position += 1;
                let path = [b"/s/w/".as_slice(), &child.name].concat();
                Placed {
                    native: child.native.map(|identity| (identity, path)),
                    name: child.name,
                    position: own,
                }
            })
            .collect();
        canonical.extend(port.place_children(owner, 1, &placed).unwrap());
        windows += 1;
    }
    assert_eq!(windows, 4);
    assert_eq!(position as usize, CHILDREN + 2);
    assert_eq!(canonical.len(), CHILDREN / 3);
    // n00000 is at position 2; n00003, at position 5, is a later path of it.
    assert_eq!(&canonical[..3], [2, 2, 8]);
    assert!(port
        .unplaced_children(owner, 1, None, limits)
        .unwrap()
        .is_empty());
    assert_eq!(
        port.place_children(
            owner,
            1,
            &[Placed {
                name: b"absent".to_vec(),
                position,
                native: None
            }]
        ),
        Err(AcquisitionError::Changed { position })
    );

    // Every entry is now in name order at contiguous positions.
    let (mut after, mut expected) = (None::<EntryKey>, 0_u64);
    loop {
        let entries = port
            .entries(owner, after.as_ref(), Limits::MAXIMUM)
            .unwrap();
        let Some(last) = entries.last() else { break };
        after = Some(last.key.clone());
        for entry in entries {
            assert_eq!(entry.position, expected);
            if expected >= 2 {
                let name = format!("n{:05}", expected - 2);
                assert_eq!(entry.key, key(Some(1), &name));
            }
            expected += 1;
        }
    }
    assert_eq!(expected as usize, CHILDREN + 2);
    let placed = port.work(owner).unwrap();
    assert_eq!((placed.held_rows, placed.held_bytes), stored(&path, 1));
    assert_eq!(
        placed.held_rows as usize,
        CHILDREN + 2 + CHILDREN / 3 - 1,
        "one native row per identity"
    );
    let directories = port.directories(owner, Some(1), limits).unwrap();
    assert_eq!(directories[0].position, 4);
    assert_eq!(directories[0].native_path, b"/s/w/n00002");
    assert_eq!(
        port.job(owner, 2).unwrap().unwrap().native_path,
        b"/s/w/n00000"
    );
    let roots = port.file_roots(owner, None, Limits::MAXIMUM).unwrap();
    assert_eq!((roots[0].position, roots[0].aliases), (2, 1));

    let mut remaining = placed.held_rows;
    while remaining != 0 {
        remaining = port.discard(owner, 512).unwrap().remaining_rows;
    }
    assert_eq!(stored(&path, 1), (0, 0));
    assert_eq!(port.work(owner).unwrap().held_bytes, 0);
    port.release(owner).unwrap();
}

#[test]
fn windows_owners_and_abandoned_operations_are_fenced() {
    let temp = Temp::new("acquisition-fences");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    let port = &handles.acquisition;
    let owner = begin(&handles);
    port.put_entries(owner, &[directory(None, "", Some(0), "/s")])
        .unwrap();

    let oversized: Vec<NewEntry> = (0..=WRITE_WINDOW_ROWS as u64)
        .map(|n| symlink(0, &format!("l{n}"), Some(n + 1)))
        .collect();
    assert_eq!(
        port.put_entries(owner, &oversized),
        Err(AcquisitionError::Bounds)
    );
    // A regular file without its identity is malformed and changes nothing.
    let mut malformed = regular(0, "f", Some(1), "/s/f", native(1, 1, 1));
    malformed.native = None;
    assert_eq!(
        port.put_entries(owner, &[malformed]),
        Err(AcquisitionError::Bounds)
    );
    for stale in [
        Owner {
            operation: owner.operation,
            epoch: owner.epoch + 1,
        },
        Owner {
            operation: owner.operation + 9,
            epoch: owner.epoch,
        },
    ] {
        assert_eq!(
            port.put_entries(stale, &[symlink(0, "s", Some(1))]),
            Err(AcquisitionError::Stale)
        );
        assert_eq!(port.discard(stale, 8), Err(AcquisitionError::Stale));
        assert_eq!(
            port.entries(stale, None, Limits::MAXIMUM),
            Err(AcquisitionError::Stale)
        );
    }
    assert_eq!(stored(&path, 1).0, 1, "no refused unit left a row");
    // This session's own operations are never abandoned.
    assert!(port.abandoned(None, 8).unwrap().is_empty());
    assert_eq!(
        port.discard_abandoned(owner, 8),
        Err(AcquisitionError::Stale)
    );
    port.put_entries(
        owner,
        &[
            symlink(0, "a", Some(1)),
            regular(0, "b", Some(2), "/s/b", native(1, 2, 3)),
        ],
    )
    .unwrap();
    port.advance(owner, Phase::Files).unwrap();
    let held = port.work(owner).unwrap();
    drop(handles);

    // A new session neither adopts nor removes the earlier operation.
    let handles = Handles::open_writable(config(&path), BINDING, [67; 32]).unwrap();
    let port = &handles.acquisition;
    assert_eq!(
        port.work(owner),
        Err(AcquisitionError::Stale),
        "an earlier session's owner is not live here"
    );
    let abandoned = port.abandoned(None, 8).unwrap();
    assert_eq!(abandoned.len(), 1);
    assert_eq!(abandoned[0].owner, owner);
    assert_eq!(abandoned[0].phase, Phase::Files);
    assert_eq!(
        (abandoned[0].held_rows, abandoned[0].held_bytes),
        (held.held_rows, held.held_bytes)
    );
    let live = begin(&handles);
    assert_eq!((live.operation, live.epoch), (2, 2));
    assert_eq!(port.abandoned(None, 8).unwrap().len(), 1);
    assert!(port.abandoned(Some(1), 8).unwrap().is_empty());
    assert_eq!(stored(&path, 1).0, 4, "nothing was removed by opening");

    // Removal happens only when the caller names the abandoned operation.
    let first = port.discard_abandoned(owner, 3).unwrap();
    assert_eq!((first.rows, first.remaining_rows), (3, 1));
    assert_eq!(port.abandoned(None, 8).unwrap()[0].held_rows, 1);
    let last = port.discard_abandoned(owner, 3).unwrap();
    assert_eq!((last.rows, last.remaining_rows), (1, 0));
    assert_eq!(first.bytes + last.bytes, held.held_bytes);
    assert!(port.abandoned(None, 8).unwrap().is_empty());
    assert_eq!(
        port.discard_abandoned(owner, 3),
        Err(AcquisitionError::Stale)
    );
    port.release(live).unwrap();
}

fn delta(before: SqlWork, after: SqlWork) -> [u64; 6] {
    [
        after.statements - before.statements,
        after.vm_steps - before.vm_steps,
        after.fullscan_steps - before.fullscan_steps,
        after.sorts - before.sorts,
        after.autoindex_rows - before.autoindex_rows,
        after.reprepares - before.reprepares,
    ]
}

/// Appends `count` symlinks under the root in full write windows.
fn populate(handles: &Handles, owner: Owner, count: u64) {
    let port = &handles.acquisition;
    port.put_entries(owner, &[directory(None, "", Some(0), "/s")])
        .unwrap();
    let mut next = 1;
    while next <= count {
        let end = (next + WRITE_WINDOW_ROWS as u64 - 1).min(count);
        let window: Vec<NewEntry> = (next..=end)
            .map(|n| symlink(0, &format!("l{n:08}"), Some(n)))
            .collect();
        port.put_entries(owner, &window).unwrap();
        next = end + 1;
    }
}

#[test]
fn every_statement_is_an_indexed_search_and_window_work_ignores_population() {
    let temp = Temp::new("acquisition-plans");
    let handles = create(&temp.join("store.sqlite"));
    let port = &handles.acquisition;
    for (name, plan) in handles.explain_acquisition().unwrap() {
        println!("ACQUISITION_PLAN {name}: {plan:?}");
        let entry_window = matches!(name, "entry_dependencies" | "put_entries" | "put_native");
        let root_window = matches!(name, "complete_file" | "set_directory_root");
        if root_window {
            assert!(
                plan.iter().any(|line| line.starts_with("SEARCH target ")),
                "{name} must search its indexed target: {plan:?}"
            );
        }
        for line in &plan {
            let constant_input = (root_window
                && matches!(line.as_str(), "SCAN 32 CONSTANT ROWS" | "SCAN roots"))
                || (entry_window
                    && matches!(
                        line.as_str(),
                        "SCAN 32 CONSTANT ROWS" | "SCAN input" | "SCAN source"
                    ));
            assert!(
                !line.starts_with("SCAN") || constant_input,
                "{name} scans stored state: {line}"
            );
            assert!(!line.contains("TEMP B-TREE"), "{name} sorts: {line}");
            assert!(
                !line.contains("AUTOMATIC"),
                "{name} builds an index: {line}"
            );
        }
    }

    // The same middle window and the same removal job at two populations.
    let mut observed = Vec::new();
    for count in [2_000_u64, 20_000] {
        let owner = begin(&handles);
        let before = handles.diagnostics().unwrap();
        populate(&handles, owner, count);
        let written = delta(before, handles.diagnostics().unwrap());
        let middle = key(Some(0), &format!("l{:08}", count / 2));
        let before = handles.diagnostics().unwrap();
        let window = port.entries(owner, Some(&middle), Limits::MAXIMUM).unwrap();
        let read = delta(before, handles.diagnostics().unwrap());
        assert_eq!(window.len(), 512);
        assert_eq!(window[0].position, count / 2 + 1);
        let before = handles.diagnostics().unwrap();
        assert_eq!(port.discard(owner, 512).unwrap().rows, 512);
        let removed = delta(before, handles.diagnostics().unwrap());
        println!(
            "ACQUISITION_PROFILE entries={count} \
             [statements,vm_steps,fullscan,sorts,autoindex,reprepares] \
             write={written:?} read_window={read:?} discard_job={removed:?}"
        );
        // A bound LIMIT the planner could read would re-prepare its statement
        // on every execution and let the plan follow the value.
        assert_eq!(&written[3..], &[0, 0, 0]);
        for work in [read, removed] {
            assert_eq!(
                &work[2..],
                [0, 0, 0, 0],
                "no scan, sort, automatic index or reprepare"
            );
        }
        observed.push((read, removed));
        let mut remaining = 1;
        while remaining != 0 {
            remaining = port
                .discard(owner, WRITE_WINDOW_ROWS)
                .unwrap()
                .remaining_rows;
        }
        port.release(owner).unwrap();
    }
    // One window is BEGIN, the owner check, the search and COMMIT, whatever is
    // stored; its engine steps do not grow with ten times the rows.
    let [(small_read, small_removed), (large_read, large_removed)] = observed[..] else {
        panic!("two populations");
    };
    assert_eq!(small_read[0], large_read[0]);
    assert_eq!(small_removed[0], large_removed[0]);
    assert!(large_read[1] <= small_read[1] + small_read[1] / 10);
    assert!(large_removed[1] <= small_removed[1] + small_removed[1] / 10);
}

/// Owner direction 2026-10-07: Disposable is the sole active development
/// verification profile. Durable execution of these bodies is NOT_RUN —
/// deferred by owner for Disposable-only development. Durable support and its
/// retained historical receipts are unchanged.
const DEVELOPMENT_PROFILES: [SqlitePersistenceProfile; 1] = [SqlitePersistenceProfile::Disposable];
