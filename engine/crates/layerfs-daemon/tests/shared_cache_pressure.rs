//! Correctness of the daemon's shared object cache under eviction pressure.
//!
//! Two Workspaces of one daemon, on two branches forked from one Layer, read
//! through a cache whose allowance is a small fraction of what they read.
//! Every read returns exactly the bytes of that Workspace's own published
//! root, which were recorded independently straight from the Store port. An
//! evicted object is demanded from the Store again and authenticated again.
//! The cache's own accounting stays inside its allowance and balances
//! exactly: entries held are entries inserted less entries evicted. An
//! object larger than the allowance is served and never retained. While one
//! Workspace scans its whole tree in a loop, the other keeps completing
//! reads.
//!
//! Counts only; nothing is timed. Real Disposable Store, real owner, no
//! mount. Public API only; every wait is bounded.
#[allow(dead_code)]
#[path = "support/captured_drive.rs"]
mod drive;
#[allow(dead_code)]
#[path = "support/namespace_model.rs"]
mod model;
#[allow(dead_code)]
#[path = "support/shared_cache_rig.rs"]
mod rig;
#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod support;
use drive::evidence;
use layerfs_content::{AuthenticatedObjects, ObjectId};
use layerfs_daemon::store::BoundWorkspace;
use rig::{apply, commit, pattern, read, Daemon, Do, Tree};
use std::{
    collections::BTreeSet,
    fs,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

/// The shared allowance: far below the two trees, above any object of
/// `src/`. The one file of `big/` is a single object larger than it.
const ALLOWANCE: usize = 64 * 1024;
const BIG: usize = 100_000;
const DIRECTORIES: usize = 16;
const FILES: usize = 20;
/// The cache's logical charge of one object, of one remembered length and
/// of one remembered mode and mtime.
const OBJECT_CHARGE: usize = 256;
const LENGTH_CHARGE: usize = 8 + 256;
const PORTABLE_CHARGE: usize = 16 + 256;

fn tree(source: &Path) {
    for directory in 0..DIRECTORIES {
        let path = source.join(format!("src/d-{directory:02}"));
        fs::create_dir_all(&path).unwrap();
        for file in 0..FILES {
            let seed = (directory * FILES + file) as u32;
            fs::write(
                path.join(format!("f-{file:02}")),
                pattern(seed, 2_048 + 211 * file),
            )
            .unwrap();
        }
    }
    fs::create_dir(source.join("big")).unwrap();
    fs::write(source.join("big/one"), pattern(40_000, BIG)).unwrap();
}

/// Regular files below `src/`, with their bytes, in path order.
fn files(flat: &model::Flat) -> Vec<(String, &[u8])> {
    flat.iter()
        .filter(|(path, seen)| seen.kind == model::FILE && path.starts_with(b"src/"))
        .map(|(path, seen)| {
            (
                String::from_utf8(path.clone()).unwrap(),
                seen.payload.as_slice(),
            )
        })
        .collect()
}

/// What must hold of the cache's accounting whenever it is observed.
/// `bypassed` counts demands of the one object too large to retain; with a
/// single reader at a time `exact` holds: every other miss, every length the
/// Store answered and every decoded mode and mtime became an entry, and each
/// eviction removed one.
fn accounted(daemon: &Daemon, bypassed: u64, exact: bool) {
    let cache = daemon.cache();
    let lengths = daemon.store.work().length_ids;
    assert!(cache.charged_cache_bytes <= ALLOWANCE, "{cache:?}");
    assert!(
        cache.cached_objects * OBJECT_CHARGE
            + cache.file_lengths * LENGTH_CHARGE
            + cache.portable_metadata * PORTABLE_CHARGE
            <= cache.charged_cache_bytes,
        "{cache:?}"
    );
    let held = (cache.cached_objects + cache.file_lengths + cache.portable_metadata) as u64;
    let inserted = cache.cache_misses + lengths + cache.portable_metadata_reads - bypassed;
    if exact {
        assert_eq!(held, inserted - cache.evictions, "{cache:?}");
    } else {
        // Two readers that miss one object at once insert it once.
        assert!(held + cache.evictions <= inserted, "{cache:?}");
    }
}

/// Sets the flag when the test leaves, however it leaves, so that no
/// spawned thread's exit depends on the test finishing without a panic.
struct Stop(Arc<AtomicBool>);
impl Drop for Stop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

#[test]
fn eviction_changes_no_byte_and_keeps_exact_accounts() {
    let tree = Tree::new("shared-cache-pressure", 2, tree);
    let (branch_a, branch_b) = (tree.branches[0], tree.branches[1]);
    let only_b = pattern(80_002, 3_333);

    // Setup by a daemon stopped before anything is counted.
    let setup = tree.daemon(2, 8 * 1024 * 1024);
    let mut clock = 1_800_000_000;
    let a = setup.bind(branch_a, 1);
    for step in [
        Do::Truncate("src/d-02/f-03", 0),
        Do::Write("src/d-02/f-03", 0, &pattern(80_001, 2_500)),
        Do::Remove("src/d-07/f-11"),
    ] {
        assert!(apply(&a, &mut clock, step), "{step:?}");
    }
    commit(&a);
    let b = setup.bind(branch_b, 2);
    for step in [
        Do::Truncate("src/d-02/f-03", 0),
        Do::Write("src/d-02/f-03", 0, &pattern(80_003, 2_700)),
        Do::Create("src/d-09/only-b", 0o644),
        Do::Write("src/d-09/only-b", 0, &only_b),
    ] {
        assert!(apply(&b, &mut clock, step), "{step:?}");
    }
    commit(&b);
    setup.close(&a);
    setup.close(&b);
    drop((a, b));
    setup.stop();

    let daemon = tree.daemon(2, ALLOWANCE);
    let (expected_a, objects_a) = daemon.published(&daemon.snapshot(branch_a));
    let (expected_b, objects_b) = daemon.published(&daemon.snapshot(branch_b));
    let distinct: BTreeSet<ObjectId> = objects_a.keys().chain(objects_b.keys()).copied().collect();
    let working_set: usize = distinct
        .iter()
        .map(|id| objects_a.get(id).or_else(|| objects_b.get(id)).unwrap() + OBJECT_CHARGE)
        .sum();
    assert!(working_set > 16 * ALLOWANCE);
    let (files_a, files_b) = (files(&expected_a), files(&expected_b));
    assert_eq!(files_a.len(), DIRECTORIES * FILES - 1);
    assert_eq!(files_b.len(), DIRECTORIES * FILES + 1);
    let a = daemon.bind(branch_a, 11);
    let b = daemon.bind(branch_b, 12);
    accounted(&daemon, 0, true);

    // One reader at a time, the two Workspaces alternating and walking the
    // tree in opposite directions. Every read is checked; so is every state.
    for (at, (path, bytes)) in files_a.iter().enumerate() {
        assert_eq!(read(&a, path), *bytes, "Workspace A {path}");
        let (path, bytes) = &files_b[files_b.len() - 1 - at.min(files_b.len() - 1)];
        assert_eq!(read(&b, path), *bytes, "Workspace B {path}");
        accounted(&daemon, 0, true);
    }
    let interleaved = daemon.cache();
    assert!(interleaved.evictions > 0);
    // More objects were fetched than exist: evicted ones were fetched again.
    assert!(interleaved.cache_misses > distinct.len() as u64);
    evidence(format_args!(
        "interleaved: allowance={ALLOWANCE} working_set_charge={working_set} distinct_objects={} hits={} misses={} evictions={} charged={} cached_objects={} file_lengths={} portable_metadata={}",
        distinct.len(),
        interleaved.cache_hits,
        interleaved.cache_misses,
        interleaved.evictions,
        interleaved.charged_cache_bytes,
        interleaved.cached_objects,
        interleaved.file_lengths,
        interleaved.portable_metadata,
    ));

    // One object, exactly. The content of a file only branch B has is
    // demanded, found again, evicted by Workspace A reading its own tree,
    // and demanded again: the Store is asked again and the bytes are
    // authenticated again.
    let id = ObjectId::from_bytes(&drive::content_root(&b, "src/d-09/only-b")).unwrap();
    assert!(objects_b.contains_key(&id) && !objects_a.contains_key(&id));
    let length = objects_b[&id] as u64;
    let pressure = |what: &str| {
        for (path, bytes) in &files_a {
            assert_eq!(read(&a, path), *bytes, "{what}: Workspace A {path}");
        }
        accounted(&daemon, 0, true);
    };
    let demand = |expect_miss: bool, what: &str| {
        let (cache, stored) = (daemon.cache(), daemon.store.work());
        let operation = b.operation().unwrap();
        let bytes = operation.client().read_canonical(id).unwrap();
        drop(operation);
        assert_eq!(ObjectId::for_bytes(&bytes), id, "{what}");
        assert_eq!(bytes.len() as u64, length, "{what}");
        let (after, stored_after) = (daemon.cache(), daemon.store.work());
        let miss = u64::from(expect_miss);
        assert_eq!(
            (
                after.cache_misses - cache.cache_misses,
                after.cache_hits - cache.cache_hits,
                stored_after.object_ids - stored.object_ids,
                after.authenticated_bytes - cache.authenticated_bytes,
            ),
            (miss, 1 - miss, miss, miss * length),
            "{what}"
        );
        accounted(&daemon, 0, true);
    };
    pressure("before the first demand");
    demand(true, "first demand");
    demand(false, "resident");
    pressure("between the demands");
    demand(true, "after eviction");
    demand(false, "resident again");

    // An object larger than the whole allowance is served every time and
    // retained never: nothing is evicted for it and nothing is charged.
    let big = ObjectId::from_bytes(&drive::content_root(&a, "big/one")).unwrap();
    assert!(objects_a[&big] + OBJECT_CHARGE > ALLOWANCE);
    for bypassed in 1..=2_u64 {
        let before = daemon.cache();
        let operation = a.operation().unwrap();
        let bytes = operation.client().read_canonical(big).unwrap();
        drop(operation);
        assert_eq!(ObjectId::for_bytes(&bytes), big);
        let after = daemon.cache();
        assert_eq!(
            (
                after.cache_misses - before.cache_misses,
                after.cache_hits - before.cache_hits,
                after.evictions - before.evictions,
            ),
            (1, 0, 0)
        );
        assert_eq!(
            (after.cached_objects, after.charged_cache_bytes),
            (before.cached_objects, before.charged_cache_bytes)
        );
        accounted(&daemon, bypassed, true);
    }
    // Two readers at once. Workspace A scans its whole tree in a loop while
    // Workspace B reads file after file; both check every byte.
    let stop = Arc::new(AtomicBool::new(false));
    let _stop = Stop(stop.clone());
    let progress = Arc::new([AtomicU64::new(0), AtomicU64::new(0)]);
    let (send, events) = mpsc::channel::<(usize, u64)>();
    let spawn = |who: usize, workspace: Arc<BoundWorkspace>, expected: &model::Flat| {
        let owned: Vec<(String, Vec<u8>)> = files(expected)
            .into_iter()
            .map(|(path, bytes)| (path, bytes.to_vec()))
            .collect();
        let (stop, progress, send) = (stop.clone(), progress.clone(), send.clone());
        std::thread::spawn(move || {
            let mut passes = 0_u64;
            while !stop.load(Ordering::Acquire) && passes < 1_000 {
                for (path, bytes) in &owned {
                    assert_eq!(read(&workspace, path), *bytes, "reader {who} {path}");
                    progress[who].fetch_add(1, Ordering::Release);
                    if stop.load(Ordering::Acquire) {
                        return;
                    }
                }
                passes += 1;
                if send.send((who, passes)).is_err() {
                    return;
                }
            }
        })
    };
    let (before, lengths_before) = (daemon.cache(), daemon.store.work().length_ids);
    let scanner = spawn(0, a.clone(), &expected_a);
    let reader = spawn(1, b.clone(), &expected_b);
    drop(send);
    // Each completed pass of one reader records how far the other has come.
    let deadline = Instant::now() + Duration::from_secs(40);
    let mut seen = [Vec::<u64>::new(), Vec::new()];
    while seen[0].len() < 3 || seen[1].len() < 3 {
        let left = deadline.saturating_duration_since(Instant::now());
        let (who, _) = events
            .recv_timeout(left)
            .expect("both readers keep completing passes");
        seen[who].push(progress[1 - who].load(Ordering::Acquire));
    }
    stop.store(true, Ordering::Release);
    let joined = Instant::now() + Duration::from_secs(20);
    for thread in [scanner, reader] {
        while !thread.is_finished() {
            assert!(Instant::now() < joined, "a reader did not stop");
            std::thread::yield_now();
        }
        thread.join().unwrap();
    }
    // Between two completed passes of either reader the other one advanced.
    for who in 0..2 {
        assert!(seen[who][2] > seen[who][0], "reader {who}: {seen:?}");
    }
    let after = daemon.cache();
    assert!(after.evictions > before.evictions);
    accounted(&daemon, 2, false);
    let readers = daemon.store.read_work();
    assert_eq!(readers.quarantined, 0);
    // Both readers walk the same order, so both often miss one object at
    // once: each fetches and authenticates it, and it is inserted once.
    let held = |work: &layerfs_workspace::ClientWork| {
        (work.cached_objects + work.file_lengths + work.portable_metadata) as i64
    };
    let inserted = (after.evictions - before.evictions) as i64 + held(&after) - held(&before);
    let fetched = after.cache_misses - before.cache_misses + daemon.store.work().length_ids
        - lengths_before
        + after.portable_metadata_reads
        - before.portable_metadata_reads;
    evidence(format_args!(
        "concurrent: passes_a={} passes_b={} other_reader_progress_at_each_pass={seen:?} hits={} misses={} evictions={} fetched_while_another_reader_fetched_the_same={} reader_demands={} retained_readers={}",
        seen[0].len(),
        seen[1].len(),
        after.cache_hits - before.cache_hits,
        after.cache_misses - before.cache_misses,
        after.evictions - before.evictions,
        fetched as i64 - inserted,
        readers.requests,
        readers.readers,
    ));

    // The ordinary view serves the oversized file too, through the same cache.
    assert_eq!(read(&a, "big/one"), pattern(40_000, BIG));
    assert!(daemon.cache().charged_cache_bytes <= ALLOWANCE);

    daemon.close(&a);
    daemon.close(&b);
    drop((a, b));
    daemon.stop();
    tree.finish();
}
