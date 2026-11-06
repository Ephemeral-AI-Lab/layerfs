//! Inode-table branches stored against their predecessor (#360 item 3).
//!
//! A ~50,000-inode table (two branch levels) is built through the real C1
//! inode-table writer and saved through the real admission path; then one value
//! changes per Save, as a small Commit does. Each Save rewrites the root branch
//! and one child branch. The proofs:
//!
//! * chains grow 1..8 and, at the metadata depth cap, restart at depth one
//!   against the whole branch at their root instead of storing the branch whole
//!   (#89), and every table, old and new, reads back value for value;
//! * a Store first written with whole branches only (the form every Store
//!   before this change holds) keeps reading identically once delta branches
//!   join it;
//! * the lookup cost bound: one lookup reconstructs exactly one branch chain per
//!   branch level (counter, not time), and a batched lookup reconstructs each
//!   branch page it visits once.
#[path = "support/memory_metadata.rs"]
mod metadata;

use layerfs_content::filesystem::attributes::portable::PortableMetadata;
use layerfs_content::filesystem::inode::{
    apply_inode_values, lookup, lookup_many, InodeReadWork, InodeTable,
};
use layerfs_content::filesystem::{FilesystemObjects, MAXIMUM_SCRATCH_BYTES};
use layerfs_content::object::inode_leaf::{InodeKind, InodeMetadata, InodeValue};
use layerfs_content::{
    AuthenticatedObjects, ContentResult, FinalizedConsumer, FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_storage::{Storage, StoragePolicy, WriteOutcome};
use std::sync::Arc;

const INODES: u64 = 50_000;

#[derive(Default)]
struct Emitted(Vec<FinalizedObject>);
impl FinalizedConsumer for Emitted {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.0.push(object);
        Ok(())
    }
}

fn value(serial: u64, version: u64) -> InodeValue {
    InodeValue {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 1,
        content_root: ObjectId::for_bytes(&serial.to_le_bytes()),
        metadata_root: InodeMetadata::Inline(PortableMetadata {
            mode: 0o644,
            mtime_seconds: 1_700_000_000 + version as i64,
            mtime_nanoseconds: 0,
        }),
    }
}

/// Applies `changes` to `base` and saves what the writer emitted. Content roots
/// are synthetic, so the saved objects carry no logical references; their
/// advisory predecessors are kept exactly as the writer declared them.
fn commit(
    storage: &Storage,
    base: Option<ObjectId>,
    changes: Vec<(u64, InodeValue)>,
) -> (ObjectId, WriteOutcome) {
    let reader = storage.reader().unwrap();
    let mut emitted = Emitted::default();
    let (root, _) = {
        let mut objects = FilesystemObjects::new(&reader, &mut emitted);
        apply_inode_values(
            &mut objects,
            base,
            changes
                .into_iter()
                .map(|(serial, value)| Ok((serial, Some(value)))),
            MAXIMUM_SCRATCH_BYTES,
        )
        .unwrap()
    };
    drop(reader);
    let save = storage.begin_save().unwrap();
    for object in emitted.0 {
        let predecessors = object.predecessors().clone();
        let parts = object.into_parts();
        save.accept(
            FinalizedObject::new(parts.role, parts.canonical)
                .unwrap()
                .with_predecessors(predecessors),
        )
        .unwrap();
    }
    (root, save.finish().unwrap())
}

/// Every value of the table at `root`, read through a fresh reader.
fn read_all(storage: &Storage, root: ObjectId) -> Vec<Option<InodeValue>> {
    let reader = storage.reader().unwrap();
    let serials: Vec<u64> = (1..=INODES).collect();
    let mut work = InodeReadWork::default();
    lookup_many(&reader, table(root), &serials, &mut work).unwrap()
}

fn table(root: ObjectId) -> InodeTable {
    InodeTable {
        root,
        root_serial: 1,
    }
}

fn expected(versions: &[u64]) -> Vec<Option<InodeValue>> {
    (1..=INODES)
        .map(|serial| Some(value(serial, versions[serial as usize])))
        .collect()
}

/// Branch levels of a table: pages read by one lookup, less its leaf.
fn branch_levels(reader: &dyn AuthenticatedObjects, root: ObjectId) -> u64 {
    let mut work = InodeReadWork::default();
    lookup(reader, table(root), 1, &mut work).unwrap();
    work.pages_read - 1
}

fn storage(policy: StoragePolicy) -> (Arc<metadata::MemoryMetadata>, Storage) {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    metadata.state.lock().unwrap().policy = policy;
    let storage = Storage::new(metadata.clone()).unwrap();
    (metadata, storage)
}

fn small_commits(
    storage: &Storage,
    root: &mut ObjectId,
    versions: &mut [u64],
    count: u64,
    first: u64,
) -> Vec<WriteOutcome> {
    (first..first + count)
        .map(|round| {
            // Spread the edits across the table, as unrelated small Commits do.
            let serial = 1 + (round * 7_919) % INODES;
            versions[serial as usize] += 1;
            let (next, outcome) = commit(
                storage,
                Some(*root),
                vec![(serial, value(serial, versions[serial as usize]))],
            );
            *root = next;
            outcome
        })
        .collect()
}

fn initial(storage: &Storage) -> (ObjectId, Vec<u64>) {
    let versions = vec![0_u64; INODES as usize + 1];
    let rows = (1..=INODES)
        .map(|serial| (serial, value(serial, 0)))
        .collect();
    let (root, outcome) = commit(storage, None, rows);
    assert_eq!(
        outcome.delta.branch.delta_branches, 0,
        "{:?}",
        outcome.delta.branch
    );
    (root, versions)
}

#[test]
fn branch_chains_grow_to_the_cap_re_base_onto_their_root_and_read_back() {
    let (_, storage) = storage(StoragePolicy::default());
    let (mut root, mut versions) = initial(&storage);
    let reader = storage.reader().unwrap();
    assert_eq!(branch_levels(&reader, root), 2, "a two-level table");
    drop(reader);
    let mut roots = vec![(root, versions.clone())];
    let mut total = layerfs_storage::encoding::delta::branch::BranchCounters::default();
    for round in 0..20 {
        let outcome = small_commits(&storage, &mut root, &mut versions, 1, round).remove(0);
        let branch = outcome.delta.branch;
        // The root and one child branch change; both are offered.
        assert_eq!(branch.branches, 2, "round {round}: {branch:?}");
        // Branch programs never touch the payload delta counts: the whole-file
        // and chunk selection a Commit's content takes is what it was.
        let payload = outcome.delta;
        assert_eq!(
            (
                payload.prepared_full,
                payload.trials,
                payload.prefix_selected
            ),
            (0, 0, 0),
            "{payload:?}"
        );
        total.accumulate(&branch);
        roots.push((root, versions.clone()));
    }
    // The root branch's chain: depth 1..8, then at the cap a program against the
    // whole branch at the chain's root (depth 1 again), then 2..8 again; no
    // branch is stored whole after the initial table (#89).
    assert!(total.depth_capped >= 2, "{total:?}");
    assert_eq!(total.rebased, total.depth_capped, "{total:?}");
    assert_eq!(total.work_exceeded, 0, "{total:?}");
    assert_eq!(total.full_branches, 0, "{total:?}");
    assert_eq!(total.depths[0], 0, "{total:?}");
    assert_eq!(
        total.trials, total.branches,
        "one trial per branch: {total:?}"
    );
    assert!(total.delta_branches >= 30, "{total:?}");
    for depth in 1..=8 {
        assert!(total.depths[depth] >= 2, "depth {depth}: {total:?}");
    }
    assert_eq!(total.depths[9..].iter().sum::<u64>(), 0, "{total:?}");
    // Every table, old and new, reads back value for value, from a fresh
    // reader each, so chains at every depth are rebuilt from the Store.
    for (root, versions) in &roots {
        assert_eq!(read_all(&storage, *root), expected(versions));
    }
}

#[test]
fn a_store_of_whole_branches_reads_identically_after_delta_branches_join_it() {
    // Metadata depth 0 writes no metadata delta at all: every branch whole, the
    // form a Store written before this change holds.
    let (metadata, old) = storage(StoragePolicy::default().with_metadata_depth(0));
    let (mut root, mut versions) = initial(&old);
    let mut roots = vec![(root, versions.clone())];
    for outcome in small_commits(&old, &mut root, &mut versions, 5, 0) {
        assert_eq!(outcome.delta.branch.delta_branches, 0);
        assert_eq!(outcome.delta.branch.full_branches, 2);
    }
    roots.push((root, versions.clone()));
    drop(old);
    metadata.state.lock().unwrap().policy = StoragePolicy::default();
    let new = Storage::new(metadata).unwrap();
    for (root, versions) in &roots {
        assert_eq!(read_all(&new, *root), expected(versions));
    }
    let outcomes = small_commits(&new, &mut root, &mut versions, 5, 5);
    assert!(
        outcomes[0].delta.branch.delta_branches >= 1,
        "{:?}",
        outcomes[0].delta
    );
    roots.push((root, versions.clone()));
    for (root, versions) in &roots {
        assert_eq!(read_all(&new, *root), expected(versions));
    }
}

#[test]
fn a_lookup_reconstructs_one_branch_chain_per_level() {
    let (_, storage) = storage(StoragePolicy::default());
    let (mut root, mut versions) = initial(&storage);
    // Seven small Commits: the root branch is at depth 7, below the cap.
    small_commits(&storage, &mut root, &mut versions, 7, 0);
    for serial in [1, INODES / 2, INODES] {
        let reader = storage.reader().unwrap();
        let mut work = InodeReadWork::default();
        let found = lookup(&reader, table(root), serial, &mut work).unwrap();
        assert_eq!(found, Some(value(serial, versions[serial as usize])));
        let pool = reader.pooled_read_counters();
        assert_eq!(work.pages_read, 3, "root branch, child branch, leaf");
        assert_eq!(
            pool.branch_requests, 2,
            "one chain per branch level: {pool:?}"
        );
        assert!(pool.branch_chain_edges <= 2 * 8, "{pool:?}");
        assert!(pool.branch_chain_edges >= 1, "{pool:?}");
        assert_eq!(pool.leaf_requests, 1, "{pool:?}");
    }
    // A batched lookup over the whole table reconstructs each branch page it
    // visits once: the chains counted equal the branch pages read.
    let reader = storage.reader().unwrap();
    let serials: Vec<u64> = (1..=INODES).step_by(37).collect();
    let mut work = InodeReadWork::default();
    lookup_many(&reader, table(root), &serials, &mut work).unwrap();
    let pool = reader.pooled_read_counters();
    let leaves = pool.leaf_requests;
    assert_eq!(
        pool.branch_requests + leaves,
        work.pages_read,
        "{pool:?} {work:?}"
    );
    let branch_pages = work.pages_read - leaves;
    assert!(
        branch_pages <= 1 + 16,
        "root and its child branches: {work:?} {pool:?}"
    );
    println!(
        "lookup_many {} serials: pages {} branch chains {} edges {} applies {} leaves {}",
        serials.len(),
        work.pages_read,
        pool.branch_requests,
        pool.branch_chain_edges,
        pool.branch_delta_applies,
        leaves
    );
}

#[test]
fn a_refused_base_stores_the_branch_whole_and_it_reads_back() {
    let (_, storage) = storage(StoragePolicy::default());
    let (root, mut versions) = initial(&storage);
    // A predecessor that is not stored is refused; the page is stored whole.
    let reader = storage.reader().unwrap();
    let page = reader.read_objects(&[root]).unwrap().remove(0);
    drop(reader);
    let mut predecessors = layerfs_content::AdvisoryPredecessors::new();
    let _ = predecessors.push(
        ObjectId::for_bytes(b"absent branch"),
        layerfs_content::PredecessorProvenance::UnchangedPrefix,
    );
    let save = storage.begin_save().unwrap();
    // The same page again is an exact reuse, not a new record; offer a changed
    // table instead, with its root's predecessor replaced by an absent one.
    let serial = 3;
    versions[serial as usize] += 1;
    let reader = storage.reader().unwrap();
    let mut emitted = Emitted::default();
    let (next, _) = {
        let mut objects = FilesystemObjects::new(&reader, &mut emitted);
        apply_inode_values(
            &mut objects,
            Some(root),
            std::iter::once(Ok((serial, Some(value(serial, versions[serial as usize]))))),
            MAXIMUM_SCRATCH_BYTES,
        )
        .unwrap()
    };
    drop(reader);
    for object in emitted.0 {
        let declared = object.predecessors().clone();
        let parts = object.into_parts();
        let object = FinalizedObject::new(parts.role, parts.canonical).unwrap();
        save.accept(if parts.role == ObjectRole::InodeBranch {
            object.with_predecessors(predecessors.clone())
        } else {
            object.with_predecessors(declared)
        })
        .unwrap();
    }
    let outcome = save.finish().unwrap();
    assert_eq!(
        outcome.delta.branch.branches, 2,
        "{:?}",
        outcome.delta.branch
    );
    assert_eq!(outcome.delta.branch.full_branches, 2);
    assert_eq!(outcome.delta.branch.delta_branches, 0);
    assert_ne!(page, Vec::<u8>::new());
    assert_eq!(read_all(&storage, next), expected(&versions));
}
