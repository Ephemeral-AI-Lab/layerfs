//! Counted work of one small and one large change committed through the
//! product constructor `BoundWorkspace::commit_captured`, over one base size,
//! a real Disposable Store and the real local owner, without a mount. Counts
//! are printed as evidence; only exact arithmetic is asserted. Nothing is timed.
#[allow(dead_code)]
#[path = "support/captured_drive.rs"]
mod drive;
#[path = "support/namespace_model.rs"]
mod model;
#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod support;
use drive::{evidence, settled, Do, Rig, Session};
use layerfs_content::ObjectId;
use layerfs_daemon::{store::ReleasedOwner, Completion, OwnerWork, ServiceClass};
use layerfs_history::CommitStagedOutcome;
use layerfs_overlay::{DatabaseWork, StatementKind};
use layerfs_persistence::SqlitePersistenceProfile;
use layerfs_workspace::CapturedNamespaceWork;
use model::{Flat, Seen, FILE, SYMLINK};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::Path,
};

/// The service window of one captured page (architecture 78: a captured
/// sequence ends on a page shorter than 64 rows).
const PAGE_ROWS: u64 = 64;
/// Files below `bulk/`, fifty to a directory, for both cases.
const BULK: usize = 1_600;
/// Directories of `bulk/` the large change touches.
const LARGE_DIRECTORIES: usize = 24;

const CLASSES: [(ServiceClass, &str); 6] = [
    (ServiceClass::Read, "read"),
    (ServiceClass::Mutation, "mutation"),
    (ServiceClass::Capture, "capture"),
    (ServiceClass::Lifecycle, "lifecycle"),
    (ServiceClass::OperationRecord, "operation_record"),
    (ServiceClass::Source, "source"),
];
const FAMILIES: [StatementKind; 14] = [
    StatementKind::Startup,
    StatementKind::Begin,
    StatementKind::Commit,
    StatementKind::Rollback,
    StatementKind::Workspace,
    StatementKind::Inode,
    StatementKind::DirectoryEntry,
    StatementKind::Payload,
    StatementKind::Frontier,
    StatementKind::Capture,
    StatementKind::OperationRecord,
    StatementKind::Lease,
    StatementKind::Explain,
    StatementKind::Reclaim,
];

fn pattern(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|n| (n as u32).wrapping_mul(31).wrapping_add(u32::from(seed)) as u8)
        .collect()
}
/// The base of `captured_commit.rs`'s cost test: directories, files of several
/// sizes, symlinks, hard links and `bulk/d-NNN/f-NN`, fifty files a directory.
fn tree(source: &Path, bulk: usize) {
    for directory in [
        "a",
        "a/b",
        "a/b/c",
        "keep",
        "keep/inner",
        "gone",
        "gone/sub",
        "wide",
        "empty",
    ] {
        fs::create_dir_all(source.join(directory)).unwrap();
    }
    for (path, bytes) in [
        ("top.txt", b"top level\n".to_vec()),
        ("a/one", b"one: sixteen byt\n".to_vec()),
        ("a/b/two", b"two contents\n".to_vec()),
        ("a/b/c/three", pattern(3, 300_000)),
        ("keep/inner/k", pattern(9, 5_000)),
        ("gone/x", b"x".to_vec()),
        ("gone/sub/y", b"y".to_vec()),
        ("empty-file", Vec::new()),
    ] {
        fs::write(source.join(path), bytes).unwrap();
    }
    for n in 0..80 {
        fs::write(source.join(format!("wide/w-{n:06}")), format!("wide {n}\n")).unwrap();
    }
    for n in 0..bulk {
        let directory = source.join(format!("bulk/d-{:03}", n / 50));
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join(format!("f-{:02}", n % 50)),
            format!("bulk {n}\n"),
        )
        .unwrap();
    }
    symlink("a/one", source.join("link")).unwrap();
    symlink("../..", source.join("a/b/back")).unwrap();
    symlink("x", source.join("gone/l")).unwrap();
    fs::hard_link(source.join("top.txt"), source.join("a/alias")).unwrap();
    fs::hard_link(source.join("a/b/two"), source.join("keep/alias2")).unwrap();
    fs::set_permissions(source.join("a/one"), fs::Permissions::from_mode(0o600)).unwrap();
    fs::set_permissions(source.join("keep"), fs::Permissions::from_mode(0o750)).unwrap();
}

/// What the independent model says one change did to inodes, counted without
/// any product output. `moved` declares each directory rename of the change as
/// (final path, base path); a rename alone changes no inode.
#[derive(Debug, Default, Eq, PartialEq)]
struct Delta {
    /// Live inodes that are new or whose mode, mtime, links or bytes changed.
    changed: u64,
    /// The regular files among them.
    files: u64,
    /// The stored regular files among them whose bytes did not change.
    files_same_bytes: u64,
    /// The new inodes among them.
    fresh: u64,
    /// The new symlinks among them.
    fresh_symlinks: u64,
    /// Base inodes no final path names.
    removed: u64,
}
fn under(path: &[u8], prefix: &[u8]) -> bool {
    path == prefix
        || (path.len() > prefix.len() && path.starts_with(prefix) && path[prefix.len()] == b'/')
}
/// The base path of the inode a final path names, if the base named it there.
fn origin(path: &[u8], moved: &[(&str, &str)]) -> Option<Vec<u8>> {
    let covering = moved
        .iter()
        .filter(|(to, _)| under(path, to.as_bytes()))
        .max_by_key(|(to, _)| to.len());
    match covering {
        Some((to, from)) => {
            let mut base = from.as_bytes().to_vec();
            base.extend_from_slice(&path[to.len()..]);
            Some(base)
        }
        // A path below a name that moved away is not the base's path there.
        None if moved.iter().any(|(_, from)| under(path, from.as_bytes())) => None,
        None => Some(path.to_vec()),
    }
}
fn delta(base: &Flat, after: &Flat, moved: &[(&str, &str)]) -> Delta {
    // One row per final inode: its final state and its base state, if any of
    // its final paths was a base path of the same kind.
    let mut inodes = BTreeMap::<&[u8], (&Seen, Option<&Seen>)>::new();
    for (path, seen) in after {
        let stored = origin(path, moved)
            .and_then(|from| base.get(&from))
            .filter(|stored| stored.kind == seen.kind);
        let row = inodes
            .entry(seen.identity.as_slice())
            .or_insert((seen, None));
        if row.1.is_none() {
            row.1 = stored;
        }
    }
    let mut delta = Delta::default();
    let mut surviving = BTreeSet::<&[u8]>::new();
    for (seen, stored) in inodes.values() {
        match stored {
            Some(stored) => {
                surviving.insert(stored.identity.as_slice());
                let same_bytes = stored.payload == seen.payload;
                if same_bytes
                    && (stored.mode, stored.mtime, stored.links)
                        == (seen.mode, seen.mtime, seen.links)
                {
                    continue;
                }
                if seen.kind == FILE && same_bytes {
                    delta.files_same_bytes += 1;
                }
            }
            None => {
                delta.fresh += 1;
                if seen.kind == SYMLINK {
                    delta.fresh_symlinks += 1;
                }
            }
        }
        delta.changed += 1;
        if seen.kind == FILE {
            delta.files += 1;
        }
    }
    let stored = base
        .values()
        .map(|seen| seen.identity.as_slice())
        .collect::<BTreeSet<_>>();
    delta.removed = stored.difference(&surviving).count() as u64;
    delta
}

/// Statement families that observed work, counts only.
fn families(sql: &DatabaseWork) -> String {
    let mut line = String::new();
    for kind in FAMILIES {
        let row = sql.statements[kind as usize];
        if row.attempts == 0 && row.executions == 0 {
            continue;
        }
        write!(
            line,
            " {kind:?}(executions={} rows_returned={} rows_changed={} direct_rows_changed={} vm_steps={} fullscan_steps={} sorts={} autoindex_rows={} reprepares={})",
            row.executions,
            row.rows_returned,
            row.rows_changed,
            row.direct_rows_changed,
            row.vm_steps,
            row.fullscan_steps,
            row.sorts,
            row.autoindex_rows,
            row.reprepares
        )
        .unwrap();
    }
    line
}
/// One original completion's own receipt: its statement families and turns.
fn receipt(name: &str, done: &Completion) -> String {
    let work = done.work();
    format!(
        "{name}(result_ok={} parked_turns={} statements={}{})",
        done.result().is_ok(),
        work.parked_turns,
        work.sql.total().executions,
        families(&work.sql.expanded())
    )
}
fn jobs(before: &OwnerWork, after: &OwnerWork) -> [u64; 6] {
    std::array::from_fn(|class| after.completed[class] - before.completed[class])
}
/// The published root as a fresh bind sees it, walked completely.
fn rebound(rig: &Rig, tag: u8, root: ObjectId) -> Flat {
    let fresh = rig.bind(tag);
    assert_eq!(fresh.snapshot().unwrap().effective_root, root);
    let operation = fresh.operation().unwrap();
    let flat = model::canonical(operation.client(), root);
    drop(operation);
    rig.close(&fresh);
    flat
}

/// The counts of one Commit that the cross-case arithmetic needs.
struct Cost {
    base_paths: usize,
    work: CapturedNamespaceWork,
    jobs: u64,
    record_class_jobs: u64,
    statements: u64,
}
fn measure(
    case: &str,
    change: impl FnOnce(&mut Session<'_>),
    moved: &[(&str, &str)],
    expect_no_fullscan: bool,
) -> Cost {
    let rig = Rig::new(&format!("product-commit-cost-{case}"), |source| {
        tree(source, BULK)
    });
    assert_eq!(
        rig.f.config.sqlite_profile,
        SqlitePersistenceProfile::Disposable
    );
    let first = rig.bind(1);
    let base = rig.base.flat();
    let base_root = first.snapshot().unwrap().effective_root;
    let mut session = Session::new(&first, rig.base.clone());
    change(&mut session);
    let expected = session.model.flat();
    let counted = delta(&base, &expected, moved);
    // No complete walk of the live view precedes the Commit: it would fill the
    // shared canonical cache and change what the Commit's own reads find. The
    // cache holds what binding and the mutations above read, nothing more.
    settled(&rig.owner.client(), 0);
    let owner_before = rig.owner.client().diagnostics().unwrap();
    let (store_before, reads_before, cache_before) = (
        rig.store.work(),
        rig.store.read_work(),
        rig.store.cache_work().unwrap(),
    );
    let success = first
        .commit_captured()
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    // Read before the oracle's own complete walk of the published root.
    let (store_after, reads_after, cache_after) = (
        rig.store.work(),
        rig.store.read_work(),
        rig.store.cache_work().unwrap(),
    );
    let published = match &success.history {
        CommitStagedOutcome::Committed(record) => record.clone(),
        other => panic!("{other:?}"),
    };
    assert_ne!(published.root, base_root);
    let namespace = success.namespace.as_ref().expect("the product closure ran");
    let work = namespace.work.expect("the producer ran");
    let counters = namespace.counters.expect("the update succeeded");
    // Structure: the attempt's two owners were released by their own jobs.
    assert!(!namespace.retained(), "{namespace:?}");
    assert!(namespace.custody.is_none() && namespace.release_error.is_none());
    assert_eq!(
        namespace.released,
        [ReleasedOwner::Reader, ReleasedOwner::Operation]
    );
    assert!(namespace.release_failure.is_none());
    // A done release's completion is dropped by the constructor, so only the
    // capture and the install keep their own receipts here.
    let receipts = [
        receipt("capture", &success.captured),
        receipt("install", &success.installed),
    ];
    let (storage, saved) = (success.storage, success.saved);
    // The Commit's own completions are caller-held credits until dropped.
    drop(success);
    settled(&rig.owner.client(), 0);
    let owner_after = rig.owner.client().diagnostics().unwrap();
    model::assert_same(&expected, &rebound(&rig, 2, published.root), "fresh bind");

    let by_class = jobs(&owner_before, &owner_after);
    let total_jobs = by_class.iter().sum::<u64>();
    let sql = owner_after
        .sql_foreground
        .since(&owner_before.sql_foreground);
    let sql_total = sql.total();
    let mut classes = String::new();
    for (class, name) in CLASSES {
        write!(classes, " {name}={}", by_class[class as usize]).unwrap();
    }
    evidence(format_args!(
        "PRODUCT_COMMIT_COST case={case} base_paths={} final_paths={} steps={} model={counted:?}",
        base.len(),
        expected.len(),
        session.steps
    ));
    evidence(format_args!(
        "PRODUCT_COMMIT_COST_OWNER_JOBS case={case} total={total_jobs} admitted={}{classes}",
        owner_after.admitted - owner_before.admitted
    ));
    evidence(format_args!(
        "PRODUCT_COMMIT_COST_SQL case={case} statements={} rows_returned={} rows_changed={} vm_steps={} fullscan_steps={} sorts={} autoindex_rows={} reprepares={} families:{}",
        sql_total.executions,
        sql_total.rows_returned,
        sql_total.rows_changed,
        sql_total.vm_steps,
        sql_total.fullscan_steps,
        sql_total.sorts,
        sql_total.autoindex_rows,
        sql_total.reprepares,
        families(&sql)
    ));
    evidence(format_args!(
        "PRODUCT_COMMIT_COST_RECEIPTS case={case} {}",
        receipts.join(" ")
    ));
    evidence(format_args!(
        "PRODUCT_COMMIT_COST_PAYLOAD case={case} {:?}",
        owner_after
            .payload_foreground
            .since(owner_before.payload_foreground)
    ));
    evidence(format_args!(
        "PRODUCT_COMMIT_COST_STORE_READS case={case} object_batches={} object_ids={} length_batches={} length_ids={} serial_reservations={} read_grants={} read_handles={} cache_hits={} cache_misses={} upstream_batches={} authenticated_bytes={} evictions={} cached_objects_before={} cached_objects_after={} cache_state=as_left_by_bind_and_mutations_not_cold",
        store_after.object_batches - store_before.object_batches,
        store_after.object_ids - store_before.object_ids,
        store_after.length_batches - store_before.length_batches,
        store_after.length_ids - store_before.length_ids,
        store_after.serial_reservations - store_before.serial_reservations,
        reads_after.grants - reads_before.grants,
        reads_after.readers,
        cache_after.cache_hits - cache_before.cache_hits,
        cache_after.cache_misses - cache_before.cache_misses,
        cache_after.upstream_batches - cache_before.upstream_batches,
        cache_after.authenticated_bytes - cache_before.authenticated_bytes,
        cache_after.evictions - cache_before.evictions,
        cache_before.cached_objects,
        cache_after.cached_objects
    ));
    evidence(format_args!(
        "PRODUCT_COMMIT_COST_STORAGE case={case} {storage:?}"
    ));
    evidence(format_args!(
        "PRODUCT_COMMIT_COST_SAVED case={case} {saved:?}"
    ));
    evidence(format_args!(
        "PRODUCT_COMMIT_COST_PRODUCER case={case} {work:?}"
    ));
    evidence(format_args!(
        "PRODUCT_COMMIT_COST_CONTENT case={case} {counters:?}"
    ));

    // Exact arithmetic of the producer's own passes (construction/namespace/
    // normalize.rs): every scanned inode row is a typed value or a skipped
    // tombstone; every value has built or patched metadata; a fresh serial is
    // declared exactly for a built one; a sealed sequence ends on its first
    // page shorter than the window.
    assert_eq!(
        work.inode_rows,
        work.values_written + work.tombstones_skipped
    );
    assert_eq!(
        work.values_written,
        work.metadata_built + work.metadata_patched
    );
    assert_eq!(work.fresh_serials, work.metadata_built);
    assert_eq!(work.inode_pages, work.inode_rows / PAGE_ROWS + 1);
    assert_eq!(work.entry_pages, work.entry_rows / PAGE_ROWS + 1);
    assert!(work.inode_pages <= work.inode_rows.div_ceil(PAGE_ROWS) + 1);
    assert!(work.largest_page <= PAGE_ROWS);
    assert!(counters.validation.peak_window_rows <= PAGE_ROWS);
    // The same counts from the independent model of the change.
    assert_eq!(work.values_written, counted.changed, "{counted:?}");
    assert_eq!(work.files_constructed, counted.files, "{counted:?}");
    assert_eq!(
        work.files_unchanged, counted.files_same_bytes,
        "{counted:?}"
    );
    assert_eq!(work.fresh_serials, counted.fresh, "{counted:?}");
    assert_eq!(work.symlinks, counted.fresh_symlinks, "{counted:?}");
    assert_eq!(work.tombstones_skipped, counted.removed, "{counted:?}");
    // One Save, begun once on this Commit's own Storage producer.
    assert_eq!(storage.initial_reservations, 1);
    if expect_no_fullscan {
        assert_eq!(sql_total.fullscan_steps, 0);
    }
    let cost = Cost {
        base_paths: base.len(),
        work,
        jobs: total_jobs,
        record_class_jobs: by_class[ServiceClass::OperationRecord as usize],
        statements: sql_total.executions,
    };
    rig.close(&first);
    drop(first);
    rig.finish();
    cost
}

/// The fixed twelve-step change of `captured_commit.rs`'s cost test, so the
/// product route's counts can be read beside the test closure's.
fn small_change(session: &mut Session<'_>) {
    session.all(&[
        Do::Write("a/one", 3, b"CHANGED"),
        Do::Create("a/new", 0o644),
        Do::Write("a/new", 0, b"new"),
        Do::Remove("gone/x"),
        Do::Rename("keep/inner", "keep/inner2"),
        Do::Rename("a/b", "keep/moved-b"),
        Do::Link("top.txt", "keep/top-alias"),
        Do::Symlink("keep/ln", b"inner2"),
        Do::Chmod("empty", 0o700),
        Do::Write("bulk/d-000/f-00", 0, b"CHANGED"),
        Do::Remove("bulk/d-001/f-01"),
        Do::Create("bulk/new", 0o644),
    ]);
}
/// In each of `LARGE_DIRECTORIES` directories: ten files rewritten, four files
/// created and written, two files removed, one file's mode changed, and one
/// new directory holding one new file. No rename and no reused name.
fn large_change(session: &mut Session<'_>) {
    for directory in 0..LARGE_DIRECTORIES {
        let at = format!("bulk/d-{directory:03}");
        for file in 0..10 {
            let bytes = format!("changed {directory} {file}\n");
            session.apply(Do::Write(&format!("{at}/f-{file:02}"), 0, bytes.as_bytes()));
        }
        for file in 0..4 {
            let path = format!("{at}/n-{file:02}");
            let bytes = format!("created {directory} {file}\n");
            session.apply(Do::Create(&path, 0o644));
            session.apply(Do::Write(&path, 0, bytes.as_bytes()));
        }
        for file in [40, 41] {
            session.apply(Do::Remove(&format!("{at}/f-{file:02}")));
        }
        session.apply(Do::Chmod(&format!("{at}/f-20"), 0o711));
        let sub = format!("{at}/sub");
        session.apply(Do::Mkdir(&sub, 0o755));
        let inner = format!("{sub}/in");
        session.apply(Do::Create(&inner, 0o600));
        session.apply(Do::Write(&inner, 0, b"inside a new directory"));
    }
}

#[test]
fn a_small_and_a_large_change_through_the_product_constructor_are_counted() {
    let small = measure(
        "small",
        small_change,
        &[("keep/inner2", "keep/inner"), ("keep/moved-b", "a/b")],
        true,
    );
    let large = measure("large", large_change, &[], false);
    assert_eq!(small.base_paths, large.base_paths);
    // The large change's declared shape, per touched directory: eighteen live
    // rows (ten rewritten, four created, one mode change, the new directory,
    // its file, and the touched directory itself) and two tombstones.
    let directories = LARGE_DIRECTORIES as u64;
    assert_eq!(large.work.values_written, 18 * directories);
    assert_eq!(large.work.tombstones_skipped, 2 * directories);
    assert_eq!(large.work.files_constructed, 16 * directories);
    assert_eq!(large.work.files_unchanged, directories);
    assert_eq!(large.work.fresh_serials, 6 * directories);
    assert!(large.work.inode_pages > 1 && small.work.inode_pages == 1);
    for (case, cost) in [("small", &small), ("large", &large)] {
        let rows = cost.work.inode_rows + cost.work.entry_rows;
        evidence(format_args!(
            "PRODUCT_COMMIT_COST_RATIO case={case} inode_rows={} entry_rows={} captured_rows={rows} producer_record_jobs={} operation_record_class_jobs={} owner_jobs={} statements={} producer_record_jobs_per_captured_row={} operation_record_class_jobs_per_captured_row={} owner_jobs_per_captured_row={} statements_per_captured_row={} producer_record_jobs_per_inode_row={} owner_jobs_per_inode_row={}",
            cost.work.inode_rows,
            cost.work.entry_rows,
            cost.work.record_jobs,
            cost.record_class_jobs,
            cost.jobs,
            cost.statements,
            cost.work.record_jobs / rows,
            cost.record_class_jobs / rows,
            cost.jobs / rows,
            cost.statements / rows,
            cost.work.record_jobs / cost.work.inode_rows,
            cost.jobs / cost.work.inode_rows
        ));
    }
}
