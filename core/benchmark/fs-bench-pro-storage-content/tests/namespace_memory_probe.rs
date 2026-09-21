//! Diagnostic: **whose** memory is the 100,000-entry row's 1.05 GB peak RSS?
//!
//! `pipeline-namespace-100000` publishes `resources.rss.process_peak_bytes` =
//! 1,048,805,376, and that figure is `getrusage(RUSAGE_SELF).ru_maxrss` - a
//! **lifetime** high-water sampled at the end of the measured region, not a phase
//! peak (`memory_cpu_space_support.md` section 3.2: *"Never report a lifetime
//! high-water as an incremental figure"*). It therefore cannot say whether the
//! product or the harness owns the bytes.
//!
//! This probe stages the driver's own setup in order and reads current RSS and the
//! counting allocator at each boundary, so the attribution is measured rather than
//! argued. It registers no row, writes no receipt and is not evidence.
//!
//! **One test, deliberately.** RSS only ever grows within a process, and the
//! counting allocator is process-global, so two diagnostics in one test binary
//! contaminate each other in whatever order the harness happens to run them - which
//! is exactly what the first version of this file did, and it reported the second
//! diagnostic's stages 2x too high. Both measurements therefore live in one test and
//! run in a fixed order.
//!
//! ```text
//! cargo test --release --locked --test namespace_memory_probe -- --nocapture
//! ```

use fs_bench_storage_content::ops::fs::WALK_CEILING;
use fs_bench_storage_content::ops::fs_fixture::{PreparedTree, Recipe, ROOT_SERIAL};
use fs_bench_storage_content::ops::namespace_content::{self, Declaration};
use fs_bench_storage_content::support::instruments;
use fs_bench_storage_content::workload::providers::{PairProvider, TreeStore};
use layerfs_content::filesystem::references::backing::{FileBacking, OrderingBacking};
use layerfs_content::filesystem::{
    build_filesystem, scope_for_seed, update_filesystem, FilesystemInput, FilesystemObjects,
    FilesystemResources,
};
use layerfs_content::{construct_bytes, ConstructionPolicy};

const SEED: u64 = 0x1234_5678_9abc_def0;

fn rss() -> u64 {
    instruments::process_usage().map(|u| u.resident_bytes).unwrap_or(0)
}

/// Live bytes the counting allocator holds, as `end - base` of a zero-length window.
fn heap_current() -> i64 {
    let window = instruments::heap_end();
    window.end_bytes as i64 - window.base_bytes as i64
}

#[test]
fn namespace_memory_attribution() {
    // ---- Part 1: the driver's per-batch prefix snapshots -------------------
    //
    // `prefixes` keeps one `TreeStore` snapshot per batch, and `TreeStore::absorb`
    // **copies** (`providers.rs:100-107`). Each snapshot is a strict subset of the
    // one after it, so the driver retains the *sum over batches* of the chain so
    // far. This builds the real chain through the real product.
    {
        let declaration = Declaration::LARGE;
        let prepared = Recipe {
            profile: "binary-v1",
            entries: declaration.entries,
            directories: declaration.directories,
            seed: SEED,
        }
        .prepare();
        let batches = prepared.batches(WALK_CEILING).expect("batches");
        let scope = scope_for_seed([7_u8; 32]);
        let empty = TreeStore::new();
        let scratch = std::env::temp_dir().join(format!("ns-prefix-probe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).expect("scratch");
        let mut backings: Vec<FileBacking> = Vec::new();
        for index in 0..batches.len() {
            let dir = scratch.join(format!("b{index}"));
            std::fs::create_dir_all(&dir).expect("backing dir");
            backings.push(FileBacking::new(dir));
        }

        instruments::heap_begin();
        let base = heap_current();
        let mut chain = TreeStore::new();
        let mut prefixes: Vec<TreeStore> = vec![TreeStore::new()];
        let mut root = None;
        for (index, batch) in batches.iter().enumerate() {
            let input = FilesystemInput {
                base: root,
                scope,
                root_serial: ROOT_SERIAL,
                directories: &batch.directories,
                inodes: &batch.inodes,
                new_inodes: &batch.new_inodes,
                resources: FilesystemResources::default(),
            };
            let mut emitted = TreeStore::new();
            let reader = PairProvider::new(&chain, &empty);
            let built = {
                let mut objects = FilesystemObjects::new(&reader, &mut emitted);
                let ordering: Option<&mut dyn OrderingBacking> =
                    Some(&mut backings[index] as &mut dyn OrderingBacking);
                if root.is_none() {
                    build_filesystem(&mut objects, &input, ordering)
                } else {
                    update_filesystem(&mut objects, &input, ordering)
                }
            };
            root = Some(built.expect("batch build").root);
            chain.absorb(&emitted);
            let mut snapshot = TreeStore::new();
            snapshot.absorb(&chain);
            prefixes.push(snapshot);
        }
        let with_prefixes = heap_current() - base;
        let retained: usize = prefixes.iter().map(TreeStore::len).sum();
        let chain_only = chain.len();
        drop(prefixes);
        let without_prefixes = heap_current() - base;
        eprintln!("PREFIX SNAPSHOTS");
        eprintln!(
            "  batches {}  final chain {} objects  snapshots {}  objects retained {}",
            batches.len(),
            chain_only,
            batches.len(),
            retained
        );
        eprintln!("  heap with all prefixes   = {with_prefixes} bytes");
        eprintln!("  heap with the chain only = {without_prefixes} bytes");
        eprintln!(
            "  the snapshots cost {} bytes ({:.1} x the chain alone)",
            with_prefixes - without_prefixes,
            (with_prefixes as f64) / (without_prefixes.max(1) as f64)
        );
        drop(chain);
        drop(backings);
        let _ = std::fs::remove_dir_all(&scratch);
        assert!(with_prefixes > without_prefixes);
    }

    // ---- Part 2: the driver's setup, stage by stage ------------------------
    instruments::heap_begin();
    let base = rss();
    let stage = |label: &str| {
        eprintln!(
            "  {label:36} rss={:>13}  ({:+14})  heap_live={:>+14}",
            rss(),
            rss() as i64 - base as i64,
            heap_current()
        );
    };
    eprintln!("DRIVER SETUP");
    stage("process start");

    let declaration = Declaration::LARGE;
    let prepared: PreparedTree = Recipe {
        profile: "binary-v1",
        entries: declaration.entries,
        directories: declaration.directories,
        seed: SEED,
    }
    .prepare();
    stage("after PreparedTree::prepare");

    let plan = namespace_content::plan(&declaration, SEED).expect("plan");
    stage("after plan()");
    eprintln!(
        "     plan: {} files, {} bytes, largest {}",
        plan.files.len(),
        plan.total_bytes,
        plan.largest()
    );

    let policy = ConstructionPolicy::frozen_default();
    let capacities = policy.capacities();
    let mut content = TreeStore::new();
    for file in &plan.files {
        if file.size == 0 {
            continue;
        }
        let index = u64::from(file.directory) * namespace_content::FILES_PER_DIRECTORY
            + u64::from(file.serial)
            - (2 + u64::from(declaration.directories));
        let bytes = fs_bench_storage_content::fixture::noise(file.size, SEED ^ index.rotate_left(13));
        let (result, _) = layerfs_telemetry::timer::Timing::disabled(
            "setup.construct",
            |scope: &layerfs_telemetry::timer::TimingScope<'_, layerfs_telemetry::timer::Active>| {
                construct_bytes(policy, &capacities, &bytes, &mut content, scope.child("content"))
            },
        );
        result.expect("construct");
    }
    stage("content store alone (pre-chain)");
    let canonical: u64 = content
        .insertion_order()
        .iter()
        .filter_map(|id| content.object(*id))
        .map(|object| object.canonical_len() as u64)
        .sum();
    eprintln!(
        "     content: {} objects, {} canonical bytes",
        content.len(),
        canonical
    );

    // `absorb` copies, so the chain is built from the content store and the content
    // store is then dropped: the driver holds one copy, never two.
    let mut chain = TreeStore::new();
    chain.absorb(&content);
    drop(content);
    stage("driver steady state (tree + chain)");
    eprintln!(
        "  prepared tree bindings = {}, chain objects = {}",
        prepared.bindings(),
        chain.len()
    );
}
