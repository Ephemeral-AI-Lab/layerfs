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
//! **Part 1 was rewritten when the snapshots were removed** (round 22, issue #226).
//! It used to build the `TreeStore` snapshot per batch and report what they cost;
//! that shape no longer exists in the driver, so the probe now runs the driver's own
//! two pieces - one complete chain, plus one cumulative `PrefixKeys` per batch - and
//! reports what *they* cost. The 164,347,158 bytes the snapshots cost is carried in
//! the printed comparison as a labelled constant from round 21's own probe run, not
//! as a measurement this file still makes: a probe that kept the dead shape alive to
//! keep printing a number would be measuring code the driver does not run.
//!
//! ```text
//! cargo test --release --locked --test namespace_memory_probe -- --nocapture
//! ```

use fs_bench_storage_content::ops::fs::WALK_CEILING;
use fs_bench_storage_content::ops::fs_fixture::{PreparedTree, Recipe, ROOT_SERIAL};
use fs_bench_storage_content::ops::namespace_content::{self, Declaration};
use fs_bench_storage_content::support::instruments;
use fs_bench_storage_content::workload::providers::{PairProvider, PrefixKeys, TreeStore};
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
    // ---- Part 1: the driver's one chain and its per-batch prefix keys ------
    //
    // The driver used to keep one `TreeStore` snapshot per batch, and because
    // `TreeStore::absorb` **copies**, it retained the sum over batches of the chain
    // so far: 25 snapshots holding 66,824 objects and 164,347,158 bytes to serve a
    // chain whose final state is 4,221 objects and 12,452,785 bytes. It now keeps one
    // complete chain and one cumulative `PrefixKeys` per batch, which is what this
    // part builds through the real product.
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
        let mut prefixes: Vec<PrefixKeys> = vec![PrefixKeys::new()];
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
            // The driver's own rule, repeated here so the probe measures the shape
            // the row runs and fails if that shape changes under it.
            let mut keys = prefixes[index].clone();
            keys.extend(emitted.insertion_order());
            assert_eq!(
                keys.len(),
                chain.len(),
                "batch {index}: the key set must describe the chain"
            );
            prefixes.push(keys);
        }
        let with_keys = heap_current() - base;
        let retained: usize = prefixes.iter().map(PrefixKeys::len).sum();
        let largest = prefixes.iter().map(PrefixKeys::len).max().unwrap_or(0);
        let chain_only = chain.len();
        let prefixes_batches = prefixes.len() - 1;
        drop(prefixes);
        let without_prefixes = heap_current() - base;
        eprintln!("PREFIX KEYS");
        eprintln!(
            "  batches {}  final chain {} objects  key sets {}  identities retained {}  largest {}",
            batches.len(),
            chain_only,
            prefixes_batches,
            retained,
            largest
        );
        eprintln!("  heap with the key sets  = {with_keys} bytes");
        eprintln!("  heap with the chain only = {without_prefixes} bytes");
        eprintln!(
            "  the key sets cost {} bytes ({:.1} x the chain alone)",
            with_keys - without_prefixes,
            (with_keys as f64) / (without_prefixes.max(1) as f64)
        );
        // Round 21's own probe run, on the shape this replaces. A labelled constant,
        // because the code that produced it is gone: `report.md` section 11.3.
        const ROUND_21_SNAPSHOT_COST: i64 = 164_347_158;
        let key_cost = with_keys - without_prefixes;
        eprintln!(
            "  round 21's snapshots cost {ROUND_21_SNAPSHOT_COST} bytes (14.2 x the chain); \
             the key sets cost {key_cost} ({:.1} % of that, a saving of {} bytes)",
            (key_cost as f64) * 100.0 / (ROUND_21_SNAPSHOT_COST as f64),
            ROUND_21_SNAPSHOT_COST - key_cost
        );
        drop(chain);
        drop(backings);
        let _ = std::fs::remove_dir_all(&scratch);
        assert!(with_keys >= without_prefixes);
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
