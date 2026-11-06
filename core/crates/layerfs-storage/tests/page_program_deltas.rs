//! Directory and extent pages stored against their predecessor (layerfs-project
//! #92).
//!
//! Real C1 writers build a directory and a chunked file, the real admission path
//! saves them, and then small changes follow, one Save each, as small Commits do.
//! The proofs:
//!
//! * directory pages take COPY/INSERT programs against the page they replace;
//!   their chains grow to the metadata depth cap and re-base onto the chain root
//!   (#89), and every version reads back name for name from a fresh reader;
//! * extent pages edited in place take programs against the base page at the same
//!   level and logical position (the edit's positional hint), and so do the pages
//!   of a complete reconstruction offered the earlier version as its base; every
//!   version reads back byte for byte;
//! * fewer metadata bytes than a Store that writes no metadata program (metadata
//!   depth 0, the form every Store before this change holds), which still reads
//!   identically once programs join it;
//! * the read cost: one page read reconstructs one chain of at most nine records.
#[path = "support/memory_metadata.rs"]
mod metadata;
#[allow(dead_code)]
#[path = "support/edits.rs"]
mod edits;

use std::collections::BTreeMap;
use std::sync::Arc;

use edits::{Edits, Parts};
use layerfs_content::filesystem::directory::{
    apply_bindings, build_directory, list_after, DirectoryReadWork,
};
use layerfs_content::filesystem::{DirectoryRoot, FilesystemObjects, PathName};
use layerfs_content::{
    apply_edits, construct_bytes, construct_bytes_with_predecessor, ConstructionPolicy,
    ContentResult, Edit, EditRequest, FinalizedConsumer, FinalizedObject, ObjectId,
    PredecessorBase,
};
use layerfs_storage::encoding::delta::branch::BranchCounters;
use layerfs_storage::{Storage, StoragePolicy, WriteOutcome};

#[derive(Default)]
struct Emitted(Vec<FinalizedObject>);
impl FinalizedConsumer for Emitted {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.0.push(object);
        Ok(())
    }
}

fn storage(policy: StoragePolicy) -> (Arc<metadata::MemoryMetadata>, Storage) {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    metadata.state.lock().unwrap().policy = policy;
    let storage = Storage::new(metadata.clone()).unwrap();
    (metadata, storage)
}

/// Saves what a writer emitted, advisory predecessors and references included.
fn save(storage: &Storage, emitted: Emitted) -> WriteOutcome {
    let save = storage.begin_save().unwrap();
    for object in emitted.0 {
        save.accept(object).unwrap();
    }
    save.finish().unwrap()
}

fn scope<T>(
    body: impl FnOnce(
        &layerfs_telemetry::timer::TimingScope<'_, layerfs_telemetry::timer::Active>,
    ) -> ContentResult<T>,
) -> T {
    layerfs_telemetry::timer::Timing::disabled("test.operation", body)
        .0
        .unwrap()
}

// ---------------------------------------------------------------- directories

const NAMES: u64 = 4_000;

fn name(serial: u64) -> PathName {
    PathName::new(&format!("source-file-{serial:06}.rs")).unwrap()
}

type Listing = BTreeMap<Vec<u8>, u64>;

/// Applies sorted `changes` to `base` and saves what the writer emitted.
fn directory_commit(
    storage: &Storage,
    base: Option<DirectoryRoot>,
    changes: Vec<(PathName, Option<u64>)>,
) -> (DirectoryRoot, WriteOutcome) {
    let reader = storage.reader().unwrap();
    let mut emitted = Emitted::default();
    let root = {
        let mut objects = FilesystemObjects::new(&reader, &mut emitted);
        match base {
            None => {
                build_directory(
                    &mut objects,
                    changes
                        .into_iter()
                        .map(|(name, serial)| Ok((name, serial.unwrap()))),
                )
                .unwrap()
                .0
            }
            Some(base) => {
                apply_bindings(
                    &mut objects,
                    Some(base),
                    changes.into_iter().map(Ok),
                    layerfs_content::filesystem::MAXIMUM_SCRATCH_BYTES,
                    &mut |_, _| Ok(()),
                )
                .unwrap()
                .0
            }
        }
    };
    drop(reader);
    (root, save(storage, emitted))
}

fn list_all(storage: &Storage, root: DirectoryRoot) -> Listing {
    let reader = storage.reader().unwrap();
    let mut listing = Listing::new();
    let mut after: Option<PathName> = None;
    loop {
        let mut work = DirectoryReadWork::default();
        let page = list_after(&reader, root, after.as_ref(), 1_000, 1 << 20, &mut work).unwrap();
        for (name, serial) in page.entries {
            listing.insert(name.as_bytes().to_vec(), serial);
        }
        match page.continuation {
            Some(next) => after = Some(next),
            None => return listing,
        }
    }
}

/// Twenty small Commits on a 4,000-name directory: each binds one new name
/// and unbinds one old one near the start, so the same leaf and the root branch
/// change every time. Returns every version and the metadata bytes per Commit.
fn directory_history(storage: &Storage) -> (Vec<(DirectoryRoot, Listing)>, Vec<WriteOutcome>) {
    let mut listing: Listing = (1..=NAMES)
        .map(|serial| (name(serial).as_bytes().to_vec(), serial))
        .collect();
    let (mut root, initial) = directory_commit(
        storage,
        None,
        (1..=NAMES).map(|serial| (name(serial), Some(serial))).collect(),
    );
    assert_eq!(initial.delta.directory.delta_branches, 0);
    let mut versions = vec![(root, listing.clone())];
    let mut outcomes = Vec::new();
    for round in 0..20_u64 {
        let added = NAMES + 1 + round;
        let removed = 10 + round;
        let mut changes = vec![(name(removed), None), (name(added), Some(added))];
        changes.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        listing.remove(name(removed).as_bytes());
        listing.insert(name(added).as_bytes().to_vec(), added);
        let (next, outcome) = directory_commit(storage, Some(root), changes);
        root = next;
        versions.push((root, listing.clone()));
        outcomes.push(outcome);
    }
    (versions, outcomes)
}

#[test]
fn directory_pages_take_programs_re_base_at_the_cap_and_read_back() {
    let (_, store) = storage(StoragePolicy::default());
    let (versions, outcomes) = directory_history(&store);
    let mut total = BranchCounters::default();
    for (round, outcome) in outcomes.iter().enumerate() {
        let pages = outcome.delta.directory;
        // A leaf near the start and the root branch change in every Commit.
        assert!(pages.branches >= 2, "round {round}: {pages:?}");
        total.accumulate(&pages);
    }
    assert!(total.delta_branches >= 30, "{total:?}");
    assert!(total.rebased >= 2, "{total:?}");
    assert_eq!(total.work_exceeded, 0, "{total:?}");
    assert_eq!(total.depths[9..].iter().sum::<u64>(), 0, "{total:?}");
    // Directory programs never move the inode-branch or payload counts.
    for outcome in &outcomes {
        assert_eq!(outcome.delta.trials, 0, "{:?}", outcome.delta);
    }
    for (root, listing) in &versions {
        assert_eq!(&list_all(&store, *root), listing);
    }

    // Read cost: a fresh reader's lookup reconstructs one chain per page, each
    // of at most nine records (depth cap 8).
    let (root, listing) = versions.last().unwrap();
    let reader = store.reader().unwrap();
    let probe = PathName::from_bytes(listing.keys().next().unwrap()).unwrap();
    let found = layerfs_content::filesystem::directory::lookup(&reader, *root, &probe).unwrap();
    assert_eq!(found, listing.values().next().copied());
    let pool = reader.pooled_read_counters();
    assert!(pool.page_requests >= 2, "{pool:?}");
    assert_eq!(pool.branch_requests, 0, "{pool:?}");
    assert!(pool.program_max_depth <= 8, "{pool:?}");
    assert!(pool.page_chain_edges <= 8 * pool.page_requests, "{pool:?}");
    println!(
        "DIAGNOSTIC directory lookup: page reads {} edges {} applies {} decoded canonical {} B \
         record {} B max depth {}",
        pool.page_requests,
        pool.page_chain_edges,
        pool.page_delta_applies,
        pool.program_chain_canonical,
        pool.program_chain_encoded,
        pool.program_max_depth
    );
}

#[test]
fn directory_programs_cut_metadata_and_an_old_store_still_reads() {
    let (metadata, old) = storage(StoragePolicy::default().with_metadata_depth(0));
    let (old_versions, old_outcomes) = directory_history(&old);
    for outcome in &old_outcomes {
        assert_eq!(outcome.delta.directory.delta_branches, 0);
    }
    let (_, new) = storage(StoragePolicy::default());
    let (_, new_outcomes) = directory_history(&new);
    let whole: u64 = old_outcomes.iter().map(|o| o.metadata_bytes).sum();
    let programs: u64 = new_outcomes.iter().map(|o| o.metadata_bytes).sum();
    println!(
        "DIAGNOSTIC directory metadata bytes over 20 Commits: whole pages {whole} B, \
         programs {programs} B"
    );
    assert!(programs < whole, "{programs} vs {whole}");

    // The whole-page Store reads identically under the program-writing policy
    // and keeps working when programs join it.
    drop(old);
    metadata.state.lock().unwrap().policy = StoragePolicy::default();
    let reopened = Storage::new(metadata).unwrap();
    for (root, listing) in &old_versions {
        assert_eq!(&list_all(&reopened, *root), listing);
    }
    let (root, mut listing) = old_versions.last().unwrap().clone();
    let added = 90_000;
    listing.insert(name(added).as_bytes().to_vec(), added);
    let (next, outcome) = directory_commit(&reopened, Some(root), vec![(name(added), Some(added))]);
    assert!(outcome.delta.directory.delta_branches >= 1, "{:?}", outcome.delta);
    assert_eq!(list_all(&reopened, next), listing);
}

// ---------------------------------------------------------------- extents

/// Deterministic incompressible bytes, distinct per seed.
fn noise(len: usize, seed: u64) -> Vec<u8> {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64 ^ seed.wrapping_mul(0xa076_1d64_78bd_642f);
    (0..len)
        .map(|_| {
            state ^= state << 7;
            state ^= state >> 9;
            state ^= state << 8;
            state as u8
        })
        .collect()
}

fn read_file(storage: &Storage, root: ObjectId) -> Vec<u8> {
    let reader = storage.reader().unwrap();
    let mut out = Vec::new();
    scope(|scope| layerfs_content::read_all(&reader, root, &mut out, scope.child("read")));
    out
}

fn construct(storage: &Storage, bytes: &[u8], base: Option<ObjectId>) -> (ObjectId, WriteOutcome) {
    let policy = ConstructionPolicy::frozen_default();
    let reader = storage.reader().unwrap();
    let mut emitted = Emitted::default();
    let file = scope(|scope| {
        construct_bytes_with_predecessor(
            policy,
            &policy.capacities(),
            bytes,
            base.map(|root| PredecessorBase::new(&reader, root)),
            &mut emitted,
            scope.child("content"),
        )
    });
    drop(reader);
    (file.root, save(storage, emitted))
}

/// Applies one edit whose replacement is `replacement` and saves the result.
fn edit(
    storage: &Storage,
    root: ObjectId,
    bytes: &mut Vec<u8>,
    change: Edit,
    replacement: Vec<u8>,
) -> (ObjectId, WriteOutcome) {
    let policy = ConstructionPolicy::frozen_default();
    let stream = Edits::new(bytes.len() as u64, vec![change]).unwrap();
    let mut parts = Parts::new();
    parts.push(replacement.clone());
    let reader = storage.reader().unwrap();
    let mut emitted = Emitted::default();
    let file = scope(|scope| {
        apply_edits(
            policy,
            &policy.capacities(),
            &reader,
            EditRequest {
                root,
                edits: &stream,
                source: &parts,
            },
            &mut emitted,
            scope.child("edit"),
        )
    });
    drop(reader);
    bytes.splice(
        change.start() as usize..change.end() as usize,
        replacement,
    );
    (file.root, save(storage, emitted))
}

/// The Q5 edit sequence, scaled to a 6 MiB file (~380 chunks, three leaves
/// and a root branch): insert 1 B at the start, 1 B in the middle, overwrite
/// 4 KiB, append 256 KiB, delete the first byte.
fn extent_history(storage: &Storage) -> (Vec<(ObjectId, Vec<u8>)>, Vec<WriteOutcome>) {
    let mut bytes = noise(6 << 20, 1);
    let (mut root, initial) = construct(storage, &bytes, None);
    assert_eq!(initial.delta.extent.delta_branches, 0);
    let mut versions = vec![(root, bytes.clone())];
    let mut outcomes = Vec::new();
    let middle = (bytes.len() / 2) as u64;
    let steps: Vec<(Edit, Vec<u8>)> = vec![
        (Edit::insert(0, 1), vec![0xA5]),
        (Edit::insert(middle, 1), vec![0x5A]),
        (Edit::overwrite(1 << 20, (1 << 20) + 4_096), noise(4_096, 2)),
    ];
    for (change, replacement) in steps {
        let (next, outcome) = edit(storage, root, &mut bytes, change, replacement);
        root = next;
        versions.push((root, bytes.clone()));
        outcomes.push(outcome);
    }
    let len = bytes.len() as u64;
    let (next, outcome) = edit(storage, root, &mut bytes, Edit::insert(len, 256 << 10), noise(256 << 10, 3));
    root = next;
    versions.push((root, bytes.clone()));
    outcomes.push(outcome);
    let (next, outcome) = edit(storage, root, &mut bytes, Edit::delete(0, 1), Vec::new());
    versions.push((next, bytes.clone()));
    outcomes.push(outcome);
    (versions, outcomes)
}

#[test]
fn edited_extent_pages_take_programs_against_the_positional_base_and_read_back() {
    let (_, store) = storage(StoragePolicy::default());
    let (versions, outcomes) = extent_history(&store);
    for (step, outcome) in outcomes.iter().enumerate() {
        let pages = outcome.delta.extent;
        assert!(pages.delta_branches >= 1, "step {step}: {pages:?}");
    }
    for (root, bytes) in &versions {
        assert!(read_file(&store, *root) == *bytes, "version {root:?}");
    }
    let (_, whole) = storage(StoragePolicy::default().with_metadata_depth(0));
    let (_, whole_outcomes) = extent_history(&whole);
    for (step, (program, plain)) in outcomes.iter().zip(&whole_outcomes).enumerate() {
        println!(
            "DIAGNOSTIC extent edit {step}: metadata whole pages {} B, programs {} B; pages {:?}",
            plain.metadata_bytes, program.metadata_bytes, program.delta.extent
        );
    }
    let programs: u64 = outcomes.iter().map(|o| o.metadata_bytes).sum();
    let plain: u64 = whole_outcomes.iter().map(|o| o.metadata_bytes).sum();
    assert!(programs * 2 < plain, "{programs} vs {plain}");
}

#[test]
fn a_reconstruction_offered_its_earlier_version_hints_every_page_by_position() {
    let (_, store) = storage(StoragePolicy::default());
    let mut bytes = noise(6 << 20, 4);
    let (root, _) = construct(&store, &bytes, None);
    // A rename-over or recreate of the same path: the whole file is written
    // again with one byte inserted near the start, and the earlier version is
    // offered as the base.
    bytes.insert(1_000, 0x11);
    let (next, outcome) = construct(&store, &bytes, Some(root));
    let pages = outcome.delta.extent;
    assert!(pages.delta_branches >= 2, "{pages:?}");
    assert!(read_file(&store, next) == bytes);
    // Without a base no page carries a hint, so none takes a program.
    let (_, plain) = storage(StoragePolicy::default());
    let (_, _) = construct(&plain, &noise(6 << 20, 4), None);
    let (_, outcome) = construct(&plain, &bytes, None);
    assert_eq!(outcome.delta.extent.delta_branches, 0);
    let _ = construct_bytes;
}
