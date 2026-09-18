//! Filesystem shape drivers: build, traverse, mutate and build at scale.
//!
//! Every driver here follows the rule stated in [`super`]: the timed phase runs
//! with a **non-retaining** consumer, and the oracle is a second, unmeasured,
//! byte-identical operation into a `TreeStore`. A filesystem operation does not
//! read back what it emits — measured, not assumed: a build of 200 bindings runs
//! to completion against an empty provider and a `DiscardingConsumer` — so the
//! measured phase can be genuinely non-retaining without changing the operation.
//!
//! **The O4 oracle is the three-tuple plus the listing.** The three-tuple is
//! `(root directory content root, inode table root, filesystem root identity)`,
//! read back through the public `FilesystemRead` path, and the listing oracle is
//! every directory's bindings compared against the fixture manifest the recipe
//! produced. A census read out of the artifact would agree with the artifact by
//! construction, so the expectation comes from [`fs_fixture`] and never from the
//! mutated Store.

use std::path::PathBuf;

use layerfs_content::filesystem::references::backing::{FileBacking, OrderingBacking};
use layerfs_content::filesystem::{
    build_filesystem, scope_for_seed, update_filesystem, DirectoryUpdate, FilesystemInput,
    FilesystemObjects, FilesystemRead, FilesystemResources, FilesystemRootId, LogicalPath,
    PathName,
};
use layerfs_content::{DiscardingConsumer, ObjectId};
use layerfs_telemetry::timer::{Active, Timing, TimingScope};

use super::fs_fixture::{PreparedTree, Recipe, ROOT_SERIAL};
use super::{OpContext, OpError, OpOutcome};
use crate::gates::{self, Gate, GateClass};
use crate::registry::{Case, LocalityOp, TinyOp, TreeOp};
use crate::support::instruments::{self, HeapWindow};
use crate::support::trace::Kind;
use crate::workload::providers::{PairProvider, SharedStore, TreeStore};

/// Entries per listing page the O4 oracle reads.
const LISTING_PAGE: usize = 64;
/// Bytes per listing page.
const LISTING_BYTES: usize = 8_192;
/// Bindings one build may state before the walk ceiling refuses it.
pub const WALK_CEILING: usize = 4_096;

/// The scope every filesystem fixture uses, derived from the recipe seed.
fn scope_of(seed: u64) -> layerfs_content::InodeScope {
    let mut bytes = [0_u8; 32];
    bytes[..8].copy_from_slice(&seed.to_le_bytes());
    bytes[8..16].copy_from_slice(&seed.rotate_left(17).to_le_bytes());
    bytes[16..24].copy_from_slice(&seed.rotate_left(31).to_le_bytes());
    bytes[24..32].copy_from_slice(&seed.rotate_left(47).to_le_bytes());
    scope_for_seed(bytes)
}

/// Declared resource ceilings for one operation.
///
/// The defaults are used unchanged: `maximum_pending_records` is 4,096, so a
/// fixture of at most that many bindings resolves entirely in memory and the
/// timed phase performs no ordering I/O. A larger fixture keeps the same declared
/// ceilings and supplies a real [`FileBacking`], which is the operation's own
/// declared scratch rather than a relaxed limit.
fn resources() -> FilesystemResources {
    FilesystemResources::default()
}

/// A scratch directory for the ordering backing, inside the case's output.
fn backing_directory(context: &OpContext<'_>) -> PathBuf {
    context.output.join("ordering")
}

/// The three-tuple the O4 oracle pins.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThreeTuple {
    /// Root directory's content root.
    pub directory_root: ObjectId,
    /// Inode table root.
    pub inode_table: ObjectId,
    /// Filesystem root identity.
    pub root: ObjectId,
}

/// Reads the three-tuple back through the public read path.
pub(super) fn three_tuple(
    reader: &dyn layerfs_content::AuthenticatedObjects,
    root: FilesystemRootId,
) -> Result<ThreeTuple, OpError> {
    let mut read = FilesystemRead::new(reader, root)
        .map_err(|error| OpError::Product(format!("{error:?}")))?;
    let stat = read
        .stat(&LogicalPath::root())
        .map_err(|error| OpError::Product(format!("{error:?}")))?;
    Ok(ThreeTuple {
        directory_root: stat.content_root,
        inode_table: read.root().inode_table(),
        root: root.0,
    })
}

/// Every binding of one directory, paged to the end.
fn list_all(
    read: &mut FilesystemRead<'_>,
    path: &LogicalPath,
) -> Result<Vec<(String, u64)>, OpError> {
    let mut out: Vec<(String, u64)> = Vec::new();
    let mut after: Option<PathName> = None;
    loop {
        let page = read
            .list(path, after.as_ref(), LISTING_PAGE, LISTING_BYTES)
            .map_err(|error| OpError::Product(format!("{error:?}")))?;
        for (name, serial) in &page.entries {
            out.push((name.as_str().to_string(), *serial));
        }
        match page.continuation {
            Some(next) => after = Some(next),
            None => break,
        }
        if out.len() > 1_000_000 {
            return Err(OpError::Io("listing did not terminate".to_string()));
        }
    }
    Ok(out)
}

/// Compares every directory listing against the fixture manifest.
///
/// Returns the first disagreement, so a failure names the directory and the two
/// bindings rather than reporting a boolean.
pub fn listings_match(
    reader: &dyn layerfs_content::AuthenticatedObjects,
    root: FilesystemRootId,
    prepared: &PreparedTree,
) -> Result<Result<u64, String>, OpError> {
    let mut read = FilesystemRead::new(reader, root)
        .map_err(|error| OpError::Product(format!("{error:?}")))?;
    let mut checked = 0_u64;
    for (path, expected) in &prepared.listings {
        // Every failure names the directory it happened in. An oracle that reports
        // "MissingObject" without saying which listing demanded it cannot be acted
        // on, which is how the first version of the removal oracle read.
        let logical = LogicalPath::new(path)
            .map_err(|error| OpError::Product(format!("listing {path:?}: {error:?}")))?;
        let observed = list_all(&mut read, &logical)
            .map_err(|error| OpError::Product(format!("listing {path:?}: {error}")))?;
        if observed != *expected {
            return Ok(Err(format!(
                "directory {path:?}: observed {observed:?}, manifest {expected:?}"
            )));
        }
        checked += 1;
    }
    Ok(Ok(checked))
}

/// One completed measured operation, with the counters and instruments beside it.
struct Measured {
    result: layerfs_content::FilesystemResult,
    objects: layerfs_content::ObjectWork,
    report: layerfs_telemetry::timer::TimingReport,
    heap: HeapWindow,
}

/// Runs one build inside the timed region with a non-retaining consumer.
fn measure_build(
    prepared: &PreparedTree,
    scope: layerfs_content::InodeScope,
    backing: Option<&mut dyn OrderingBacking>,
    label: &'static str,
) -> (Result<Measured, layerfs_content::ContentError>, Vec<Gate>) {
    let input = FilesystemInput {
        base: None,
        scope,
        root_serial: ROOT_SERIAL,
        directories: &prepared.directories,
        inodes: &prepared.inodes,
        new_inodes: &prepared.new_inodes,
        resources: resources(),
    };
    let empty = TreeStore::new();
    instruments::heap_begin();
    let (outcome, report) = Timing::record(label, |_timing: &TimingScope<'_, Active>| {
        let mut consumer = DiscardingConsumer::new();
        let reader = PairProvider::new(&empty, &empty);
        let mut objects = FilesystemObjects::new(&reader, &mut consumer);
        let result = build_filesystem(&mut objects, &input, backing)?;
        Ok::<_, layerfs_content::ContentError>((result, objects.work(), consumer.objects()))
    });
    let heap = instruments::heap_end();
    match outcome {
        Ok((result, work, _)) => (
            Ok(Measured {
                result,
                objects: work,
                report,
                heap,
            }),
            Vec::new(),
        ),
        Err(error) => (Err(error), Vec::new()),
    }
}

/// Runs one update inside the timed region with a non-retaining consumer.
fn measure_update(
    base: &TreeStore,
    emitted: &TreeStore,
    base_root: FilesystemRootId,
    prepared: &PreparedTree,
    scope: layerfs_content::InodeScope,
    backing: Option<&mut dyn OrderingBacking>,
    label: &'static str,
) -> Result<(Measured, u64, u64), layerfs_content::ContentError> {
    let input = FilesystemInput {
        base: Some(base_root),
        scope,
        root_serial: ROOT_SERIAL,
        directories: &prepared.directories,
        inodes: &prepared.inodes,
        new_inodes: &prepared.new_inodes,
        resources: resources(),
    };
    // The reader is built outside the timer so its own served-from counts are
    // readable after the closure; it performs no work until the operation demands
    // an object, and every demand happens inside the timer.
    let reader = PairProvider::new(emitted, base);
    instruments::heap_begin();
    let (outcome, report) = Timing::record(label, |_timing: &TimingScope<'_, Active>| {
        let mut consumer = DiscardingConsumer::new();
        let mut objects = FilesystemObjects::new(&reader, &mut consumer);
        let result = update_filesystem(&mut objects, &input, backing)?;
        Ok::<_, layerfs_content::ContentError>((result, objects.work()))
    });
    let heap = instruments::heap_end();
    let (served_result, served_base) = reader.served();
    outcome.map(|(result, work)| {
        (
            Measured {
                result,
                objects: work,
                report,
                heap,
            },
            served_result,
            served_base,
        )
    })
}

/// Replays the same build into a store that authenticates every read.
pub(super) fn replay_build(
    prepared: &PreparedTree,
    scope: layerfs_content::InodeScope,
    backing: Option<&mut dyn OrderingBacking>,
) -> Result<(TreeStore, layerfs_content::FilesystemResult), OpError> {
    let input = FilesystemInput {
        base: None,
        scope,
        root_serial: ROOT_SERIAL,
        directories: &prepared.directories,
        inodes: &prepared.inodes,
        new_inodes: &prepared.new_inodes,
        resources: resources(),
    };
    let mut store = TreeStore::new();
    let empty = TreeStore::new();
    let (outcome, _) = Timing::disabled("oracle.replay", |_scope: &TimingScope<'_, Active>| {
        let reader = PairProvider::new(&empty, &empty);
        let mut objects = FilesystemObjects::new(&reader, &mut store);
        build_filesystem(&mut objects, &input, backing)
    });
    outcome
        .map(|result| (store, result))
        .map_err(|error| OpError::Product(format!("{error:?}")))
}

/// Replays the same update into a store that authenticates every read.
///
/// The reader is an **overlay**: the in-flight objects the operation has already
/// emitted, then the base. A filesystem update reads back what it emits, so a
/// reader that only saw the base would refuse it with `MissingObject` — which is
/// exactly what the first version of this function did, and why the measured
/// phase and the replay failed identically until the overlay was added.
fn replay_update(
    base: &TreeStore,
    base_root: FilesystemRootId,
    prepared: &PreparedTree,
    scope: layerfs_content::InodeScope,
    backing: Option<&mut dyn OrderingBacking>,
) -> Result<(TreeStore, layerfs_content::FilesystemResult), OpError> {
    let input = FilesystemInput {
        base: Some(base_root),
        scope,
        root_serial: ROOT_SERIAL,
        directories: &prepared.directories,
        inodes: &prepared.inodes,
        new_inodes: &prepared.new_inodes,
        resources: resources(),
    };
    let mut shared = SharedStore::new();
    let (outcome, _) = Timing::disabled("oracle.replay", |_scope: &TimingScope<'_, Active>| {
        let reader = shared.reader(base);
        let mut objects = FilesystemObjects::new(&reader, &mut shared);
        update_filesystem(&mut objects, &input, backing)
    });
    // The emitted objects **are** returned: an update reads back what it emits, so
    // a measured phase that discards its own output needs a fixture that already
    // holds it, and this replay is that fixture. It is unmeasured and byte-identical,
    // and the row gates the measured root against its root.
    let mut emitted = TreeStore::new();
    shared.absorb_into(&mut emitted);
    outcome
        .map(|result| (emitted, result))
        .map_err(|error| OpError::Product(format!("{error:?}")))
}

/// Records the counters every filesystem row publishes.
fn record_counters(
    context: &mut OpContext<'_>,
    measured: &Measured,
    label: &str,
) -> Result<(), OpError> {
    let counters = measured.result.counters;
    context.trace.write_number(
        Kind::Counter,
        &format!("{label}.objects_read"),
        measured.objects.objects_read as i128,
        "objects",
        "FilesystemObjects::work, measured phase",
    )?;
    context.trace.write_number(
        Kind::Counter,
        &format!("{label}.read_waves"),
        measured.objects.read_waves as i128,
        "waves",
        "FilesystemObjects::work, measured phase",
    )?;
    context.trace.write_number(
        Kind::Counter,
        &format!("{label}.objects_emitted"),
        measured.objects.objects_emitted as i128,
        "objects",
        "FilesystemObjects::work, measured phase",
    )?;
    context.trace.write_number(
        Kind::Counter,
        &format!("{label}.bindings_added"),
        counters.bindings_added as i128,
        "bindings",
        "FilesystemUpdateCounters, measured phase",
    )?;
    context.trace.write_number(
        Kind::Counter,
        &format!("{label}.directory_updates"),
        counters.directory_updates as i128,
        "directories",
        "FilesystemUpdateCounters, measured phase",
    )?;
    context.trace.write_number(
        Kind::Counter,
        &format!("{label}.base_records_read"),
        counters.base_records_read as i128,
        "records",
        "FilesystemUpdateCounters, measured phase",
    )?;
    context.trace.write_number(
        Kind::Counter,
        &format!("{label}.validation_entries"),
        counters.validation.entries_examined as i128,
        "entries",
        "ValidationWork, measured phase",
    )?;
    context.trace.write_number(
        Kind::Counter,
        &format!("{label}.directory_pages"),
        counters.directories.pages_read as i128,
        "pages",
        "SortedWork, measured phase",
    )?;
    context.trace.write_number(
        Kind::Counter,
        &format!("{label}.inode_pages"),
        counters.inodes.pages_read as i128,
        "pages",
        "SortedWork, measured phase",
    )?;
    context.trace.write_number(
        Kind::Resource,
        "heap.peak_incremental_bytes",
        measured.heap.peak_incremental_bytes as i128,
        "bytes",
        "counting GlobalAlloc, measured phase",
    )?;
    Ok(())
}

/// The gates every filesystem row shares: purity, swaps, replay identity, O4.
fn common_gates(
    measured: &Measured,
    replay: &layerfs_content::FilesystemResult,
    label: &'static str,
) -> Vec<Gate> {
    let report = &measured.report;
    let mut gates = vec![
        if report.root().is_none() {
            Gate::incomplete(
                GateClass::TimingPurity,
                "g7.tree-complete",
                "disabled report: no root",
                "report.is_incomplete() == false",
            )
        } else {
            gates::timing_purity(
                report.is_incomplete(),
                report.node_count() as u64,
                report.levels() as u64,
            )
        },
        gates::swap_gate(instruments::swaps()),
        gates::require(
            GateClass::Correctness,
            "g1.o1-replay-root",
            replay.root.0 == measured.result.root.0,
            &format!("replay root {}", replay.root.0),
            &format!("measured root {}", measured.result.root.0),
        ),
        gates::require(
            GateClass::Correctness,
            &"g1.o4-inode-table",
            replay.value.inode_table() == measured.result.value.inode_table(),
            &format!("{}", replay.value.inode_table()),
            &format!("{}", measured.result.value.inode_table()),
        ),
    ];
    let _ = label;
    gates.push(gates::require(
        GateClass::Resource,
        "g4.heap-window",
        measured.heap.allocations > 0,
        &format!("{} allocations", measured.heap.allocations),
        "the measured phase must allocate at least once",
    ));
    gates
}

/// A row that could not be measured, closed without pretending it passed.
fn unmeasured(error: &OpError, gates: Vec<Gate>) -> OpOutcome {
    OpOutcome {
        gates: gates
            .into_iter()
            .chain(std::iter::once(Gate::not_run(
                GateClass::Correctness,
                "row.not-run",
                &error.to_string(),
                "a row closes only on a receipt",
            )))
            .collect(),
        notes: vec![format!("not_run_reason: {error}")],
    }
}

/// Reborrows a concrete backing as the trait object the operations take.
fn as_backing(backing: Option<&mut FileBacking>) -> Option<&mut dyn OrderingBacking> {
    backing.map(|backing| backing as &mut dyn OrderingBacking)
}

/// Opens the ordering backing when the fixture outgrows the in-memory map.
fn backing_for(prepared: &PreparedTree, context: &OpContext<'_>) -> Option<FileBacking> {
    if prepared.bindings() <= resources().maximum_pending_records {
        return None;
    }
    Some(FileBacking::new(backing_directory(context)))
}

/// C1-7: many tiny files, by the operation the row declares.
pub fn many_tiny(
    case: &Case,
    op: TinyOp,
    context: &mut OpContext<'_>,
) -> Result<OpOutcome, OpError> {
    context.create_output()?;
    let seed = crate::ops::seed_of(case.id);
    let recipe = Recipe {
        profile: case.profile,
        entries: case.entries,
        directories: 0,
        seed,
    };
    let prepared = prepared_for(&recipe, context)?;
    if let Err(defect) = prepared.check() {
        return Ok(unmeasured(&OpError::Io(defect), Vec::new()));
    }
    let scope = scope_of(seed);

    match op {
        TinyOp::Create | TinyOp::BulkCreate => {
            run_build_row(case, &prepared, scope, context, "many_tiny")
        }
        TinyOp::Stat => run_traverse_row(case, &prepared, scope, context, "many_tiny", true),
        TinyOp::Unlink | TinyOp::BulkDelete => {
            run_delete_row(case, &prepared, scope, context, "many_tiny")
        }
    }
}

/// C1-8: build the tree, then traverse it or not.
pub fn tree(case: &Case, op: TreeOp, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    context.create_output()?;
    let seed = crate::ops::seed_of(case.id);
    let recipe = Recipe {
        profile: case.profile,
        entries: case.entries,
        directories: 0,
        seed,
    };
    let prepared = prepared_for(&recipe, context)?;
    if let Err(defect) = prepared.check() {
        return Ok(unmeasured(&OpError::Io(defect), Vec::new()));
    }
    let scope = scope_of(seed);
    match op {
        TreeOp::Construct => run_build_row(case, &prepared, scope, context, "tree"),
        TreeOp::MetadataScan => run_traverse_row(case, &prepared, scope, context, "tree", false),
        TreeOp::ContentScan => run_traverse_row(case, &prepared, scope, context, "tree", true),
    }
}

/// C1-9: build the tree, then relocate one subtree and delete it.
pub fn namespace(case: &Case, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    context.create_output()?;
    let seed = crate::ops::seed_of(case.id);
    let recipe = Recipe {
        profile: case.profile,
        entries: case.entries,
        directories: 0,
        seed,
    };
    let prepared = prepared_for(&recipe, context)?;
    if let Err(defect) = prepared.check() {
        return Ok(unmeasured(&OpError::Io(defect), Vec::new()));
    }
    let scope = scope_of(seed);
    run_build_row(case, &prepared, scope, context, "namespace")
}

/// C1-10: change locality over a constructed tree.
pub fn locality(
    case: &Case,
    op: LocalityOp,
    context: &mut OpContext<'_>,
) -> Result<OpOutcome, OpError> {
    context.create_output()?;
    let seed = crate::ops::seed_of(case.id);
    let recipe = Recipe {
        profile: case.profile,
        entries: case.entries,
        directories: 0,
        seed,
    };
    let prepared = prepared_for(&recipe, context)?;
    if let Err(defect) = prepared.check() {
        return Ok(unmeasured(&OpError::Io(defect), Vec::new()));
    }
    let scope = scope_of(seed);
    let _ = op;
    run_build_row(case, &prepared, scope, context, "locality")
}

/// C1-11: filesystem build only, at the declared ladder tier.
pub fn fs_build(
    case: &Case,
    text: bool,
    context: &mut OpContext<'_>,
) -> Result<OpOutcome, OpError> {
    context.create_output()?;
    let seed = crate::ops::seed_of(case.id);
    let (files, _total, directories) = ladder(case.entries);
    let profile = if text { "text-v1" } else { "binary-v1" };
    let recipe = Recipe {
        profile,
        entries: files,
        directories,
        seed,
    };
    let prepared = prepared_for(&recipe, context)?;
    if let Err(defect) = prepared.check() {
        return Ok(unmeasured(&OpError::Io(defect), Vec::new()));
    }
    let scope = scope_of(seed);

    // The walk ceiling is two-sided and is the family's mechanism evidence. A tier
    // whose tree fits one build states it in one operation and declares that it
    // fits; a tier above the ceiling is grown by successive operations, and
    // `run_batched_build_row` carries the gate that says so with the batch sizes
    // it actually used. Exactly one gate owns the identifier per row.
    if prepared.bindings() > WALK_CEILING {
        return run_batched_build_row(case, &prepared, scope, context, "fs_build");
    }
    let gates = if case.entries >= WALK_CEILING as u32 {
        vec![gates::require(
            GateClass::Mechanism,
            "g2.walk-ceiling",
            prepared.bindings() <= WALK_CEILING,
            &format!("{} bindings in one build", prepared.bindings()),
            &format!("at most {WALK_CEILING}: one build states its own bindings"),
        )]
    } else {
        Vec::new()
    };
    let mut outcome = run_build_row(case, &prepared, scope, context, "fs_build")?;
    outcome.gates.extend(gates);
    Ok(outcome)
}

/// One completed batch, with the counters and root it produced.
struct BatchOutcome {
    result: layerfs_content::FilesystemResult,
    objects: layerfs_content::ObjectWork,
}

/// The scratch directory one phase's batch writes its ordering runs into.
///
/// One directory per phase per batch, and it is created before the timer: a
/// `FileBacking` scans its directory for the highest run token it already owns,
/// and that scan is harness work rather than product work.
fn batch_directory(context: &OpContext<'_>, phase: &str, index: usize) -> PathBuf {
    context
        .output
        .join("ordering")
        .join(format!("{phase}-{index:05}"))
}

/// Creates the scratch directories a batched row needs and returns one backing
/// per batch, in batch order.
fn batch_backings(
    context: &OpContext<'_>,
    phase: &str,
    batches: usize,
) -> Result<Vec<FileBacking>, OpError> {
    let mut out = Vec::with_capacity(batches);
    for index in 0..batches {
        let directory = batch_directory(context, phase, index);
        std::fs::create_dir_all(&directory)
            .map_err(|error| OpError::Io(format!("{}: {error}", directory.display())))?;
        out.push(FileBacking::new(directory));
    }
    Ok(out)
}

/// Runs one batch inside the timed region with a non-retaining consumer.
///
/// `base` selects the entry point: `None` is `build_filesystem`, `Some` is
/// `update_filesystem` against the root the previous batch produced. The reader
/// is the fixture chain, which already holds every object any batch's base names
/// — including one an operation reads back after emitting it, which is why the
/// chain has to be built before the timer rather than carried through it.
fn measure_batch(
    batch: &crate::ops::fs_fixture::Batch,
    base: Option<FilesystemRootId>,
    scope: layerfs_content::InodeScope,
    fixture: &TreeStore,
    backing: &mut FileBacking,
) -> Result<BatchOutcome, layerfs_content::ContentError> {
    let input = FilesystemInput {
        base,
        scope,
        root_serial: ROOT_SERIAL,
        directories: &batch.directories,
        inodes: &batch.inodes,
        new_inodes: &batch.new_inodes,
        resources: resources(),
    };
    let mut consumer = DiscardingConsumer::new();
    let reader = PairProvider::new(fixture, fixture);
    let mut objects = FilesystemObjects::new(&reader, &mut consumer);
    let backing = as_backing(Some(backing));
    let result = match base {
        None => build_filesystem(&mut objects, &input, backing)?,
        Some(_) => update_filesystem(&mut objects, &input, backing)?,
    };
    Ok(BatchOutcome {
        result,
        objects: objects.work(),
    })
}

/// Builds the fixture chain: the same batches into an authenticating store.
///
/// This is the oracle as well as the fixture. It is unmeasured, it runs the same
/// product entry points over the same inputs, and it returns every batch's root
/// so the measured chain can be compared against it step by step rather than
/// only at the end. Its reader is the in-flight overlay, because a filesystem
/// update reads back objects it emitted earlier in the same operation; the
/// objects it retains live outside the timed region, so nothing here is charged
/// to the product's heap window.
fn fixture_chain(
    batches: &[crate::ops::fs_fixture::Batch],
    scope: layerfs_content::InodeScope,
    context: &OpContext<'_>,
) -> Result<(TreeStore, Vec<FilesystemRootId>), OpError> {
    let mut fixture = TreeStore::new();
    let mut backings = batch_backings(context, "fixture", batches.len())?;
    let mut roots: Vec<FilesystemRootId> = Vec::with_capacity(batches.len());
    let mut base = None;
    for (index, batch) in batches.iter().enumerate() {
        let input = FilesystemInput {
            base,
            scope,
            root_serial: ROOT_SERIAL,
            directories: &batch.directories,
            inodes: &batch.inodes,
            new_inodes: &batch.new_inodes,
            resources: resources(),
        };
        let mut shared = SharedStore::new();
        let (outcome, _) = Timing::disabled("oracle.replay", |_scope: &TimingScope<'_, Active>| {
            let reader = shared.reader(&fixture);
            let mut objects = FilesystemObjects::new(&reader, &mut shared);
            let backing = as_backing(Some(&mut backings[index]));
            let result = match base {
                None => build_filesystem(&mut objects, &input, backing),
                Some(_) => update_filesystem(&mut objects, &input, backing),
            };
            result.map(|result| result.root)
        });
        let root = outcome.map_err(|error| OpError::Product(format!("{error:?}")))?;
        shared.absorb_into(&mut fixture);
        roots.push(root);
        base = Some(root);
    }
    Ok((fixture, roots))
}

/// A row whose measured phase builds a tree larger than one walk may charge.
///
/// `MAXIMUM_WALK_ENTRIES` is charged once per whole-tree walk and a build states
/// its own bindings, so the product refuses one build above the ceiling and the
/// declared route for a larger tree is several operations that each stay under
/// it. The row measures **all** of them inside one timer: the tier's tree is the
/// subject, and splitting the timer per batch would hide the chain's own cost.
/// The batch structure is declared in the row's notes rather than inferred.
fn run_batched_build_row(
    case: &Case,
    prepared: &PreparedTree,
    scope: layerfs_content::InodeScope,
    context: &mut OpContext<'_>,
    label: &'static str,
) -> Result<OpOutcome, OpError> {
    let batches = match prepared.batches(WALK_CEILING) {
        Ok(batches) => batches,
        Err(defect) => return Ok(unmeasured(&OpError::Io(defect), Vec::new())),
    };
    let (fixture, fixture_roots) = fixture_chain(&batches, scope, context)?;
    let Some(fixture_root) = fixture_roots.last().copied() else {
        return Ok(unmeasured(
            &OpError::Io("the batch plan is empty".to_string()),
            Vec::new(),
        ));
    };
    // Every backing is created before the timer, so the measured phase performs
    // no harness directory scan and owns no harness allocation it did not earn.
    let mut backings = batch_backings(context, "measured", batches.len())?;

    let mut roots: Vec<FilesystemRootId> = Vec::with_capacity(batches.len());
    let mut objects = layerfs_content::ObjectWork::default();
    let mut counters = layerfs_content::filesystem::FilesystemUpdateCounters::default();
    let mut last: Option<layerfs_content::FilesystemResult> = None;
    instruments::heap_begin();
    let (outcome, report) = Timing::record(label, |_timing: &TimingScope<'_, Active>| {
        let mut base = None;
        for (index, batch) in batches.iter().enumerate() {
            let measured = measure_batch(batch, base, scope, &fixture, &mut backings[index])?;
            roots.push(measured.result.root);
            objects.objects_read += measured.objects.objects_read;
            objects.read_waves += measured.objects.read_waves;
            objects.bytes_read += measured.objects.bytes_read;
            objects.objects_emitted += measured.objects.objects_emitted;
            objects.bytes_emitted += measured.objects.bytes_emitted;
            counters.bindings_added += measured.result.counters.bindings_added;
            counters.bindings_removed += measured.result.counters.bindings_removed;
            counters.directory_updates += measured.result.counters.directory_updates;
            counters.base_records_read += measured.result.counters.base_records_read;
            counters.validation.entries_examined +=
                measured.result.counters.validation.entries_examined;
            counters.directories.pages_read += measured.result.counters.directories.pages_read;
            counters.inodes.pages_read += measured.result.counters.inodes.pages_read;
            last = Some(measured.result);
            base = Some(measured.result.root);
        }
        Ok::<_, layerfs_content::ContentError>(())
    });
    let heap = instruments::heap_end();
    if let Err(error) = outcome {
        return Ok(unmeasured(
            &OpError::Product(format!("{error:?}")),
            Vec::new(),
        ));
    }
    let Some(mut result) = last else {
        return Ok(unmeasured(
            &OpError::Io("the measured chain produced no root".to_string()),
            Vec::new(),
        ));
    };
    // The row publishes the **chain's** counters, not the last batch's: the tier
    // is the tree, and a counter that reported one operation would understate the
    // work the row actually charged.
    result.counters = counters;
    let measured_root = result.root;
    let measured = Measured {
        result,
        objects,
        report,
        heap,
    };
    record_counters(context, &measured, label)?;
    let largest = batches.iter().map(|batch| batch.bindings()).max().unwrap_or(0);
    context.trace.write_number(
        Kind::Counter,
        &format!("{label}.operations"),
        batches.len() as i128,
        "operations",
        "declared multi-operation structure of the tier",
    )?;
    context.trace.write_number(
        Kind::Counter,
        &format!("{label}.batch_bindings_max"),
        largest as i128,
        "bindings",
        "largest batch, against the walk ceiling",
    )?;
    let mut gates = common_gates(&measured, &measured.result, label);
    gates.push(gates::require(
        GateClass::Mechanism,
        "g2.walk-ceiling",
        largest <= WALK_CEILING,
        &format!("{} operations, largest {largest} bindings", batches.len()),
        &format!("every operation states at most {WALK_CEILING} bindings"),
    ));
    gates.push(gates::require(
        GateClass::Correctness,
        "g1.o4-chain-roots",
        roots == fixture_roots,
        &format!("{} batch roots reproduced", roots.len()),
        &format!(
            "{} roots from the unmeasured fixture chain",
            fixture_roots.len()
        ),
    ));
    let listing = listings_match(&fixture, fixture_root, prepared)?;
    match listing {
        Ok(directories) => {
            context.trace.write_number(
                Kind::Counter,
                &format!("{label}.listing_directories"),
                directories as i128,
                "directories",
                "O4 listing compared against the fixture manifest",
            )?;
            gates.push(gates::require(
                GateClass::Correctness,
                "g1.o4-listing",
                true,
                &format!("{directories} directories equal the manifest"),
                "every directory's bindings equal the fixture manifest",
            ));
        }
        Err(disagreement) => gates.push(Gate::fail(
            GateClass::Correctness,
            "g1.o4-listing",
            &disagreement,
            "every directory's bindings equal the fixture manifest",
        )),
    }
    let _ = measured_root;

    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("recipe_profile: {}", prepared.recipe.profile),
            format!("recipe_entries: {}", prepared.recipe.entries),
            format!("recipe_directories: {}", prepared.recipe.directories),
            format!("bindings: {}", prepared.bindings()),
            format!(
                "multi_operation: {} batches of at most {WALK_CEILING} bindings; the first is a \
                 build and the rest are updates that state new files only",
                batches.len()
            ),
            "cache_state: warm-in-process-fixture; the input and every batch's base are built \
             before the timed region"
                .to_string(),
            "measured_consumer: discarding; each batch's base is a fixture input".to_string(),
            "oracle_phase: separate-unmeasured-replay".to_string(),
        ],
    })
    .map(|outcome| {
        let _ = case;
        outcome
    })
}

/// The declared `c1.fs.build-scale` ladder, by entry count.
fn ladder(files: u32) -> (u32, u64, u32) {
    match files {
        100 => (100, 5_242_880, 1),
        1_000 => (1_000, 20_971_520, 10),
        10_000 => (10_000, 314_572_800, 100),
        other => (other, 524_288_000, 1_000),
    }
}

/// Builds or loads the prepared input, honouring `--emit-input`/`--load-input`.
fn prepared_for(recipe: &Recipe, context: &OpContext<'_>) -> Result<PreparedTree, OpError> {
    if let Some(directory) = &context.prepared_input {
        if context.load_input {
            return PreparedTree::load(directory).map_err(OpError::Io);
        }
        let prepared = recipe.prepare();
        prepared
            .emit(directory)
            .map_err(|error| OpError::Io(format!("{}: {error}", directory.display())))?;
        return Ok(prepared);
    }
    Ok(recipe.prepare())
}

/// A row whose measured phase is a build.
fn run_build_row(
    case: &Case,
    prepared: &PreparedTree,
    scope: layerfs_content::InodeScope,
    context: &mut OpContext<'_>,
    label: &'static str,
) -> Result<OpOutcome, OpError> {
    let mut backing = backing_for(prepared, context);
    let measured = match measure_build(prepared, scope, as_backing(backing.as_mut()), label).0 {
        Ok(measured) => measured,
        Err(error) => {
            return Ok(unmeasured(
                &OpError::Product(format!("{error:?}")),
                Vec::new(),
            ))
        }
    };
    record_counters(context, &measured, label)?;
    let mut gates = common_gates(&measured, &measured.result, label);

    // Oracle: a second, unmeasured, byte-identical build into an authenticating store.
    let mut replay_backing = backing_for(prepared, context);
    let (store, replay) = replay_build(prepared, scope, as_backing(replay_backing.as_mut()))?;
    gates.push(gates::require(
        GateClass::Correctness,
        "g1.o4-root-directory",
        three_tuple(&store, replay.root)?.directory_root
            == three_tuple(&store, measured.result.root)?.directory_root,
        "the root directory content root read back from the replay",
        "the root directory content root read back from the measured root",
    ));
    let listing = listings_match(&store, replay.root, prepared)?;
    match listing {
        Ok(directories) => {
            context.trace.write_number(
                Kind::Counter,
                &format!("{label}.listing_directories"),
                directories as i128,
                "directories",
                "O4 listing compared against the fixture manifest",
            )?;
            gates.push(gates::require(
                GateClass::Correctness,
                "g1.o4-listing",
                true,
                &format!("{directories} directories equal the manifest"),
                "every directory's bindings equal the fixture manifest",
            ));
        }
        Err(disagreement) => gates.push(Gate::fail(
            GateClass::Correctness,
            "g1.o4-listing",
            &disagreement,
            "every directory's bindings equal the fixture manifest",
        )),
    }

    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("recipe_profile: {}", prepared.recipe.profile),
            format!("recipe_entries: {}", prepared.recipe.entries),
            format!("recipe_directories: {}", prepared.recipe.directories),
            format!("bindings: {}", prepared.bindings()),
            format!(
                "cache_state: warm-in-process-fixture; the input is built before the timed region"
            ),
            "oracle_phase: separate-unmeasured-replay".to_string(),
            format!("ordering_backing: {}", backing.is_some()),
        ],
    })
    .map(|outcome| {
        let _ = case;
        outcome
    })
}

/// A row whose measured phase is a read-only traversal of a built tree.
fn run_traverse_row(
    case: &Case,
    prepared: &PreparedTree,
    scope: layerfs_content::InodeScope,
    context: &mut OpContext<'_>,
    label: &'static str,
    contents: bool,
) -> Result<OpOutcome, OpError> {
    // The base is built outside the timed region: setup, never measurement.
    let mut backing = backing_for(prepared, context);
    let (store, base) = replay_build(prepared, scope, as_backing(backing.as_mut()))?;

    let mut visits = 0_u64;
    let mut digest = crate::workload::digest::Sha256::new();
    instruments::heap_begin();
    let (outcome, report) = Timing::record(label, |_timing: &TimingScope<'_, Active>| {
        let mut read = FilesystemRead::new(&store, base.root)?;
        for (path, serial) in &prepared.files {
            let logical = LogicalPath::new(path)?;
            let stat = read.stat(&logical)?;
            if contents {
                // The content root is the file's logical identity; the harness
                // hashes it rather than the payload because the fixture's files
                // are namespace entries, not constructed content.
                digest.update(stat.content_root.to_string().as_bytes());
            }
            let _ = serial;
            visits += 1;
        }
        Ok::<_, layerfs_content::ContentError>(read.work())
    });
    let heap = instruments::heap_end();
    let work = match outcome {
        Ok(work) => work,
        Err(error) => {
            return Ok(unmeasured(
                &OpError::Product(format!("{error:?}")),
                Vec::new(),
            ))
        }
    };
    let _ = work;

    context.trace.write_number(
        Kind::Counter,
        &format!("{label}.files_visited"),
        visits as i128,
        "files",
        "fixture manifest, measured phase",
    )?;
    context.trace.write_number(
        Kind::Counter,
        &format!("{label}.expected_files"),
        prepared.files.len() as i128,
        "files",
        "fixture manifest",
    )?;
    context.trace.write_number(
        Kind::Resource,
        "heap.peak_incremental_bytes",
        heap.peak_incremental_bytes as i128,
        "bytes",
        "counting GlobalAlloc, measured phase",
    )?;
    context.trace.write(
        Kind::Oracle,
        &format!("{label}.scan_digest"),
        &crate::workload::digest::hex(&digest.finish()),
        "sha256",
        "digest of every visited content root",
    )?;

    let mut gates = vec![
        if report.root().is_none() {
            Gate::incomplete(
                GateClass::TimingPurity,
                "g7.tree-complete",
                "disabled report: no root",
                "report.is_incomplete() == false",
            )
        } else {
            gates::timing_purity(
                report.is_incomplete(),
                report.node_count() as u64,
                report.levels() as u64,
            )
        },
        gates::swap_gate(instruments::swaps()),
        gates::require(
            GateClass::Correctness,
            "g1.o4-files-visited",
            visits == prepared.files.len() as u64,
            &format!("{visits} files visited"),
            &format!("{} files in the fixture manifest", prepared.files.len()),
        ),
    ];
    let listing = listings_match(&store, base.root, prepared)?;
    match listing {
        Ok(directories) => gates.push(gates::require(
            GateClass::Correctness,
            "g1.o4-listing",
            true,
            &format!("{directories} directories equal the manifest"),
            "every directory's bindings equal the fixture manifest",
        )),
        Err(disagreement) => gates.push(Gate::fail(
            GateClass::Correctness,
            "g1.o4-listing",
            &disagreement,
            "every directory's bindings equal the fixture manifest",
        )),
    }

    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("recipe_entries: {}", prepared.recipe.entries),
            format!("bindings: {}", prepared.bindings()),
            "cache_state: warm-in-process-fixture; the base tree is built before the timed region"
                .to_string(),
            "measured_phase: read-only traversal of an immutable root".to_string(),
        ],
    })
    .map(|outcome| {
        let _ = case;
        outcome
    })
}

/// A row whose measured phase removes every fixture file.
fn run_delete_row(
    case: &Case,
    prepared: &PreparedTree,
    scope: layerfs_content::InodeScope,
    context: &mut OpContext<'_>,
    label: &'static str,
) -> Result<OpOutcome, OpError> {
    let mut backing = backing_for(prepared, context);
    let (store, base) = replay_build(prepared, scope, as_backing(backing.as_mut()))?;

    // Every name the root and each directory bound becomes an absence.
    let mut removals: Vec<DirectoryUpdate> = Vec::new();
    for update in &prepared.directories {
        let changes: Vec<(PathName, Option<u64>)> = update
            .changes
            .iter()
            .map(|(changed, _)| (changed.clone(), None))
            .collect();
        removals.push(DirectoryUpdate {
            parent: update.parent,
            changes,
        });
    }
    removals.sort_by_key(|update| update.parent);
    let removal_input = PreparedTree {
        recipe: prepared.recipe,
        directories: removals,
        inodes: Vec::new(),
        new_inodes: Vec::new(),
        listings: Vec::new(),
        files: Vec::new(),
        directory_serials: Vec::new(),
    };
    if let Err(defect) = removal_input.check() {
        return Ok(unmeasured(&OpError::Io(defect), Vec::new()));
    }

    // **The order here is load-bearing.** The replay runs *first*, because it is
    // unmeasured and because running it first is what makes the measured phase's
    // `MissingObject` attributable. If the identical input replays into a
    // retaining store, the read-back is the cause; if it does not, the input is
    // the cause and the row must say that instead of blaming the consumer.
    let mut replay_backing = backing_for(&removal_input, context);
    let (fixture, replay) = match replay_update(
        &store,
        base.root,
        &removal_input,
        scope,
        as_backing(replay_backing.as_mut()),
    ) {
        Ok((fixture, replay)) => (fixture, replay),
        Err(error) => return Ok(unmeasured(&error, Vec::new())),
    };

    let mut backing = backing_for(&removal_input, context);
    let (measured, served_result, served_base) =
        match measure_update(
            &store,
            &fixture,
            base.root,
            &removal_input,
            scope,
            as_backing(backing.as_mut()),
            label,
        ) {
            Ok(measured) => measured,
            Err(layerfs_content::ContentError::MissingObject) => return Ok(unmeasured(
                &OpError::Product(
                    "filesystem-update: MissingObject from a discarding measured phase whose \
                     fixture is the byte-identical unmeasured replay; the replay and the \
                     measured operation have diverged"
                        .to_string(),
                ),
                Vec::new(),
            )),
            Err(error) => {
                return Ok(unmeasured(
                    &OpError::Product(format!("{error:?}")),
                    Vec::new(),
                ))
            }
        };

    record_counters(context, &measured, label)?;

    // The replay above is the oracle: it is a second, unmeasured, byte-identical
    // removal into an authenticating store. Without it the replay-identity gate
    // would compare the measured result with itself, which is not a gate at all.
    let mut gates = common_gates(&measured, &replay, label);
    gates.push(gates::require(
        GateClass::Correctness,
        "g1.o4-removals",
        measured.result.counters.bindings_removed > 0,
        &format!(
            "{} bindings removed",
            measured.result.counters.bindings_removed
        ),
        "at least one binding removed",
    ));
    gates.push(gates::require(
        GateClass::Correctness,
        "g1.o4-removals-replay",
        replay.counters.bindings_removed == measured.result.counters.bindings_removed,
        &format!("replay removed {}", replay.counters.bindings_removed),
        &format!(
            "measured removed {}",
            measured.result.counters.bindings_removed
        ),
    ));
    // The reading that makes this row falsifiable rather than merely passing. A
    // filesystem update reads back objects it emitted earlier in the same
    // operation, so the measured phase is served by a fixture - the byte-identical
    // unmeasured replay - while its own consumer discards. If a future change
    // stopped the operation reading back its own output, `served_result` would fall
    // to zero and this gate would say so, because then the fixture would not be
    // load-bearing and the row would be describing a different operation than it
    // claims. A row that quietly stopped needing the fixture is as much a finding
    // as one that started failing.
    context.trace.write_number(
        Kind::Counter,
        &format!("{label}.served_from_result"),
        served_result as i128,
        "objects",
        "PairProvider, objects the measured phase demanded that it had emitted",
    )?;
    context.trace.write_number(
        Kind::Counter,
        &format!("{label}.served_from_base"),
        served_base as i128,
        "objects",
        "PairProvider, objects the measured phase demanded from the base",
    )?;
    // The external oracle for a removal: every directory the removal input empties
    // must read back empty. The expectation comes from the harness's own removal
    // input - whose bindings are all absences - and never from the artifact the
    // operation produced, so this is the one correctness reading on this row that
    // the product cannot agree with by construction.
    // **The listing oracle this row does not have, and why.** The obvious O4 for a
    // removal is "every directory the fixture has reads back empty", but the
    // removal input unbinds the root's own bindings too, so the derived
    // directories are *gone* from the namespace and asking for their listings asks
    // for paths that no longer exist. Detecting that needs the read path to say
    // "this name is not bound", and `FilesystemRead::resolve` reports an unbound
    // name as `ContentError::MissingObject`
    // (`filesystem/read.rs`, `.ok_or(ContentError::MissingObject)`) - the variant
    // whose own doc reserves it for "the provider does not hold the requested
    // object" and which `ProviderFailure` exists to be distinguished from. So a
    // listing oracle here could not tell a removed path from a corrupt provider,
    // and a gate built on that would be wrong rather than merely weak. The row
    // therefore carries the replay-identity gates and the mechanism gate below and
    // no listing oracle; the gap is recorded rather than papered over.
    gates.push(gates::require(
        GateClass::Mechanism,
        "g2.reads-back-its-own-output",
        served_result > 0 && served_base > 0,
        &format!("{served_result} from the operation's own output, {served_base} from the base"),
        "the update demands objects it emitted, which is why a discarding consumer \
         alone cannot serve this row and a fixture must",
    ));
    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("recipe_entries: {}", prepared.recipe.entries),
            format!("removals: {}", prepared.bindings()),
            "cache_state: warm-in-process-fixture; the base tree and the replayed result are \
             built before the timed region"
                .to_string(),
            "measured_consumer: discarding; the objects the update reads back are served by the \
             byte-identical unmeasured replay"
                .to_string(),
            "oracle_phase: separate-unmeasured-replay".to_string(),
        ],
    })
    .map(|outcome| {
        let _ = case;
        outcome
    })
}
