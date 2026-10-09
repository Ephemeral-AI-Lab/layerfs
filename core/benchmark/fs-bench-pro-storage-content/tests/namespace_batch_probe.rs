//! Functional parity of historical namespace batches after R4 validation.
//!
//! No row is registered or timed here. Every preparation batch must succeed,
//! a discarding pass must reproduce each prepared root, and the final root's
//! complete listings must equal the independent recipe manifest. The widths are
//! historical workload choices, not product topology-work ceilings.

use fs_bench_storage_content::ops::fs::listings_match;
use fs_bench_storage_content::ops::fs_fixture::{PreparedTree, Recipe, ROOT_SERIAL};
use fs_bench_storage_content::workload::providers::{PairProvider, TreeStore};
use layerfs_content::filesystem::references::backing::FileBacking;
use layerfs_content::filesystem::{
    build_filesystem, scope_for_seed, update_filesystem, FilesystemInput, FilesystemObjects,
    FilesystemResources, FilesystemRootId,
};
use layerfs_content::DiscardingConsumer;

const ENTRIES: u32 = 10_000;
const DIRECTORIES: u32 = 100;

fn backing(phase: &str, width: usize, index: usize) -> FileBacking {
    let dir = std::env::temp_dir().join(format!(
        "nsprobe-{}-{phase}-{width}-{index}",
        std::process::id()
    ));
    std::fs::create_dir(&dir).expect("new owned scratch directory");
    FileBacking::new(dir)
}

fn run_batches(prepared: &PreparedTree, width: usize) {
    let batches = prepared.batches(width).expect("declared batching");
    let scope = scope_for_seed([7_u8; 32]);
    let empty = TreeStore::new();
    let mut chain = TreeStore::new();
    let mut roots = Vec::new();
    let mut root = None;
    for (index, batch) in batches.iter().enumerate() {
        assert!(batch.bindings() <= width, "declared workload width");
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
        let mut ordering = backing("prepared", width, index);
        let mut objects = FilesystemObjects::new(&reader, &mut emitted);
        let built = match root {
            None => build_filesystem(&mut objects, &input, Some(&mut ordering)),
            Some(_) => update_filesystem(&mut objects, &input, Some(&mut ordering)),
        }
        .unwrap_or_else(|error| panic!("prepared width {width} batch {index}: {error:?}"));
        root = Some(built.root);
        roots.push(built.root);
        chain.absorb(&emitted);
    }
    let final_root = root.expect("at least one prepared batch");
    assert_eq!(
        listings_match(&chain, final_root, prepared).expect("complete listing oracle"),
        Ok(prepared.listings.len() as u64)
    );

    // Every base is prepared before this discarding pass. No emitted object is
    // retained by the consumer, matching the historical driver's operation.
    let mut root: Option<FilesystemRootId> = None;
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
        let reader = PairProvider::new(&chain, &empty);
        let mut consumer = DiscardingConsumer::new();
        let mut ordering = backing("discarding", width, index);
        let mut objects = FilesystemObjects::new(&reader, &mut consumer);
        let built = match root {
            None => build_filesystem(&mut objects, &input, Some(&mut ordering)),
            Some(_) => update_filesystem(&mut objects, &input, Some(&mut ordering)),
        }
        .unwrap_or_else(|error| panic!("discarding width {width} batch {index}: {error:?}"));
        assert_eq!(built.root, roots[index], "width {width} batch {index} root");
        root = Some(built.root);
    }
    assert_eq!(root, Some(final_root), "complete chain root parity");
}

#[test]
fn historical_namespace_batches_reproduce_roots_and_complete_listings() {
    let prepared = Recipe {
        profile: "binary-v1",
        entries: ENTRIES,
        directories: DIRECTORIES,
        seed: 1,
    }
    .prepare();
    assert_eq!(prepared.bindings(), 10_100);
    for width in [4_096_usize, 2_048, 1_024] {
        run_batches(&prepared, width);
    }
}
