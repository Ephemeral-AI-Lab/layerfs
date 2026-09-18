//! `pipeline.*`: the integrated C1 to C2 handoff.
//!
//! The four rows are the only place in the registry where the C1 and C2 halves
//! run **in one region**. `test_setup_and_cache_discipline.md` section 2.2 fixes
//! what is inside the timer: `update_filesystem` (or `apply_edits`) plus
//! `Store::open` plus the save plus its acknowledgement. C1 construction of the
//! base is setup; the measured C1 half is the operation that edits it.
//!
//! **The handoff is the product's own adapter.** `SaveHandoff` is
//! `layerfs-storage`'s "adapter that lets C1 feed a save operation directly", so
//! every object C1 emits reaches the save operation on the path the product
//! provides and nothing is retained by the harness inside the heap window. The
//! count of what crossed is taken by `CountingConsumer` on that same path, which
//! is why `g2.handoff` compares the objects C1 emitted with the objects the save
//! acknowledged rather than with a number the save reports about itself.
//!
//! **The base is read from the Store, not from harness memory.** A pipeline row
//! that read its base from a `TreeStore` would prove nothing about the handoff in
//! the read direction. The reader is `StoreProvider` over the sample Store, so
//! the measured region genuinely goes C1 -> C2 for the base and C1 -> C2 for the
//! result.

use layerfs_content::filesystem::{
    build_filesystem, scope_for_seed, FilesystemInput, FilesystemObjects, FilesystemResources,
};
use layerfs_content::{
    apply_edits, ConstructionPolicy, Edit, EditRequest, EditStream, ObjectId, Replacements,
};
use layerfs_storage::{SaveHandoff, Store, StoreProvider};
use layerfs_telemetry::timer::{Active, Timing, TimingScope};

use super::c1;
use super::c2;
use super::fs;
use super::fs_fixture::{PreparedTree, Recipe, ROOT_SERIAL};
use super::{seed_of, OpContext, OpError, OpOutcome};
use crate::fixture::{self, structured};
use crate::gates::{self, Gate, GateClass};
use crate::registry::{Case, PipelineOp};
use crate::support::instruments;
use crate::support::trace::Kind;
use crate::workload::oracle::Expectation;
use crate::workload::providers::{CountingConsumer, PairProvider, TreeStore};

/// The 4 KiB replacement the edit pipeline rows use, as the C1 edit families do.
pub const REPLACEMENT_LEN: u64 = 4_096;

/// The representation cutoff the `large-to-small` row crosses.
///
/// Read from the frozen policy rather than restated: the row's whole point is
/// that the result lands on the other side of the product's own boundary.
fn cutoff() -> u64 {
    ConstructionPolicy::frozen_default().small_file_threshold_bytes()
}

/// What one fixed pipeline row declares.
///
/// The registry fixes four rows and no tier, so the configuration is declared
/// here, once, exactly as `c2_pool.rs` declares its leaf and row counts.
pub struct Configuration {
    /// Base bytes the row constructs and stores before the timer.
    pub base_bytes: u64,
    /// First byte the measured edit rewrites.
    pub start: u64,
    /// Byte the measured edit stops rewriting at.
    pub end: u64,
    /// Replacement bytes offered for the rewritten range.
    pub replacement: u64,
    /// Files the filesystem row builds.
    pub files: u32,
    /// Directories the filesystem row builds.
    pub directories: u32,
}

/// The declared configuration of one pipeline row.
pub fn configuration(op: PipelineOp) -> Configuration {
    match op {
        PipelineOp::EditsSmall => Configuration {
            base_bytes: 1 << 20,
            start: (1 << 20) / 2,
            end: (1 << 20) / 2 + REPLACEMENT_LEN,
            replacement: REPLACEMENT_LEN,
            files: 0,
            directories: 0,
        },
        PipelineOp::EditsChunked => Configuration {
            base_bytes: 10 << 20,
            start: (10 << 20) / 2,
            end: (10 << 20) / 2 + REPLACEMENT_LEN,
            replacement: REPLACEMENT_LEN,
            files: 0,
            directories: 0,
        },
        PipelineOp::EditsLargeToSmall => Configuration {
            base_bytes: 1 << 20,
            // Everything from the cutoff on is removed, so the result is
            // `cutoff - 1` bytes: below the boundary, and therefore whole-file
            // where the base was chunked.
            start: cutoff() - 1,
            end: 1 << 20,
            replacement: 0,
            files: 0,
            directories: 0,
        },
        PipelineOp::FilesystemBuild => Configuration {
            base_bytes: 0,
            start: 0,
            end: 0,
            replacement: 0,
            files: 1_000,
            directories: 10,
        },
    }
}

/// Runs the one registered row.
pub fn run(
    case: &Case,
    op: PipelineOp,
    context: &mut OpContext<'_>,
) -> Result<OpOutcome, OpError> {
    context.create_output()?;
    match op {
        PipelineOp::FilesystemBuild => filesystem(case, op, context),
        _ => edit(case, op, context),
    }
}

/// The C1 half of an edit pipeline row: the base, built and stored before the
/// timer, plus the edit request the measured phase applies.
struct EditSetup {
    base_store: TreeStore,
    base_root: ObjectId,
    stream: EditStream,
    source: Replacements,
    expectation: Expectation,
    base_bytes: u64,
}

fn edit_setup(case: &Case, config: &Configuration) -> Result<EditSetup, OpError> {
    let seed = seed_of(case.id);
    let policy = ConstructionPolicy::frozen_default();
    let capacities = policy.capacities();
    let bytes = structured(config.base_bytes, seed, true);
    let mut base_store = TreeStore::new();
    let base_file = c1::build_base(policy, &capacities, &bytes, &mut base_store)?;
    let stream = EditStream::new(
        bytes.len() as u64,
        vec![Edit::new(config.start, config.end, config.replacement)],
    )
    .map_err(|error| OpError::Product(format!("{error:?}")))?;
    let replacement = if config.replacement > 0 {
        fixture::noise(config.replacement, seed ^ 0x2222)
    } else {
        Vec::new()
    };
    let mut source = Replacements::new();
    if config.replacement > 0 {
        source.push(replacement.clone());
    }
    let expectation = Expectation::spliced(&bytes, config.start, config.end, &replacement);
    Ok(EditSetup {
        base_store,
        base_root: base_file.root,
        stream,
        source,
        expectation,
        base_bytes: bytes.len() as u64,
    })
}

/// One integrated edit row: `Store::open` + `apply_edits` + save + ack.
fn edit(case: &Case, op: PipelineOp, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    let config = configuration(op);
    let setup = edit_setup(case, &config)?;
    let policy = ConstructionPolicy::frozen_default();
    let capacities = policy.capacities();

    let base = context.output.join("base.sqlite");
    let sample = context.output.join("sample.sqlite");
    c2::create_and_save_untimed(&base, &setup.base_store)?;
    let de_warm = c2::prepare_sample(&base, &sample)?;
    let mut gates = vec![
        gates::residency_gate(Some(de_warm.resident_after)),
        gates::attribution_gate(gates::Attribution::Exclusive),
    ];

    let mut emitted = 0_u64;
    instruments::heap_begin();
    let (measured, report) = Timing::record("pipeline", |scope: &TimingScope<'_, Active>| {
        let store = Store::open(&sample, scope.child("store.open"))?;
        let provider = StoreProvider::new(&store);
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        let file = {
            let mut handoff = SaveHandoff::new(&mut operation);
            let mut counting = CountingConsumer::new(&mut handoff);
            let request = EditRequest {
                root: setup.base_root,
                edits: &setup.stream,
                source: &setup.source,
            };
            let file = apply_edits(
                policy,
                &capacities,
                &provider,
                request,
                &mut counting,
                scope.child("edit"),
            )?;
            emitted = counting.accepted();
            if let Some(error) = handoff.take_failure() {
                return Err(PipelineFailure::Storage(error));
            }
            file
        };
        let outcome = operation.finish(scope.child("storage.finish"))?;
        Ok::<_, PipelineFailure>((file, outcome))
    });
    let heap = instruments::heap_end();
    let timing_bytes = c1::write_timing(context.output, &report)?;
    let (file, outcome) = match measured {
        Ok(value) => value,
        Err(error) => {
            return Ok(c1::unmeasured(
                &OpError::Product(format!("{error:?}")),
                gates,
            ))
        }
    };

    // Oracle: a second, unmeasured, byte-identical edit into an authenticating
    // store. The base is the same base store the master was saved from, so the
    // replay is a replay of the measured operation and not of a different one.
    let mut result_store = TreeStore::new();
    let (replay, _) = Timing::disabled("oracle.replay", |scope: &TimingScope<'_, Active>| {
        let request = EditRequest {
            root: setup.base_root,
            edits: &setup.stream,
            source: &setup.source,
        };
        apply_edits(
            policy,
            &capacities,
            &setup.base_store,
            request,
            &mut result_store,
            scope.child("edit"),
        )
    });
    let replay = match replay {
        Ok(file) => file,
        Err(error) => {
            return Ok(c1::unmeasured(
                &OpError::Product(format!("{error:?}")),
                gates,
            ))
        }
    };

    context.trace.write_number(Kind::Counter, "pipeline.base_bytes", setup.base_bytes as i128, "bytes", "fixture recipe")?;
    context.trace.write_number(Kind::Counter, "pipeline.final_bytes", file.logical_len as i128, "bytes", "ConstructedFile.logical_len")?;
    context.trace.write_number(Kind::Counter, "pipeline.expected_final_bytes", setup.expectation.logical_len as i128, "bytes", "base - removed + replacement, computed by the oracle")?;
    context.trace.write_number(Kind::Counter, "pipeline.handoff_objects", emitted as i128, "objects", "CountingConsumer on the SaveHandoff path, measured phase")?;
    context.trace.write_number(Kind::Counter, "pipeline.inserted", i128::from(outcome.inserted), "objects", "SaveOutcome.inserted")?;
    context.trace.write_number(Kind::Counter, "pipeline.reused", i128::from(outcome.reused), "objects", "SaveOutcome.reused")?;
    context.trace.write_number(Kind::Counter, "pipeline.commits", i128::from(outcome.commits), "transactions", "SaveOutcome.commits")?;
    context.trace.write_number(Kind::Counter, "pipeline.statements", i128::from(outcome.statements), "statements", "SaveOutcome.statements")?;
    context.trace.write_number(Kind::Counter, "pipeline.nodes_read", file.counters.nodes_read as i128, "nodes", "EditCounters.nodes_read")?;
    context.trace.write_number(Kind::Counter, "pipeline.nodes_created", file.counters.nodes_created as i128, "nodes", "EditCounters.nodes_created")?;
    context.trace.write_number(Kind::Counter, "timing_json_bytes", timing_bytes as i128, "bytes", "product timing.json, byte-verbatim")?;
    context.trace.write_number(Kind::Resource, "heap.peak_incremental_bytes", heap.peak_incremental_bytes as i128, "bytes", "counting GlobalAlloc, measured phase")?;

    gates.push(c1::completeness_gate(&report, "g7.tree-complete"));
    gates.push(gates::require(
        GateClass::Correctness,
        "g1.o1-replay-root",
        replay.root == file.root,
        &format!("replay root {}", replay.root),
        &format!("measured root {}", file.root),
    ));
    gates.push(gates::require(
        GateClass::Correctness,
        "g1.o2-length-equation",
        file.logical_len == setup.expectation.logical_len,
        &format!("{} bytes", file.logical_len),
        &format!("{} bytes = base - removed + replacement", setup.expectation.logical_len),
    ));
    gates.push(gates::require(
        GateClass::Mechanism,
        "g2.handoff",
        emitted > 0 && u64::from(outcome.inserted) + u64::from(outcome.reused) == emitted,
        &format!(
            "{emitted} objects crossed the handoff; save acknowledged {} inserted + {} reused",
            outcome.inserted, outcome.reused
        ),
        "every object C1 emitted reached the save operation and the save acknowledged each one",
    ));

    // O2: the result read back through the Store's own read path and compared
    // with the oracle's expectation. This is the gate that says the handoff wrote
    // a file the product can read, not merely one it accepted.
    let store = match c2::open_untimed(&sample) {
        Ok(store) => store,
        Err(error) => return Ok(c1::unmeasured(&error, gates)),
    };
    let back = c2::read_back_through_store(&store, file.root, &setup.expectation);
    match back {
        Ok(back) => gates.push(gates::require(
            GateClass::Correctness,
            "g1.o2-readback",
            back.matches(),
            &format!("{} bytes, {}", back.bytes, back.digest),
            &format!("{} bytes, {}", back.expected_bytes, back.expected_digest),
        )),
        Err(error) => gates.push(Gate::fail(
            GateClass::Correctness,
            "g1.o2-readback",
            &format!("{error}"),
            "the edited file reads back through the Store byte-exact",
        )),
    }
    gates.extend(c2::sidecar_gates(&sample));
    gates.push(gates::swap_gate(instruments::swaps()));

    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("pipeline_op: {op:?}"),
            format!("declared_base_bytes: {}", config.base_bytes),
            format!("declared_edit: {}..{} -> {} bytes", config.start, config.end, config.replacement),
            format!("cutoff_bytes: {}", cutoff()),
            "measured_region: Store::open + apply_edits + save + acknowledgement".to_string(),
            "base_reader: StoreProvider over the sample copy".to_string(),
            "handoff: layerfs_storage::SaveHandoff, the product's own C1-to-save adapter".to_string(),
            format!("store_state: opened-from-copy"),
            format!("copy_rung: {}", c2::COPY_RUNG),
            format!("allocation_attribution: {}", c2::ATTRIBUTION),
            format!("heap_charged_bytes: {}", heap.charged_bytes),
        ],
    })
}

/// The two failures an integrated row can end on.
#[derive(Debug)]
enum PipelineFailure {
    Content(layerfs_content::ContentError),
    Storage(layerfs_storage::StorageError),
}

impl From<layerfs_content::ContentError> for PipelineFailure {
    fn from(error: layerfs_content::ContentError) -> Self {
        Self::Content(error)
    }
}

impl From<layerfs_storage::StorageError> for PipelineFailure {
    fn from(error: layerfs_storage::StorageError) -> Self {
        Self::Storage(error)
    }
}

impl std::fmt::Display for PipelineFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Content(error) => write!(formatter, "{error:?}"),
            Self::Storage(error) => write!(formatter, "{error:?}"),
        }
    }
}

/// One integrated filesystem row: build a tree and save every emitted object.
fn filesystem(case: &Case, op: PipelineOp, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    let config = configuration(op);
    let seed = seed_of(case.id);
    let prepared: PreparedTree = Recipe {
        profile: "binary-v1",
        entries: config.files,
        directories: config.directories,
        seed,
    }
    .prepare();
    if let Err(defect) = prepared.check() {
        return Ok(c1::unmeasured(&OpError::Io(defect), Vec::new()));
    }
    let scope = scope_for_seed({
        let mut bytes = [0_u8; 32];
        bytes[..8].copy_from_slice(&seed.to_le_bytes());
        bytes[8..16].copy_from_slice(&seed.rotate_left(17).to_le_bytes());
        bytes[16..24].copy_from_slice(&seed.rotate_left(31).to_le_bytes());
        bytes[24..32].copy_from_slice(&seed.rotate_left(47).to_le_bytes());
        bytes
    });

    let base = context.output.join("base.sqlite");
    let sample = context.output.join("sample.sqlite");
    c2::create_and_save_untimed(&base, &TreeStore::new())?;
    let de_warm = c2::prepare_sample(&base, &sample)?;
    let mut gates = vec![
        gates::residency_gate(Some(de_warm.resident_after)),
        gates::attribution_gate(gates::Attribution::Exclusive),
    ];

    let input = FilesystemInput {
        base: None,
        scope,
        root_serial: ROOT_SERIAL,
        directories: &prepared.directories,
        inodes: &prepared.inodes,
        new_inodes: &prepared.new_inodes,
        resources: FilesystemResources::default(),
    };
    let empty = TreeStore::new();
    let mut emitted = 0_u64;
    instruments::heap_begin();
    let (measured, report) = Timing::record("pipeline", |timing: &TimingScope<'_, Active>| {
        let store = Store::open(&sample, timing.child("store.open"))?;
        let mut operation = store.begin_save(timing.child("storage.begin"))?;
        let result = {
            let mut handoff = SaveHandoff::new(&mut operation);
            let mut counting = CountingConsumer::new(&mut handoff);
            let reader = PairProvider::new(&empty, &empty);
            let mut objects = FilesystemObjects::new(&reader, &mut counting);
            let result = build_filesystem(&mut objects, &input, None)?;
            emitted = counting.accepted();
            result
        };
        let outcome = operation.finish(timing.child("storage.finish"))?;
        Ok::<_, PipelineFailure>((result, outcome))
    });
    let heap = instruments::heap_end();
    let timing_bytes = c1::write_timing(context.output, &report)?;
    let (result, outcome) = match measured {
        Ok(value) => value,
        Err(error) => {
            return Ok(c1::unmeasured(
                &OpError::Product(format!("{error}")),
                gates,
            ))
        }
    };

    // Oracle: the same build, unmeasured, into an authenticating store.
    let (store, replay) = fs::replay_build(&prepared, scope, None)?;
    let listing = fs::listings_match(&store, replay.root, &prepared)?;

    context.trace.write_number(Kind::Counter, "pipeline.declared_files", config.files as i128, "files", "fixture recipe")?;
    context.trace.write_number(Kind::Counter, "pipeline.declared_directories", config.directories as i128, "directories", "fixture recipe")?;
    context.trace.write_number(Kind::Counter, "pipeline.bindings", prepared.bindings() as i128, "bindings", "fixture manifest")?;
    context.trace.write_number(Kind::Counter, "pipeline.handoff_objects", emitted as i128, "objects", "CountingConsumer on the SaveHandoff path, measured phase")?;
    context.trace.write_number(Kind::Counter, "pipeline.inserted", i128::from(outcome.inserted), "objects", "SaveOutcome.inserted")?;
    context.trace.write_number(Kind::Counter, "pipeline.reused", i128::from(outcome.reused), "objects", "SaveOutcome.reused")?;
    context.trace.write_number(Kind::Counter, "pipeline.commits", i128::from(outcome.commits), "transactions", "SaveOutcome.commits")?;
    context.trace.write_number(Kind::Counter, "pipeline.objects_emitted", result.counters.objects.objects_emitted as i128, "objects", "ObjectWork.objects_emitted")?;
    context.trace.write_number(Kind::Counter, "timing_json_bytes", timing_bytes as i128, "bytes", "product timing.json, byte-verbatim")?;
    context.trace.write_number(Kind::Resource, "heap.peak_incremental_bytes", heap.peak_incremental_bytes as i128, "bytes", "counting GlobalAlloc, measured phase")?;

    gates.push(c1::completeness_gate(&report, "g7.tree-complete"));
    gates.push(gates::require(
        GateClass::Correctness,
        "g1.o1-replay-root",
        replay.root.0 == result.root.0,
        &format!("replay root {}", replay.root.0),
        &format!("measured root {}", result.root.0),
    ));
    gates.push(gates::require(
        GateClass::Mechanism,
        "g2.handoff",
        emitted > 0 && u64::from(outcome.inserted) + u64::from(outcome.reused) == emitted,
        &format!(
            "{emitted} objects crossed the handoff; save acknowledged {} inserted + {} reused",
            outcome.inserted, outcome.reused
        ),
        "every object the build emitted reached the save operation and the save acknowledged each one",
    ));
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
            disagreement.as_str(),
            "every directory's bindings equal the fixture manifest",
        )),
    }

    // O2: the root read back through the Store the measured phase wrote.
    let store = match c2::open_untimed(&sample) {
        Ok(store) => store,
        Err(error) => return Ok(c1::unmeasured(&error, gates)),
    };
    let provider = StoreProvider::new(&store);
    let tuple = fs::three_tuple(&provider, result.root);
    match tuple {
        Ok(tuple) => gates.push(gates::require(
            GateClass::Correctness,
            "g1.o2-root-readback",
            tuple.inode_table == result.value.inode_table(),
            &format!("read back inode table {}", tuple.inode_table),
            &format!("built inode table {}", result.value.inode_table()),
        )),
        Err(error) => gates.push(Gate::fail(
            GateClass::Correctness,
            "g1.o2-root-readback",
            &format!("{error}"),
            "the built filesystem reads back through the Store",
        )),
    }
    // The strongest O2 reading: every directory's listing, read out of the Store
    // the measured phase itself wrote, compared against the fixture manifest.
    match fs::listings_match(&provider, result.root, &prepared)? {
        Ok(directories) => gates.push(gates::require(
            GateClass::Correctness,
            "g1.o2-listing",
            true,
            &format!("{directories} directories read back through the Store"),
            "every directory's bindings equal the fixture manifest",
        )),
        Err(disagreement) => gates.push(Gate::fail(
            GateClass::Correctness,
            "g1.o2-listing",
            disagreement.as_str(),
            "every directory's bindings equal the fixture manifest",
        )),
    }
    gates.extend(c2::sidecar_gates(&sample));
    gates.push(gates::swap_gate(instruments::swaps()));

    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("pipeline_op: {op:?}"),
            format!("declared_files: {}", config.files),
            format!("declared_directories: {}", config.directories),
            "measured_region: Store::open + build_filesystem + save + acknowledgement".to_string(),
            "handoff: layerfs_storage::SaveHandoff, the product's own C1-to-save adapter".to_string(),
            "store_state: opened-from-copy".to_string(),
            format!("copy_rung: {}", c2::COPY_RUNG),
            format!("allocation_attribution: {}", c2::ATTRIBUTION),
            format!("heap_charged_bytes: {}", heap.charged_bytes),
        ],
    })
    .map(|outcome| {
        let _ = case;
        outcome
    })
}
