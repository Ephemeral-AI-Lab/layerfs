//! The walk ceiling, and the batched route a tier above it takes.
//!
//! `MAXIMUM_WALK_ENTRIES` is charged **once per whole-tree walk**. A
//! `build_filesystem` states its own bindings and the single reachability walk
//! charges every one of them, so one build is refused above the ceiling; the
//! product's own `limits.rs` doc says a tree larger than that "is reached by
//! several operations that each stay under the ceiling". These tests pin the
//! exact accepted/refused boundary through the public API, and they hold the
//! batched route to the two properties the driver depends on:
//!
//! * every batch stays at or under the ceiling, and
//! * an update that states **new files only** is served by a purely
//!   non-retaining measured phase — it does not read back what it emits.
//!
//! The second property is the load-bearing one. `check_parent_aliases` walks the
//! base tree once for every child that already has a stored record, so a batch
//! that rebinds an existing name pays for the whole tree beside it; a batch of
//! new files pays for nothing. A driver that restated a binding would still
//! compile and would still be legal below the ceiling — and would refuse every
//! tier above it.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use fs_bench_storage_content::ops::fs_fixture::{Batch, PreparedTree, Recipe, ROOT_SERIAL};
use fs_bench_storage_content::workload::providers::{PairProvider, SharedStore, TreeStore};
use layerfs_content::filesystem::references::backing::{FileBacking, OrderingBacking};
use layerfs_content::filesystem::{
    build_filesystem, scope_for_seed, update_filesystem, FilesystemInput, FilesystemObjects,
    FilesystemRead, FilesystemResources, FilesystemRootId, LogicalPath,
};
use layerfs_content::DiscardingConsumer;

/// The product's own `MAXIMUM_WALK_ENTRIES`, restated so a change to it fails
/// here rather than silently moving the tiers' shape.
const CEILING: usize = 4_096;

fn scope() -> layerfs_content::InodeScope {
    scope_for_seed([7_u8; 32])
}

fn recipe(entries: u32, directories: u32) -> PreparedTree {
    Recipe {
        profile: "binary-v1",
        entries,
        directories,
        seed: 11,
    }
    .prepare()
}

/// A scratch directory no other test can collide with.
fn scratch(tag: &str) -> FileBacking {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let unique = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "fs-bench-walk-{tag}-{}-{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch directory");
    FileBacking::new(dir)
}

/// Runs one batch into `store` and returns the root it produced.
fn run_batch(
    batch: &Batch,
    base: Option<FilesystemRootId>,
    reader: &dyn layerfs_content::AuthenticatedObjects,
    store: &mut TreeStore,
    tag: &str,
) -> Result<FilesystemRootId, String> {
    let input = FilesystemInput {
        base,
        scope: scope(),
        root_serial: ROOT_SERIAL,
        directories: &batch.directories,
        inodes: &batch.inodes,
        new_inodes: &batch.new_inodes,
        resources: FilesystemResources::default(),
    };
    let mut backing = scratch(tag);
    let mut objects = FilesystemObjects::new(reader, store);
    let backing = Some(&mut backing as &mut dyn OrderingBacking);
    let result = match base {
        None => build_filesystem(&mut objects, &input, backing),
        Some(_) => update_filesystem(&mut objects, &input, backing),
    };
    result
        .map(|result| result.root)
        .map_err(|error| format!("{error:?}"))
}

/// Reads the root listing back through the public read path, so the route is
/// proved to produce a tree that can be read and not merely one that builds.
fn root_listing(store: &TreeStore, root: FilesystemRootId) -> Vec<(String, u64)> {
    let mut read = FilesystemRead::new(store, root).expect("read");
    let mut out = Vec::new();
    let mut after = None;
    loop {
        let page = read
            .list(&LogicalPath::root(), after.as_ref(), 64, 8_192)
            .expect("list root");
        out.extend(
            page.entries
                .iter()
                .map(|(name, serial)| (name.as_str().to_string(), *serial)),
        );
        match page.continuation {
            Some(next) => after = Some(next),
            None => break,
        }
    }
    out
}

#[test]
fn a_single_build_of_exactly_the_ceiling_is_accepted() {
    // 1 directory binding + 4,095 file bindings = 4,096 stated bindings.
    let prepared = recipe(4_095, 1);
    assert_eq!(prepared.bindings(), CEILING);
    let mut store = TreeStore::new();
    let batch = &prepared.batches(CEILING).expect("batches")[0];
    let empty = TreeStore::new();
    assert!(
        run_batch(batch, None, &empty, &mut store, "accept").is_ok(),
        "4,096 bindings must be accepted"
    );
}

#[test]
fn a_single_build_of_the_ceiling_plus_one_is_refused_by_the_walk() {
    let prepared = recipe(4_096, 1);
    assert_eq!(prepared.bindings(), CEILING + 1);
    let mut store = TreeStore::new();
    let empty = TreeStore::new();
    let error = run_batch(
        &prepared.batches(CEILING + 1).expect("batches")[0],
        None,
        &empty,
        &mut store,
        "refuse",
    )
    .expect_err("4,097 bindings must be refused");
    assert!(
        error.contains("cycle check work limit"),
        "the refusal must be the walk ceiling, got {error}"
    );
}

#[test]
fn every_batch_stays_under_the_ceiling_and_states_new_files_only() {
    let prepared = recipe(10_000, 100);
    assert_eq!(prepared.bindings(), 10_100);
    let batches = prepared.batches(CEILING).expect("batches");
    assert!(
        batches.len() > 1,
        "a tree above the ceiling needs more than one operation"
    );
    for (index, batch) in batches.iter().enumerate() {
        assert!(
            batch.bindings() <= CEILING,
            "batch {index} states {} bindings",
            batch.bindings()
        );
    }
    // The declared new serials are the batch's own inode serials, minus the root
    // on the first batch. A batch that declared a serial it does not state would
    // be refused by the product as a new identity it never allocates.
    assert!(batches[0].new_inodes.contains(&ROOT_SERIAL));
    for batch in &batches[1..] {
        assert!(!batch.new_inodes.contains(&ROOT_SERIAL));
    }
    // Every binding of the whole tree is stated exactly once across the batches.
    let mut stated: BTreeMap<(u64, String), u64> = BTreeMap::new();
    for batch in &batches {
        for update in &batch.directories {
            for (name, binding) in &update.changes {
                let Some(child) = binding else { continue };
                let key = (update.parent, name.as_str().to_string());
                assert!(
                    stated.insert(key, *child).is_none(),
                    "a binding is stated twice across the batches"
                );
            }
        }
    }
    assert_eq!(stated.len(), prepared.bindings());
}

#[test]
fn the_batched_route_is_served_by_a_non_retaining_measured_phase() {
    // The tier the registry declares, at the smallest size above the ceiling:
    // 10,000 files over 100 directories. Every operation after the first reads a
    // base that a *fixture* store already holds, and the consumer discards.
    let prepared = recipe(10_000, 100);
    let batches = prepared.batches(CEILING).expect("batches");

    // The fixture chain: the same operations into an authenticating store, so
    // every object the measured phase reads — including one an operation reads
    // back after emitting it — is present before the timer starts. The reader is
    // the in-flight overlay, which is what makes the chain itself reproducible.
    let mut fixture = TreeStore::new();
    let mut fixture_roots = Vec::new();
    let mut base = None;
    for (index, batch) in batches.iter().enumerate() {
        let input = FilesystemInput {
            base,
            scope: scope(),
            root_serial: ROOT_SERIAL,
            directories: &batch.directories,
            inodes: &batch.inodes,
            new_inodes: &batch.new_inodes,
            resources: FilesystemResources::default(),
        };
        let mut shared = SharedStore::new();
        let root = {
            let reader = shared.reader(&fixture);
            let mut backing = scratch(&format!("fixture{index}"));
            let mut objects = FilesystemObjects::new(&reader, &mut shared);
            let result = match base {
                None => build_filesystem(&mut objects, &input, Some(&mut backing as &mut dyn OrderingBacking)),
                Some(_) => update_filesystem(&mut objects, &input, Some(&mut backing as &mut dyn OrderingBacking)),
            }
            .unwrap_or_else(|error| panic!("fixture batch {index} failed: {error:?}"));
            result.root
        };
        shared.absorb_into(&mut fixture);
        fixture_roots.push(root);
        base = Some(root);
    }

    // The measured shape: a discarding consumer, each base read from the fixture.
    let mut measured_roots = Vec::new();
    let mut base = None;
    for (index, batch) in batches.iter().enumerate() {
        let reader = PairProvider::new(&fixture, &fixture);
        let mut consumer = DiscardingConsumer::new();
        let input = FilesystemInput {
            base,
            scope: scope(),
            root_serial: ROOT_SERIAL,
            directories: &batch.directories,
            inodes: &batch.inodes,
            new_inodes: &batch.new_inodes,
            resources: FilesystemResources::default(),
        };
        let mut backing = scratch(&format!("measured{index}"));
        let mut objects = FilesystemObjects::new(&reader, &mut consumer);
        let result = match base {
            None => build_filesystem(&mut objects, &input, Some(&mut backing as &mut dyn OrderingBacking)),
            Some(_) => update_filesystem(&mut objects, &input, Some(&mut backing as &mut dyn OrderingBacking)),
        }
        .unwrap_or_else(|error| panic!("measured batch {index} failed: {error:?}"));
        measured_roots.push(result.root);
        base = Some(result.root);
    }
    assert_eq!(
        measured_roots, fixture_roots,
        "the measured chain must reproduce the fixture chain at every step"
    );
    // The route produces a tree that reads back, not merely one that builds.
    let listing = root_listing(&fixture, fixture_roots[fixture_roots.len() - 1]);
    assert_eq!(listing.len(), 100, "the root binds every derived directory");
    assert!(
        listing.windows(2).all(|pair| pair[0].0 < pair[1].0),
        "the root listing is sorted by name"
    );
}
