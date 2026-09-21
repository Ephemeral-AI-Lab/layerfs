//! Diagnostic for the namespace-10000 parity driver's timed pass.
//!
//! Reproduces, without registering a row, the two loops the driver runs: the untimed
//! reader chain (which succeeds) and the timed pass (which returns
//! `InvalidRecord("cycle check work limit")`). It reports which batch trips and with
//! what budget, so the next fix is chosen from a measurement.

use fs_bench_storage_content::ops::fs_fixture::{PreparedTree, Recipe};
use fs_bench_storage_content::workload::providers::{PairProvider, TreeStore};
use layerfs_content::filesystem::references::backing::FileBacking;
use layerfs_content::filesystem::{
    build_filesystem, scope_for_seed, update_filesystem, FilesystemInput, FilesystemObjects,
    FilesystemResources,
};

const ENTRIES: u32 = 10_000;
const DIRECTORIES: u32 = 100;

fn scope_of(_seed: u64) -> layerfs_content::InodeScope {
    scope_for_seed([7_u8; 32])
}

/// Runs the batches once, with `complete` as the reader, and returns the first error.
fn run_batches(
    prepared: &PreparedTree,
    budget: usize,
    scratch: &str,
) -> Result<usize, String> {
    let batches = prepared.batches(budget).map_err(|e| format!("batches: {e}"))?;
    let scope = scope_of(1);
    let empty = TreeStore::new();
    let mut chain = TreeStore::new();
    let mut backings: Vec<FileBacking> = Vec::new();
    for index in 0..batches.len() {
        let dir = std::env::temp_dir().join(format!("nsprobe-{scratch}-{index}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| format!("scratch: {e}"))?;
        backings.push(FileBacking::new(dir));
    }
    let mut root = None;
    for (index, batch) in batches.iter().enumerate() {
        let input = FilesystemInput {
            base: root,
            scope,
            root_serial: fs_bench_storage_content::ops::fs_fixture::ROOT_SERIAL,
            directories: &batch.directories,
            inodes: &batch.inodes,
            new_inodes: &batch.new_inodes,
            resources: FilesystemResources::default(),
        };
        let mut emitted = TreeStore::new();
        let reader = PairProvider::new(&chain, &empty);
        let built = {
            let mut objects = FilesystemObjects::new(&reader, &mut emitted);
            let ordering: Option<&mut dyn layerfs_content::filesystem::references::backing::OrderingBacking> =
                Some(&mut backings[index] as &mut dyn layerfs_content::filesystem::references::backing::OrderingBacking);
            if root.is_none() {
                build_filesystem(&mut objects, &input, ordering)
            } else {
                update_filesystem(&mut objects, &input, ordering)
            }
        };
        match built {
            Ok(built) => {
                root = Some(built.root);
                eprintln!("  {scratch} batch {index}: OK bindings={}", batch.bindings());
            }
            Err(error) => {
                eprintln!("  {scratch} batch {index}: FAIL bindings={} {error:?}", batch.bindings());
                return Err(format!("{error:?}"));
            }
        }
        chain.absorb(&emitted);
    }
    Ok(chain.len())
}

#[test]
fn probe_batched_build_under_two_budgets() {
    let prepared = Recipe {
        profile: "binary-v1",
        entries: ENTRIES,
        directories: DIRECTORIES,
        seed: 1,
    }
    .prepare();
    eprintln!("bindings={}", prepared.bindings());
    for budget in [4_096_usize, 2_048, 1_024] {
        eprintln!("budget {budget}:");
        match run_batches(&prepared, budget, &format!("b{budget}")) {
            Ok(objects) => eprintln!("  => PASS, chain objects {objects}"),
            Err(error) => eprintln!("  => FAIL {error}"),
        }
    }
}
