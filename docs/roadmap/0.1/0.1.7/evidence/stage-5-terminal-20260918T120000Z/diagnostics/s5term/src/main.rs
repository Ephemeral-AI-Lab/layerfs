//! Round-4 terminal-handoff probes for the Stage 5 ordering rows.
//!
//! This client depends on the two core packages as path dependencies, uses only
//! their public entry points, and prints one labelled block per probe. Nothing
//! in it is product code and nothing in the repository imports it.
//!
//! Row coverage:
//! - `grid` (R2-F8/N-6): the ordering scaling grid - the same fixture shape the
//!   round-2 review measured (4,000-file base, `maximum_pending_records = 64`,
//!   four change sizes), with the review's own columns plus the counters the
//!   simultaneous-memory row needs.
//! - `coexist` (TR-5): what one update operation holds at once during its
//!   references phase - pending rows, ordering bytes, live-tier scan buffers and
//!   caller backing, each with its scope, unit and the counter that produced it
//!   - and the sequential-phase scratch peaks beside them.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use layerfs_content::filesystem::references::backing::{FileBacking, OrderingBacking};
use layerfs_content::filesystem::references::runs::DEFAULT_MERGE_BUFFER_BYTES;
use layerfs_content::filesystem::references::record::ROW_BYTES;
use layerfs_content::filesystem::{
    build_filesystem, scope_for_seed, update_filesystem, DirectoryUpdate, FilesystemInput,
    FilesystemObjects, FilesystemResources, FilesystemRootId, InodeScope, InodeUpdate, PathName,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, FinalizedObject, ObjectId,
    ObjectRole,
};

#[derive(Clone, Debug, Default)]
struct Bag {
    objects: BTreeMap<ObjectId, (ObjectRole, Vec<u8>)>,
}

impl Bag {
    fn get(&self, id: ObjectId) -> ContentResult<Vec<u8>> {
        self.objects
            .get(&id)
            .map(|(_, bytes)| bytes.clone())
            .ok_or(ContentError::MissingObject)
    }
}

impl FinalizedConsumer for Bag {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        let parts = object.into_parts();
        self.objects
            .insert(parts.id, (parts.role, parts.canonical));
        Ok(())
    }
}

impl AuthenticatedObjects for Bag {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        ids.iter().map(|id| self.get(*id)).collect()
    }
}

fn name(value: &str) -> PathName {
    PathName::new(value).expect("name")
}

fn synthetic(label: &str) -> ObjectId {
    ObjectId::for_bytes(format!("layerfs/s5term/{label}").as_bytes())
}

fn scope() -> InodeScope {
    scope_for_seed([0x41; 32])
}

struct Fixture {
    bag: Bag,
    root: ObjectId,
}

/// Builds root (serial 1) with directory d (serial 2) holding `files` files.
fn fixture(files: usize) -> Fixture {
    let mut bag = Bag::default();
    let mut inodes: Vec<InodeUpdate> = vec![
        InodeUpdate {
            serial: 1,
            value: InodeValue {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: synthetic("root-content"),
                metadata_root: synthetic("root-meta"),
            },
        },
        InodeUpdate {
            serial: 2,
            value: InodeValue {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: synthetic("d-content"),
                metadata_root: synthetic("d-meta"),
            },
        },
    ];
    let mut new_inodes: Vec<u64> = vec![1, 2];
    for index in 0..files {
        inodes.push(InodeUpdate {
            serial: index as u64 + 3,
            value: InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 0,
                content_root: synthetic(&format!("content-{index:05}")),
                metadata_root: synthetic("file-meta"),
            },
        });
        new_inodes.push(index as u64 + 3);
    }
    let directories = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![(name("d"), Some(2))],
        },
        DirectoryUpdate {
            parent: 2,
            changes: (0..files)
                .map(|index| (name(&format!("f{index:05}")), Some(index as u64 + 3)))
                .collect(),
        },
    ];
    let input = FilesystemInput {
        base: None,
        scope: scope(),
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &new_inodes,
        resources: FilesystemResources::default(),
    };
    let reader = bag.clone();
    let mut sink = Bag::default();
    // The build is itself an operation: above the default pending-row ceiling it
    // needs a caller-owned ordering backing, exactly like any other operation.
    let mut backing = FileBacking::new(backing_dir("fixture"));
    let result = {
        let mut objects = FilesystemObjects::new(&reader, &mut sink);
        let backing_ref: &mut dyn OrderingBacking = &mut backing;
        build_filesystem(&mut objects, &input, Some(backing_ref))
    }
    .expect("fixture build");
    bag.objects.extend(sink.objects);
    Fixture {
        bag,
        root: result.root.0,
    }
}

/// A rename batch over the first `pairs` names of d.
fn rename_pairs(pairs: usize) -> Vec<DirectoryUpdate> {
    let mut changes: Vec<(PathName, Option<u64>)> = Vec::with_capacity(pairs * 2);
    for index in 0..pairs {
        changes.push((name(&format!("f{index:05}")), None));
        changes.push((name(&format!("r{index:05}")), Some(index as u64 + 3)));
    }
    changes.sort_by(|left, right| left.0.cmp(&right.0));
    vec![DirectoryUpdate {
        parent: 2,
        changes,
    }]
}

fn backing_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("layerfs-s5term-{label}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("backing directory");
    dir
}

/// One measured update: the pair count's row of the scaling grid.
fn measured_update(
    fixture: &Fixture,
    pairs: usize,
) -> (layerfs_content::filesystem::FilesystemResult, u128) {
    let directories = rename_pairs(pairs);
    let input = FilesystemInput {
        base: Some(FilesystemRootId(fixture.root)),
        scope: scope(),
        root_serial: 1,
        directories: &directories,
        inodes: &[],
        new_inodes: &[],
        resources: FilesystemResources {
            maximum_pending_records: 64,
            ..FilesystemResources::default()
        },
    };
    let mut backing = FileBacking::new(backing_dir(&format!("grid-{pairs}")));
    let started = Instant::now();
    let mut sink = Bag::default();
    let result = {
        let reader = fixture.bag.clone();
        let mut objects = FilesystemObjects::new(&reader, &mut sink);
        let backing_ref: &mut dyn OrderingBacking = &mut backing;
        update_filesystem(&mut objects, &input, Some(backing_ref))
    };
    let elapsed = started.elapsed().as_nanos();
    let result = result.expect("update");
    let work = result.counters.references.runs;
    println!(
        "{:>6} {:>8} {:>13} {:>10} {:>10} {:>6} {:>11} {:>11} {:>11} {:>9} {:>7} {:>7} {:>7} {:>9}",
        pairs,
        result.counters.references.rows_spilled,
        elapsed,
        work.rows_read,
        work.rows_written,
        work.runs_created,
        work.peak_run_bytes,
        backing.held_bytes(),
        backing.peak_bytes(),
        result.counters.objects.objects_emitted,
        result.counters.references.peak_pending,
        result.counters.directories.peak_scratch_bytes,
        result.counters.inodes.peak_scratch_bytes,
        work.peak_live_runs,
    );
    (result, elapsed)
}

/// R2-F8/N-6: the ordering scaling grid at a fixed pending ceiling.
fn probe_grid() {
    println!("== grid: ordering scaling, maximum_pending_records = 64, base 4000 files ==");
    println!(
        "{:>6} {:>8} {:>13} {:>10} {:>10} {:>6} {:>11} {:>11} {:>11} {:>9} {:>7} {:>7} {:>7} {:>9}",
        "pairs", "spilled", "elapsed_ns", "rows_read", "rows_writ", "runs", "peak_owned",
        "held_now", "peak_back", "emitted", "pend", "dir_scr", "ino_scr", "tiers"
    );
    let mut previous: Option<(usize, u64, u128)> = None;
    for pairs in [250usize, 500, 1000, 2000] {
        let fixture = fixture(4000);
        let (result, elapsed) = measured_update(&fixture, pairs);
        let work = result.counters.references.runs;
        if let Some((prior_pairs, prior_reads, prior_elapsed)) = previous {
            println!(
                "     doubling {} -> {}: rows_read x{:.2}, elapsed x{:.2}",
                prior_pairs,
                pairs,
                work.rows_read as f64 / prior_reads as f64,
                elapsed as f64 / prior_elapsed as f64
            );
        }
        previous = Some((pairs, work.rows_read, elapsed));
    }
}

/// TR-5: what one operation holds at once, with scope, unit and counter.
fn probe_coexist() {
    println!("== coexist: simultaneous memory and backing during one update ==");
    println!("phases are sequential: validate, directories, references, inodes, cleanup, root.encode");
    println!("the references phase holds all four rows below at once; the scratch rows belong to their own phases");
    for pairs in [2000usize] {
        let fixture = fixture(4000);
        let (result, _elapsed) = measured_update(&fixture, pairs);
        let work = result.counters.references.runs;
        let pending_rows = result.counters.references.peak_pending as u64;
        println!("  references phase, simultaneous owners (pairs = {pairs}):");
        println!(
            "    pending rows:            {:>9} rows = {:>9} B  (counter references.peak_pending x {ROW_BYTES} B/row)",
            pending_rows,
            pending_rows * ROW_BYTES as u64
        );
        println!(
            "    ordering bytes:          {:>9} B              (counter references.runs.peak_run_bytes: live runs + spilled-but-unmerged inputs + pending + reserved outputs)",
            work.peak_run_bytes
        );
        println!(
            "    live-tier scan buffers:  {:>9} tiers x {DEFAULT_MERGE_BUFFER_BYTES} B = {:>9} B  (counter references.runs.peak_live_runs; derived bound MAXIMUM_LEVELS x merge buffer)",
            work.peak_live_runs,
            work.peak_live_runs as u64 * DEFAULT_MERGE_BUFFER_BYTES as u64
        );
        // The backing peak is the physical side of the same phase; the grid row
        // above prints it for this pair count (peak_back column).
        println!(
            "    caller backing peak:     the peak_back column of the grid row above (counter FileBacking::peak_bytes, the caller-owned physical owner)"
        );
        println!("  sequential-phase scratch leases:");
        println!(
            "    directories scratch peak: {:>9} B           (counter directories.peak_scratch_bytes)",
            result.counters.directories.peak_scratch_bytes
        );
        println!(
            "    inodes scratch peak:      {:>9} B           (counter inodes.peak_scratch_bytes)",
            result.counters.inodes.peak_scratch_bytes
        );
        println!("  no process-level RSS, cgroup or page-cache figure is claimed: every number above is the operation's own counter");
    }
}

fn main() {
    let which: Vec<String> = std::env::args().skip(1).collect();
    let run = |label: &str| which.is_empty() || which.iter().any(|item| item == label);
    println!("s5term: round-4 terminal-handoff probes (public API only)");
    if run("grid") {
        probe_grid();
    }
    if run("coexist") {
        probe_coexist();
    }
}
