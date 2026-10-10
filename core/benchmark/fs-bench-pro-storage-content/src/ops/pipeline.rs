//! `pipeline.*`: the integrated C1 to C2 handoff.
//!
//! The four rows are the only place in the registry where the C1 and C2 halves
//! run **in one region**. `test_setup_and_cache_discipline.md` section 2.2 fixes
//! what is inside the timer: `update_filesystem` (or `apply_edits`) plus
//! the Store open plus the save plus its acknowledgement. C1 construction of the
//! base is setup; the measured C1 half is the operation that edits it.
//!
//! **The handoff is the product's own adapter.** `Save::sink()` returns
//! `layerfs-storage`'s `SaveSink`, the C1 consumer adapter that forwards to
//! `Save::accept` (it replaced the removed `SaveHandoff`), so every object C1
//! emits reaches the save operation on the path the product provides and nothing
//! is retained by the harness inside the heap window. The count of what crossed
//! is taken by `CountingConsumer` on that same path, which is why `g2.handoff`
//! compares the objects C1 emitted with the objects the save acknowledged rather
//! than with a number the save reports about itself.
//!
//! **The base is read from the Store, not from harness memory.** A pipeline row
//! that read its base from a `TreeStore` would prove nothing about the handoff in
//! the read direction. The reader is `Storage::reader()` over the sample Store
//! (it replaced the removed `StoreProvider`), so the measured region genuinely
//! goes C1 -> C2 for the base and C1 -> C2 for the result.
//!
//! Ported 2026-10-10 to the current public API through `ops::store`; the Store is
//! Disposable / WAL / `synchronous = OFF`, selected explicitly. Counters with no
//! current source are listed in `ops::store::UNAVAILABLE_COUNTERS` and not emitted.

use layerfs_content::filesystem::references::backing::OrderingBacking;
use layerfs_content::filesystem::{
    build_filesystem, build_filesystem_timed, scope_for_seed, update_filesystem,
    update_filesystem_timed, FilesystemInput, FilesystemObjects, FilesystemPhases,
    FilesystemResources, FilesystemRootId,
};
use layerfs_content::{
    apply_edits, construct_bytes, construct_stream, ConstructionPolicy, Edit, EditRequest, ObjectId,
};
use layerfs_telemetry::timer::{Active, Timing, TimingScope};

use super::c1;
use super::c2;
use super::fs;
use super::fs_fixture::{PreparedTree, Recipe, ROOT_SERIAL};
use super::store::{self, Store};
use crate::workload::edits::{Edits, Parts};
use super::{seed_of, OpContext, OpError, OpOutcome};
use crate::fixture::{self, structured};
use crate::gates::{self, Gate, GateClass};
use crate::registry::{Case, PipelineOp};
use crate::support::instruments;
use crate::support::trace::Kind;
use crate::workload::oracle::Expectation;
use crate::workload::providers::{
    CountingConsumer, PairProvider, PrefixKeys, PrefixProvider, TreeStore,
};
use crate::workload::stream::{NoiseReader, STREAM_WINDOW_BYTES};

/// Whether this invocation builds the fixture's content **inside** the timer, from a stream.
///
/// `LAYERFS_PIPELINE_STREAM_FIXTURE=1` replaces the untimed construction pass and the in-memory content
/// store with a per-file [`NoiseReader`] feeding `construct_stream` inside the measured closure, so the
/// fixture never exists as bytes in this process - a window per file instead of 502,914,928 B retained.
///
/// **This moves the row's boundary, and that is the owner's ruling, not a default.** The row's declared
/// figure excludes content construction (`test_setup_and_cache_discipline.md` section 2.2); under this
/// switch the construction is *inside* the region the figure is read from, which is the boundary the
/// reference product's own timer has - v0.1.6's `init_namespace` read 887,242,752 bytes off disk inside
/// `layerstack_init_ns` (`benchmark-results/issue152/g1/namespace-100000-r4/perf.jsonl`). Unset, the row
/// behaves exactly as it did before this switch existed.
fn stream_fixture_requested() -> bool {
    std::env::var("LAYERFS_PIPELINE_STREAM_FIXTURE").is_ok_and(|value| value == "1")
}

/// Charges the canonical bytes of every object offered to the save.
///
/// **Why logical length will not do.** `pipeline.content_bytes` is pinned at **502,914,928** canonical
/// bytes for this row's 500,000,000 declared. A streamed construction knows each file's *logical* length
/// (that is what it asked for), but the canonical bytes are the consumer's business: 11.7 % of envelope
/// and hash framing sits between the two, and the gap is the whole difference between the pinned figure
/// and the declared one. This counts where the objects actually pass, so the streamed path publishes the
/// same quantity the store-held path published.
struct ContentMeter<'a> {
    inner: &'a mut dyn layerfs_content::FinalizedConsumer,
    canonical_bytes: u64,
    objects: u64,
}

impl<'a> ContentMeter<'a> {
    fn new(inner: &'a mut dyn layerfs_content::FinalizedConsumer) -> Self {
        Self {
            inner,
            canonical_bytes: 0,
            objects: 0,
        }
    }
}

impl layerfs_content::FinalizedConsumer for ContentMeter<'_> {
    fn accept(&mut self, object: layerfs_content::FinalizedObject) -> layerfs_content::ContentResult<()> {
        self.canonical_bytes = self.canonical_bytes.saturating_add(object.canonical_len() as u64);
        self.objects += 1;
        self.inner.accept(object)
    }
}

/// Bytes the plan expects the fixture to contribute, and the most objects it could produce.
///
/// `pipeline.content_objects` is **objects produced**, not files: the row's pinned 109,414 is 98,998
/// whole-file members plus 10,330 CDC chunks from the two 100,000,000-byte anchors, and no count of
/// non-empty files can reproduce it. The streamed path therefore publishes what it actually offered -
/// the same quantity the store-held path published, since the store held exactly those objects - and this
/// helper supplies the bound that keeps that count honest: a whole-file member is one object, so the
/// object count can never fall below the non-empty file count, and `content_bytes` can never exceed the
/// logical total plus one envelope per object.
fn planned_content_files(plan: &super::namespace_content::BytePlan) -> u64 {
    plan.files.iter().filter(|file| file.size > 0).count() as u64
}

/// The widest per-object envelope the canonical framing can add.
///
/// A framed object is a 9-byte header, a 4-byte length and the payload; the payload ceiling is what the
/// policy allows, and this bound is deliberately generous because it exists to catch a *shape* error -
/// content that never reached the save - and not to re-derive the codec.
const CONTENT_FRAME_CEILING_BYTES: u64 = 4096;

/// The build phases this row reads back, in the order the product records them.
const BUILD_PHASES: [&str; 6] = [
    "validate",
    "directories",
    "references",
    "inodes",
    "cleanup",
    "root.encode",
];

/// Totals one phase name per entry, summed over every batch of one row.
///
/// The report is walked rather than the phases captured at the call site because
/// the phases are recorded by the product, inside the product's own tree; a
/// caller that timed them itself would be measuring a different thing from the
/// one `build_filesystem_timed` charges.
fn build_phase_totals(report: &layerfs_telemetry::timer::TimingReport) -> Vec<(&'static str, u64)> {
    let mut totals = [0_u64; BUILD_PHASES.len()];
    fn walk(node: &layerfs_telemetry::timer::TimingNode, totals: &mut [u64; BUILD_PHASES.len()]) {
        for child in node.children() {
            if let Some(index) = BUILD_PHASES.iter().position(|name| *name == child.name()) {
                totals[index] += child.elapsed().as_nanos() as u64;
            }
            walk(child, totals);
        }
    }
    if let Some(root) = report.root() {
        walk(root, &mut totals);
    }
    BUILD_PHASES.iter().copied().zip(totals).collect()
}

/// The 4 KiB replacement the edit pipeline rows use, as the C1 edit families do.
pub const REPLACEMENT_LEN: u64 = 4_096;

/// Logical bytes the `namespace-10000` parity row declares, matching the reference
/// case: 300,000,000 of file content plus a 100,000,000-byte anchor.
pub const NAMESPACE_SCALE_BYTES: u64 = 300_000_000;

/// What one namespace-scale row declares.
///
/// The declaration is a **port, not an invention**, and the two rows port two
/// different reference scenarios: `namespace-10000` (10,000 files / 100
/// directories / 300,000,000 B / one 100,000,000-byte anchor) and
/// `namespace-100000` (100,000 files / 1,000 directories / **500,000,000** decimal
/// bytes / **two** 100,000,000-byte anchors / the scaled band mix). Neither is a
/// total to choose: both are `Declaration` constants read out of the reference
/// harness's own `NAMESPACE_SCENARIOS` table.
///
/// The two rows' declarations are therefore *different shapes at different
/// totals*, and the comparison between them is a comparison of two declared
/// scaling points rather than of one shape at two sizes. That is what the
/// reference declares and it is what this harness ports; a row that reused the
/// 10,000-entry declaration at `entries = 100,000` would measure 97,899 tiny files
/// and one anchor, which is a legitimate fixture and **not** the reference's case.
pub fn declaration(op: PipelineOp) -> super::namespace_content::Declaration {
    use super::namespace_content::Declaration;
    match op {
        PipelineOp::NamespaceScaleLarge => Declaration::LARGE,
        _ => Declaration::TEN_THOUSAND,
    }
}

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
        // The reference `namespace-10000` shape: 10,000 files over 100
        // directories, 300,000,000 logical bytes with a 100,000,000-byte anchor.
        PipelineOp::NamespaceScale => Configuration {
            base_bytes: 0,
            start: 0,
            end: 0,
            replacement: 0,
            files: 10_000,
            directories: 100,
        },
        // The reference `namespace-100000` shape: 100,000 files over **1,000**
        // directories, 500,000,000 decimal bytes with **two** 100,000,000-byte
        // anchors. The directory count is not decoration: the plan's index space is
        // `directory * FILES_PER_DIRECTORY + ordinal` with `ordinal = position /
        // directories`, so 100,000 files over 100 directories reuse index 100 from
        // position 10,000 on and give 100,000 files 10,000 distinct serials. 1,000
        // directories is the declaration that keeps the space injective, and it is
        // also the reference's own `data_directories` for this scenario.
        PipelineOp::NamespaceScaleLarge => Configuration {
            base_bytes: 0,
            start: 0,
            end: 0,
            replacement: 0,
            files: 100_000,
            directories: 1_000,
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
        PipelineOp::NamespaceScale | PipelineOp::NamespaceScaleLarge => {
            namespace_scale(case, op, context)
        }
        _ => edit(case, op, context),
    }
}

/// The C1 half of an edit pipeline row: the base, built and stored before the
/// timer, plus the edit request the measured phase applies.
struct EditSetup {
    base_store: TreeStore,
    base_root: ObjectId,
    stream: Edits,
    source: Parts,
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
    let stream = Edits::new(
        bytes.len() as u64,
        vec![Edit::new(config.start, config.end, config.replacement)],
    )
    .map_err(|error| OpError::Product(format!("{error:?}")))?;
    let replacement = if config.replacement > 0 {
        fixture::noise(config.replacement, seed ^ 0x2222)
    } else {
        Vec::new()
    };
    let mut source = Parts::new();
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
    let (measured, report) = super::measure("pipeline", |scope: &TimingScope<'_, Active>| {
        let store = Store::open(&sample)?;
        let provider = store.reader()?;
        let operation = store.begin_save()?;
        let file = {
            let mut handoff = operation.sink();
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
            if let Some(error) = operation.take_failure() {
                return Err(PipelineFailure::Storage(error));
            }
            file
        };
        let outcome = operation.finish()?;
        Ok::<_, PipelineFailure>((file, outcome))
    });
    let heap = instruments::heap_end();
    let timing_bytes = crate::support::phases::timing_json_bytes();
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
    context.trace.write_number(Kind::Counter, "pipeline.handoff_objects", emitted as i128, "objects", "CountingConsumer on the SaveSink path, measured phase")?;
    context.trace.write_number(Kind::Counter, "pipeline.inserted", i128::from(outcome.inserted), "objects", "WriteOutcome.inserted")?;
    context.trace.write_number(Kind::Counter, "pipeline.reused", i128::from(outcome.reused), "objects", "WriteOutcome.reused")?;
    context.trace.write_number(Kind::Counter, "pipeline.nodes_read", file.counters.nodes_read as i128, "nodes", "EditCounters.nodes_read")?;
    context.trace.write_number(Kind::Counter, "pipeline.nodes_created", file.counters.nodes_created as i128, "nodes", "EditCounters.nodes_created")?;
    context.trace.write_number(Kind::Counter, "timing_json_bytes", timing_bytes as i128, "bytes", "product timing.json, byte-verbatim")?;
    context.trace.write_number(Kind::Resource, "heap.peak_incremental_bytes", heap.peak_incremental_bytes as i128, "bytes", "counting GlobalAlloc, measured phase")?;

    gates.push(c1::completeness_gate(&report, "g7.tree-complete"));
    crate::workload::expected::publish(context.trace, "file_root", &file.root.to_string())?;
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
        emitted > 0 && outcome.inserted + outcome.reused == emitted,
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
    gates.extend(c2::sealed_sidecar_gates(store, &sample));
    gates.push(gates::swap_gate(instruments::swaps()));

    Ok(OpOutcome {
        gates,
        notes: vec![
            store::profile_note(),
            store::unavailable_note(&["pipeline.commits", "pipeline.statements"]),
            format!("pipeline_op: {op:?}"),
            format!("declared_base_bytes: {}", config.base_bytes),
            format!("declared_edit: {}..{} -> {} bytes", config.start, config.end, config.replacement),
            format!("cutoff_bytes: {}", cutoff()),
            "measured_region: Store open + apply_edits + save + acknowledgement".to_string(),
            "base_reader: Storage::reader() over the sample copy".to_string(),
            "handoff: layerfs_storage::SaveSink, the product's own C1-to-save adapter".to_string(),
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
    // **The row's own memory window.** `phases::measured` publishes
    // `rss.process_peak_bytes`, which is `getrusage(RUSAGE_SELF).ru_maxrss` - a
    // **lifetime** high-water, and `memory_cpu_space_support.md` section 3.2 is
    // explicit that a lifetime figure is never an incremental one. So the row had no
    // phase peak at all: it could not say how much of its 1.05 GB belonged to the
    // measured region and how much to the fixture the driver holds before the timer.
    // This samples the measured region at the declared 10 ms interval and publishes
    // the bundle, so the two questions are separable. Diagnostic: never pinned.
    let (measured, report) = super::measure("pipeline", |_timing: &TimingScope<'_, Active>| {
        let store = Store::open(&sample)?;
        let operation = store.begin_save()?;
        let result = {
            let mut handoff = operation.sink();
            let mut counting = CountingConsumer::new(&mut handoff);
            let reader = PairProvider::new(&empty, &empty);
            let mut objects = FilesystemObjects::new(&reader, &mut counting);
            let result = build_filesystem(&mut objects, &input, None)?;
            emitted = counting.accepted();
            result
        };
        let outcome = operation.finish()?;
        Ok::<_, PipelineFailure>((result, outcome))
    });
    let heap = instruments::heap_end();
    let timing_bytes = crate::support::phases::timing_json_bytes();
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
    context.trace.write_number(Kind::Counter, "pipeline.handoff_objects", emitted as i128, "objects", "CountingConsumer on the SaveSink path, measured phase")?;
    context.trace.write_number(Kind::Counter, "pipeline.inserted", i128::from(outcome.inserted), "objects", "WriteOutcome.inserted")?;
    context.trace.write_number(Kind::Counter, "pipeline.reused", i128::from(outcome.reused), "objects", "WriteOutcome.reused")?;
    context.trace.write_number(Kind::Counter, "pipeline.objects_emitted", result.counters.objects.objects_emitted as i128, "objects", "ObjectWork.objects_emitted")?;
    context.trace.write_number(Kind::Counter, "timing_json_bytes", timing_bytes as i128, "bytes", "product timing.json, byte-verbatim")?;
    context.trace.write_number(Kind::Resource, "heap.peak_incremental_bytes", heap.peak_incremental_bytes as i128, "bytes", "counting GlobalAlloc, measured phase")?;

    gates.push(c1::completeness_gate(&report, "g7.tree-complete"));
    crate::workload::expected::publish(
        context.trace,
        "filesystem_root",
        &result.root.0.to_string(),
    )?;
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
        emitted > 0 && outcome.inserted + outcome.reused == emitted,
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
    let provider = match store.reader() {
        Ok(provider) => provider,
        Err(error) => {
            return Ok(c1::unmeasured(
                &OpError::Product(format!("{error:?}")),
                gates,
            ))
        }
    };
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
    drop(provider);
    gates.extend(c2::sealed_sidecar_gates(store, &sample));
    gates.push(gates::swap_gate(instruments::swaps()));

    Ok(OpOutcome {
        gates,
        notes: vec![
            store::profile_note(),
            store::unavailable_note(&["pipeline.commits"]),
            format!("pipeline_op: {op:?}"),
            format!("declared_files: {}", config.files),
            format!("declared_directories: {}", config.directories),
            "measured_region: Store open + build_filesystem + save + acknowledgement".to_string(),
            "handoff: layerfs_storage::SaveSink, the product's own C1-to-save adapter".to_string(),
            "store_state: opened-from-copy".to_string(),
            format!("copy_rung: {}", c2::COPY_RUNG),
            format!("allocation_attribution: {}", c2::ATTRIBUTION),
            format!("heap_charged_bytes: {}", heap.charged_bytes),
        ],
    })
    .inspect(|_| {
        let _ = case;
    })
}

/// One `namespace-*` parity row: the namespace **and** its declared content saved
/// in one measured region.
///
/// This is the shape the reference `init_namespace` case measures and that no other
/// v0.1.7 row performs. `c1.fs.build-scale` builds the same tree but emits no
/// content at all, and `pipeline-filesystem-build` saves a filesystem whose inodes
/// are empty. Neither moves bytes into a Store.
///
/// **Two rows drive this one function**, `pipeline-namespace-10000` and
/// `pipeline-namespace-100000`, and they differ only in the declaration they read
/// from [`declaration`]: the entry count, the directory count, the band mix, the
/// anchor count and the total. The driver takes its batched route automatically at
/// either size - `PreparedTree::batches` splits the tree at the workload's own
/// `BUILD_BATCH_BINDINGS` (4,096), which a 10,101-binding tree already exceeds.
///
/// **Why the content is a second stream.** `build_filesystem` emits metadata only:
/// its internal `contents` map holds *directory* roots, and a file's `content_root`
/// is copied out of the input `InodeValue` without ever being demanded from the
/// reader. So a filesystem build cannot put the declared content into a Store, and
/// the row has to construct that content itself and accept it on the same
/// `Save`. The construction happens before the timer; the save does not.
fn namespace_scale(
    case: &Case,
    op: PipelineOp,
    context: &mut OpContext<'_>,
) -> Result<OpOutcome, OpError> {
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

    // The byte plan: the reference scenario's own declaration, ported rather than
    // re-derived from the entry count. `declaration` returns the whole tuple -
    // entries, directories, total and band mix - so the tree recipe above and the
    // content plan here cannot disagree about the shape they are building.
    let declared = declaration(op);
    // Two routes now describe one shape - `configuration` for the tree, this
    // declaration for the content - and a row whose two halves disagree would
    // measure a fixture no page declares. It is a harness defect rather than a
    // measurement, so it is refused before the timer with both numbers named.
    if declared.entries != config.files || declared.directories != config.directories {
        return Ok(c1::unmeasured(
            &OpError::Io(format!(
                "row declaration disagrees with its configuration: {} files over {} \
                 directories against {} over {}",
                declared.entries, declared.directories, config.files, config.directories
            )),
            Vec::new(),
        ));
    }
    let plan = match super::namespace_content::plan(&declared, seed) {
        Ok(plan) => plan,
        Err(defect) => return Ok(c1::unmeasured(&OpError::Io(defect), Vec::new())),
    };

    // Construct the content. Untimed: C2's rule is that every canonical object a
    // C2 row saves is supplied by the harness, and construction is C1's half.
    //
    // **The excluded work is charged anyway, and published.** Untimed is not
    // unmeasured: the v0.1.6 `init_namespace` timer *includes* reading its fixture
    // and building the objects it admits, while this row's timer excludes that
    // construction and includes the C1 tree build - so the two published figures
    // are not the same measurement, and only their sum (`preparation_wall_ns`) was
    // available to a reader who wanted the difference. The two parts are charged
    // here, each around existing code, and neither enters the row's formula; they
    // are diagnostics beside it, like `establishment_ns` and `teardown_ns`.
    let policy = ConstructionPolicy::frozen_default();
    let capacities = policy.capacities();
    // **Under the streaming switch the fixture is not built here.** Its bytes are regenerated per file
    // inside the timer, so this untimed pass holds nothing and the store below stays empty; the noise
    // span is still charged, because generating those bytes is what the row now pays for inside the
    // region. Unset, every line of this behaves as it did before the switch existed.
    let streaming = stream_fixture_requested();
    let mut content = TreeStore::new();
    let mut construct_ns = 0_u64;
    let mut construct_noise_ns = 0_u64;
    for file in if streaming { &plan.files[..0] } else { &plan.files[..] } {
        if file.size == 0 {
            continue;
        }
        // The index the plan gave this file, re-derived rather than carried: the
        // tree's file serials are `2 + directories + index`, so this inverts exactly
        // the mapping `namespace_content::plan` applied forward. It must use the
        // declaration's directory count, because the serials the recipe built did.
        let index = u64::from(file.directory) * super::namespace_content::FILES_PER_DIRECTORY
            + file.serial
            - (2 + u64::from(declared.directories));
        let noise_started = std::time::Instant::now();
        let bytes = fixture::noise(file.size, seed ^ index.rotate_left(13));
        construct_noise_ns += noise_started.elapsed().as_nanos() as u64;
        let construct_started = std::time::Instant::now();
        let (result, _) = Timing::disabled(
            "setup.construct",
            |scope: &TimingScope<'_, Active>| {
                construct_bytes(
                    policy,
                    &capacities,
                    &bytes,
                    &mut content,
                    scope.child("content"),
                )
            },
        );
        construct_ns += construct_started.elapsed().as_nanos() as u64;
        let constructed = match result {
            Ok(constructed) => constructed,
            Err(error) => {
                return Ok(c1::unmeasured(
                    &OpError::Product(format!("{error:?}")),
                    Vec::new(),
                ))
            }
        };
        if constructed.logical_len != file.size {
            return Ok(c1::unmeasured(
                &OpError::Io(format!(
                    "constructed {} bytes for a {} byte file",
                    constructed.logical_len, file.size
                )),
                Vec::new(),
            ));
        }
    }
    // Objects the content stream offers inside the timer. Declared here because
    // `pipeline.content_objects` is published from it when the switch is on; counted below, where the
    // stream runs. Without the switch it stays 0 and the store's own length is published instead.
    let mut content_objects_offered = 0_u64;
    let content_objects_store_held = content.len() as u64;

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

    // A 10,000-file / 100-directory tree states 10,101 bindings, and this row builds
    // it in operations of at most `BUILD_BATCH_BINDINGS` (4,096) bindings each. That
    // figure is a **workload constant** of this harness, kept so the batch count and
    // the operation boundaries stay what the recorded rows measured; it is not a
    // product refusal (the historical `MAXIMUM_WALK_ENTRIES` ceiling this comment
    // used to cite is no longer in `layerfs-content`). `PreparedTree::batches` is the
    // route, and `c1.fs.build-scale`'s own namespace-10000 row does the same thing
    // and reports `fs_build.operations: 3`.
    let batches = match prepared.batches(fs::WALK_CEILING) {
        Ok(batches) => batches,
        Err(defect) => return Ok(c1::unmeasured(&OpError::Io(defect), gates)),
    };

    // The reader chain: every object any batch will name, built **before** the
    // timer. Inside the measured region a batch's base has already been handed to
    // the save operation, so no batch can read it back from there; `fs.rs` takes the
    // same route, where `measure_batch`'s reader is a pre-computed chain and never
    // the live Store. Reader and consumer are different objects, so the accumulating
    // store can be borrowed for reading while a fresh one takes the batch's output.
    //
    // **One chain, and one identity set per batch** — not one chain copy per batch.
    // This loop used to keep a `TreeStore` snapshot of the chain at each step, and
    // because `TreeStore::absorb` copies, the driver retained the sum over batches of
    // the chain so far: at 100,000 entries, 25 snapshots holding 66,824 objects and
    // 164,347,158 bytes to serve a chain whose final state is 4,221 objects and
    // 12,452,785 bytes (round 21 `report.md` section 11.4). What a batch is owed is
    // the chain *as it stood before it*, which is a statement about the objects a
    // reader may serve rather than about holding a second copy of them, so this loop
    // keeps the identities and `PrefixProvider` serves the batch through them.
    let empty = TreeStore::new();
    // One ordering backing per batch per phase, created before the timer: a batch's
    // bindings outgrow the in-memory ordering map, and `FileBacking::new` scans its
    // directory for the highest run token it owns, which is harness work rather than
    // product work. This is the rule `fs.rs` applies to its own batched rows.
    let mut plan_backings = match fs::batch_backings(context, "plan", batches.len()) {
        Ok(backings) => backings,
        Err(error) => return Ok(c1::unmeasured(&error, gates)),
    };
    let mut oracle_backings = match fs::batch_backings(context, "oracle", batches.len()) {
        Ok(backings) => backings,
        Err(error) => return Ok(c1::unmeasured(&error, gates)),
    };
    let mut perf_backings = match fs::batch_backings(context, "perf", batches.len()) {
        Ok(backings) => backings,
        Err(error) => return Ok(c1::unmeasured(&error, gates)),
    };
    let mut chain = TreeStore::new();
    // One **key set** per batch index, and no copy: the chain as it stood before that
    // batch, expressed as the identities that batch may be served. Measured, not
    // assumed: a standalone probe passes every batch at budgets 4096, 2048 and 1024
    // when the reader is the chain-so-far, while the driver's timed pass returned
    // `cycle check work limit` when it served every batch from the complete chain. A
    // batch's base is what the *previous* batches produced, so handing it the whole
    // chain is both wrong and more expensive; `PrefixProvider` hands it the same set
    // the snapshot did, at 3.8 MB instead of 164 MB.
    //
    // `prefixes[0]` is the empty set — batch 0 has no base — and `prefixes[index]` is
    // captured *after* batch `index - 1` was absorbed, so the vector is indexed by
    // batch exactly as the snapshots were.
    let mut prefixes: Vec<PrefixKeys> = vec![PrefixKeys::new()];
    let mut planned_root: Option<FilesystemRootId> = None;
    for (index, batch) in batches.iter().enumerate() {
        let input = FilesystemInput {
            base: planned_root,
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
                Some(&mut plan_backings[index] as &mut dyn OrderingBacking);
            if planned_root.is_none() {
                build_filesystem(&mut objects, &input, ordering)
            } else {
                update_filesystem(&mut objects, &input, ordering)
            }
        };
        match built {
            Ok(built) => planned_root = Some(built.root),
            Err(error) => {
                return Ok(c1::unmeasured(
                    &OpError::Product(format!(
                        "untimed chain batch {index}/{} of {}: {error:?}",
                        batches.len(),
                        batches.len()
                    )),
                    gates,
                ))
            }
        }
        // What this batch produced is chained (so the *next* batch can read it) and
        // recorded as that batch's own prefix. `emitted` is fresh per batch and a
        // batch emits only what it changed, so the length delta is exactly the
        // objects to admit; an operation that emitted something it had already
        // emitted would be a defect here rather than a silently counted duplicate.
        let before = chain.len();
        chain.absorb(&emitted);
        if chain.len() != before + emitted.len() {
            return Ok(c1::unmeasured(
                &OpError::Io(format!(
                    "untimed chain batch {index} emitted {} objects over a chain of \
                     {before} but advanced it by {}",
                    emitted.len(),
                    chain.len() - before
                )),
                gates,
            ));
        }
        // The prefix a batch is owed is **cumulative** - everything every earlier
        // batch produced - and a chain's objects are never evicted once chained, so
        // each key set is the previous one plus this batch's emissions. The copies
        // are 3,786,440 bytes of identity set at 100,000 entries; the `TreeStore`
        // this replaces were 164,347,158 bytes.
        let mut keys = prefixes[index].clone();
        keys.extend(emitted.insertion_order());
        if keys.len() != chain.len() {
            return Ok(c1::unmeasured(
                &OpError::Io(format!(
                    "untimed chain batch {index}: key set {} does not describe the chain {}",
                    keys.len(),
                    chain.len()
                )),
                gates,
            ));
        }
        prefixes.push(keys);
    }
    let chain_objects = chain.len() as u64;
    // What the identity sets retained, published beside `chain_objects` so the row's
    // fixture cost is readable from its own counters rather than re-derived: the
    // chain, plus one key set per batch whose size is that batch's prefix. A batch
    // that is served its whole prefix is the rule, so the check below is not a
    // formality - it is the difference between "the keys stand in for the snapshots"
    // and "the batch stopped reading what it emitted", which the tree's own pinned
    // digest cannot see while the read count falls.
    let prefix_keys_total: usize = prefixes.iter().map(PrefixKeys::len).sum();
    let prefix_keys_largest = prefixes.iter().map(PrefixKeys::len).max().unwrap_or(0);

    let mut metadata_emitted = 0_u64;
    let mut content_bytes = 0_u64;
    // Objects the content stream moved into the save operation. Counted here rather
    // than read off `content.len()` afterwards, because the drain is what empties it.
    // The streamed path's own readings. `content_rng_seed_ns` charges the noise generation the untimed
    // pass used to charge into `construct_noise_ns`, and `content_stream_ns` is the whole content region
    // inside the timer, published so a reader can see which boundary produced the figure.
    let mut content_rng_seed_ns = 0_u64;
    let mut content_stream_ns = 0_u64;
    let mut batch_roots = 0_u64;
    let mut largest_batch = 0_u64;
    // The accept span. It runs from the moment the operation is owned to the moment
    // `finish` returns. It was the denominator the removed `SaveProfile`'s seven
    // disjoint buckets were reported against; those buckets have no current source
    // (`ops::store::UNAVAILABLE_COUNTERS`), and the span is still published because
    // it is the harness's own `Instant` pair, not a product timing node.
    let mut accept_span_ns = 0_u64;
    // What the per-batch prefix keys actually served, accumulated over the timed
    // pass. This is the reading that keeps the identity sets honest: with one chain
    // and one key set per batch, `prefixes[index]` exists for exactly one reason -
    // to admit the objects a batch's base needs - and a batch whose reader admitted
    // nothing did not read what it emitted, so the chain was not load-bearing.
    let mut prefix_served = 0_u64;
    let mut prefix_refused = 0_u64;
    let mut prefix_batches_served = 0_u64;
    // The accept span, split by its own driver into the three regions that share
    // it: the C1 construction of the metadata stream (the caller builds the whole
    // 10,101-binding filesystem inside the timed closure), the content stream's
    // own accept loop, and the save's seal. The product's seven buckets are charged
    // inside the second and third of these and charged to nothing in the first, so
    // without this split a remainder measured against the span cannot tell product
    // work from the driver's own work. Diagnostics only, not pinned.
    let mut build_span_ns = 0_u64;
    let mut content_span_ns = 0_u64;
    let mut finish_span_ns = 0_u64;
    // `SaveOutcome.pack_bytes_written` is `Storage::diagnostics().pack_write_bytes`
    // across the one save (`ops::store::COUNTER_MAPPINGS`).
    let mut pack_bytes_written = 0_u64;
    let mut establishment_ns = 0_u64;
    // The build span's two parts: the product's `accept` calls, and (by
    // subtraction) the caller's tree build. `span_build_ns` is one span; a span
    // cannot say which half is the product's.
    let mut build_accept_ns = 0_u64;
    // The build's own work counters, accumulated over its batches. Round 10 put
    // 221.70 ms in `validate` and nothing in the tree says which loop spends it;
    // the product already maintains these and this row is the first to publish
    // them, so the phase's price can be read per entry read and per entry walked
    // instead of guessed at.
    #[derive(Default)]
    struct BuildWork {
        objects_read: u64,
        base_records_read: u64,
        validation: layerfs_content::filesystem::validate::ValidationWork,
        sites: layerfs_content::filesystem::validate::ValidationReadSites,
    }
    let mut build_work = BuildWork::default();
    // **The row's own memory window.** `phases::measured` publishes
    // `rss.process_peak_bytes`, which is `getrusage(RUSAGE_SELF).ru_maxrss` - a
    // **lifetime** high-water, and `memory_cpu_space_support.md` section 3.2 is
    // explicit that a lifetime figure is never an incremental one. So the row had no
    // phase peak at all: it could not say how much of its 1.05 GB belonged to the
    // measured region and how much to the fixture the driver holds before the timer.
    // This samples the measured region at the declared 10 ms interval and publishes
    // the bundle, so the two questions are separable. Diagnostic: never pinned.
    let rss_sampler = instruments::RssSampler::start();
    instruments::heap_begin();
    let (measured, report) = super::measure("pipeline", |timing: &TimingScope<'_, Active>| {
        // **The row's formula excludes the connection.** The measured closure
        // still spans everything, so nothing is hidden, but the number this row
        // reports as its work is `accept_span_ns` minus the teardown, and the
        // establishment that precedes `accept_span_ns` is published as its own
        // figure. Both are counted here so a reader can reconstruct the inclusive
        // span from the receipt: `establishment_ns + accept_span_ns` is the whole
        // closure and `teardown_ns` is inside `accept_span_ns`.
        //
        // Why the boundary moved, and what it costs: the teardown is the save's
        // connection close, which is the operating system's price for the pages
        // the save wrote (~0.75 ms/MB, identical in every journal mode, measured
        // with no product code in
        // `docs/roadmap/0.1/0.1.7/evidence/issue219-ns19d-release-20260921T070000Z/close-probe.py`).
        // It is a property of the machine and the window, not of the operation:
        // 6.7 ms in one row and 469.9 ms in another, identical product code. Keeping
        // it in the phase makes the row's level uncomparable between windows, which
        // is the failure the campaign already recorded once. It is published rather
        // than dropped, and the v0.1.6 pairing must use the same boundary on both
        // sides.
        //
        // Ported 2026-10-10: the teardown figure was a `SaveProfile` bucket
        // (`diag.finish_drop_ns`) and has no current source, so `pipeline.teardown_ns`
        // and the formula built on it (`pipeline.operation_work_ns`) are not emitted;
        // `pipeline.establishment_ns` and `pipeline.accept_span_ns` still are.
        let closure_started = std::time::Instant::now();
        let store = Store::open(&sample)?;
        // Cumulative per handle, so the save's own pack bytes are after minus before.
        let pack_bytes_before = store.diagnostics().pack_write_bytes;
        let operation = store.begin_save()?;
        let accept_started = std::time::Instant::now();
        let result = {
            let mut handoff = operation.sink();
            let mut counting = CountingConsumer::new(&mut handoff);
            let mut last: Option<layerfs_content::FilesystemResult> = None;
            for (index, batch) in batches.iter().enumerate() {
                largest_batch = largest_batch.max(batch.bindings() as u64);
                let base = last.as_ref().map(|result| result.root);
                let input = FilesystemInput {
                    base,
                    scope,
                    root_serial: ROOT_SERIAL,
                    directories: &batch.directories,
                    inodes: &batch.inodes,
                    new_inodes: &batch.new_inodes,
                    resources: FilesystemResources::default(),
                };
                // The chain as it stood before this batch, out of the one complete
                // chain: `prefixes[index]` is the identity set, not a copy. Every
                // object this reader serves is still re-identified by the store.
                let reader = PrefixProvider::new(&chain, &prefixes[index]);
                // The build's own phases. `FilesystemPhases` has had no call site
                // outside its definition until here, so this is the first run that
                // can say where the span goes: `validate`, `directories`,
                // `references`, `inodes`, `cleanup` and `root.encode` are charged
                // by the product inside this scope and published below.
                let built = timing
                    .child("build")
                    .run(|build| -> Result<_, PipelineFailure> {
                        let phases = FilesystemPhases::new(build);
                        let mut objects = FilesystemObjects::new(&reader, &mut counting);
                        let ordering: Option<&mut dyn OrderingBacking> =
                            Some(&mut perf_backings[index] as &mut dyn OrderingBacking);
                        if base.is_none() {
                            Ok(build_filesystem_timed(&mut objects, &input, ordering, &phases)?)
                        } else {
                            Ok(update_filesystem_timed(&mut objects, &input, ordering, &phases)?)
                        }
                    })?;
                build_work.validation.objects_read += built.counters.validation.objects_read;
                build_work.validation.read_waves += built.counters.validation.read_waves;
                build_work.validation.inode_demands += built.counters.validation.inode_demands;
                build_work.validation.inode_pages_read += built.counters.validation.inode_pages_read;
                build_work.validation.directory_pages_read +=
                    built.counters.validation.directory_pages_read;
                build_work.validation.entries_examined += built.counters.validation.entries_examined;
                let sites = built.counters.validation.inode_pages_by_site;
                build_work.sites.allocation += sites.allocation;
                build_work.sites.prefetch += sites.prefetch;
                build_work.sites.bindings += sites.bindings;
                build_work.sites.aliases += sites.aliases;
                build_work.sites.cycles += sites.cycles;
                build_work.sites.reachability += sites.reachability;
                build_work.objects_read += built.counters.objects.objects_read;
                build_work.base_records_read += built.counters.base_records_read;
                let (served, refused) = reader.served();
                prefix_served += served;
                prefix_refused += refused;
                if served > 0 {
                    prefix_batches_served += 1;
                }
                last = Some(built);
                batch_roots += 1;
            }
            // `batches` is non-empty for any prepared tree that has files; a tree
            // with none is refused before the timer, so this is unreachable in the
            // normal path and is an explicit integrity error rather than a panic.
            let built = match last {
                Some(built) => built,
                None => {
                    return Err(PipelineFailure::Storage(
                        layerfs_storage::StorageError::Integrity("no batch produced a root"),
                    ))
                }
            };
            metadata_emitted = counting.accepted();
            build_accept_ns = counting.nanos();
            built
        };
        let build_done = std::time::Instant::now();
        // The content stream, on the same operation. The metadata stream's `SaveSink`
        // is dropped above; the identities are the constructed ones.
        //
        // **The object is moved into the save, not copied into it.** This loop used
        // `cloned_object`, which deep-copies every canonical object - 502,912,427
        // bytes of copy for the 100,000-entry row, measured at 79,174,500 ns, inside
        // this timer. `Save::accept` takes the object by value, so the copy bought
        // nothing: the operation is handed the object the harness already held. The
        // drain leaves `content` empty and reusable, and the object count is kept
        // because the row publishes it and the loop is what consumes it.
        let content_started = std::time::Instant::now();
        if streaming {
            // One file at a time, from a reader. `construct_stream` buffers at most
            // `small_file_threshold_bytes` (131,072 B) for its threshold probe and then hands
            // `prefix.chain(source)` to the chunker, so a 100,000,000-byte anchor costs a window and the
            // chunker's own buffers - never the file. Objects go straight into the save operation:
            // construction emits to its consumer as it builds (`file/content.rs:247-260`), so there is no
            // object store in this path and no second pass over anything.
            for file in &plan.files {
                if file.size == 0 {
                    continue;
                }
                // The index the plan gave this file, re-derived exactly as the untimed pass derived it:
                // the tree's file serials are `2 + directories + index`.
                let index = u64::from(file.directory) * super::namespace_content::FILES_PER_DIRECTORY
                    + file.serial
                    - (2 + u64::from(declared.directories));
                let (bytes, canonical, counted, failure) = {
                    // `SaveSink` is the product's own C1-to-save adapter and is what the metadata
                    // stream above already feeds; the content stream takes the same path rather than a
                    // second one, so both halves of this row cross into storage the same way.
                    let mut handoff = operation.sink();
                    let mut meter = ContentMeter::new(&mut handoff);
                    let reader = std::io::BufReader::with_capacity(
                        STREAM_WINDOW_BYTES,
                        NoiseReader::new(file.size, seed ^ index.rotate_left(13)),
                    );
                    let (result, _) = Timing::disabled(
                        "content.construct",
                        |scope: &TimingScope<'_, Active>| {
                            construct_stream(
                                policy,
                                &capacities,
                                reader,
                                &mut meter,
                                scope.child("content"),
                            )
                        },
                    );
                    let built = result?;
                    (
                        built.logical_len,
                        meter.canonical_bytes,
                        meter.objects,
                        operation.take_failure(),
                    )
                };
                if let Some(error) = failure {
                    return Err(PipelineFailure::Storage(error));
                }
                // `content_bytes` is canonical, and the offered count is the meter's: one object per file
                // for a whole-file member, more for a chunked one, exactly as the store-held path counted.
                content_bytes = content_bytes.saturating_add(canonical);
                content_objects_offered += counted;
                        if bytes != file.size {
                    return Err(PipelineFailure::Storage(
                        layerfs_storage::StorageError::Integrity(
                            "streamed content length disagrees with the plan",
                        ),
                    ));
                }
            }
            content_rng_seed_ns = 0;
        } else {
        let offered = content.drain();
        content_objects_offered = offered.len() as u64;
        for object in offered {
            // The bytes the Store has to hold are counted on the way in, from the
            // objects themselves, exactly as the row always counted them.
            content_bytes = content_bytes.saturating_add(object.canonical_len() as u64);
            operation.accept(object)?;
        }
        }
        content_stream_ns = content_started.elapsed().as_nanos() as u64;
        let content_done = std::time::Instant::now();
        // `finish` takes no timing scope now, so no `storage.finish` node is created
        // and there is no node-creation cost to charge inside the finish span.
        let outcome = operation.finish()?;
        let finish_done = std::time::Instant::now();
        pack_bytes_written = store
            .diagnostics()
            .pack_write_bytes
            .saturating_sub(pack_bytes_before);
        accept_span_ns = accept_started.elapsed().as_nanos() as u64;
        establishment_ns = accept_started.duration_since(closure_started).as_nanos() as u64;
        build_span_ns = build_done.duration_since(accept_started).as_nanos() as u64;
        content_span_ns = content_done.duration_since(build_done).as_nanos() as u64;
        finish_span_ns = finish_done.duration_since(content_done).as_nanos() as u64;
        Ok::<_, PipelineFailure>((result, outcome))
    });
    // The store-held path knows its count before the timer; the streamed path only knows it after, when
    // the fixture has been built. One name, resolved once, from whichever path ran.
    let content_objects = if streaming {
        content_objects_offered
    } else {
        content_objects_store_held
    };


    let heap = instruments::heap_end();
    let measured_rss = rss_sampler.stop();
    let timing_bytes = crate::support::phases::timing_json_bytes();
    let (result, outcome) = match measured {
        Ok(value) => value,
        Err(error) => {
            return Ok(c1::unmeasured(
                &OpError::Product(format!("{error}")),
                gates,
            ))
        }
    };

    // The oracle: a second, unmeasured, byte-identical build into an authenticating
    // store. It **is batched too**, with the same `BUILD_BATCH_BINDINGS` workload
    // batches: the oracle must replay the operations the timer ran, not one
    // `build_filesystem` over the whole prepared tree (`fs::replay_build`), which is
    // a different sequence of operations. The batches and their prefix chains are
    // reused, so the oracle replays exactly what the timer ran.
    let mut store = TreeStore::new();
    let mut replay: Option<layerfs_content::FilesystemResult> = None;
    for (index, batch) in batches.iter().enumerate() {
        let base = replay.as_ref().map(|result| result.root);
        let input = FilesystemInput {
            base,
            scope,
            root_serial: ROOT_SERIAL,
            directories: &batch.directories,
            inodes: &batch.inodes,
            new_inodes: &batch.new_inodes,
            resources: FilesystemResources::default(),
        };
        let reader = PrefixProvider::new(&chain, &prefixes[index]);
        let built = {
            let mut objects = FilesystemObjects::new(&reader, &mut store);
            let ordering: Option<&mut dyn OrderingBacking> =
                Some(&mut oracle_backings[index] as &mut dyn OrderingBacking);
            if base.is_none() {
                build_filesystem(&mut objects, &input, ordering)
            } else {
                update_filesystem(&mut objects, &input, ordering)
            }
        };
        match built {
            Ok(built) => replay = Some(built),
            Err(error) => {
                return Ok(c1::unmeasured(
                    &OpError::Product(format!("oracle batch {index}: {error:?}")),
                    gates,
                ))
            }
        }
    }
    let replay = match replay {
        Some(replay) => replay,
        None => {
            return Ok(c1::unmeasured(
                &OpError::Io("oracle built no batch".to_string()),
                gates,
            ))
        }
    };
    let listing = fs::listings_match(&store, replay.root, &prepared)?;

    let inserted_bytes = content_bytes;
    context.trace.write_number(
        Kind::Counter,
        "pipeline.declared_files",
        config.files as i128,
        "files",
        "fixture recipe",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "pipeline.declared_directories",
        config.directories as i128,
        "directories",
        "fixture recipe",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "pipeline.bindings",
        prepared.bindings() as i128,
        "bindings",
        "fixture manifest",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "pipeline.declared_content_bytes",
        plan.total_bytes as i128,
        "bytes",
        "namespace_content::plan over ops::pipeline::declaration, the ported reference scenario total",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "pipeline.batches",
        batches.len() as i128,
        "operations",
        "PreparedTree::batches under BUILD_BATCH_BINDINGS, a workload constant",
    )?;
    // The build span's own split, read back out of the product's timing tree by
    // phase name. Each is a total over the row's batches, so the six together
    // plus the residual (`unreachable_parents`, the reducer's construction and
    // `register_values`, which no phase covers) are `pipeline.span_build_ns`.
    for (name, nanos) in build_phase_totals(&report) {
        context.trace.write_number(
            Kind::Counter,
            &format!("pipeline.build_{}_ns", name.replace('.', "_")),
            nanos as i128,
            "ns",
            "FilesystemPhases inside the measured closure",
        )?;
    }
    context.trace.write_number(
        Kind::Counter,
        "pipeline.build_accept_ns",
        build_accept_ns as i128,
        "ns",
        "CountingConsumer wall time around the product's accept, the other half of span_build_ns",
    )?;
    for (name, value) in [
        ("pipeline.validation_objects_read", build_work.validation.objects_read),
        ("pipeline.validation_read_waves", build_work.validation.read_waves),
        ("pipeline.validation_inode_demands", build_work.validation.inode_demands),
        ("pipeline.validation_inode_pages_read", build_work.validation.inode_pages_read),
        (
            "pipeline.validation_directory_pages_read",
            build_work.validation.directory_pages_read,
        ),
        ("pipeline.validation_entries_examined", build_work.validation.entries_examined),
        (
            "pipeline.validation_pages_allocation",
            build_work.sites.allocation,
        ),
        ("pipeline.validation_pages_prefetch", build_work.sites.prefetch),
        ("pipeline.validation_pages_bindings", build_work.sites.bindings),
        ("pipeline.validation_pages_aliases", build_work.sites.aliases),
        ("pipeline.validation_pages_cycles", build_work.sites.cycles),
        (
            "pipeline.validation_pages_reachability",
            build_work.sites.reachability,
        ),
        ("pipeline.build_objects_read", build_work.objects_read),
        ("pipeline.build_base_records_read", build_work.base_records_read),
    ] {
        context.trace.write_number(
            Kind::Counter,
            name,
            value as i128,
            "operations",
            "FilesystemUpdateCounters, accumulated over the row's batches",
        )?;
    }
    // The measured region's own RSS, in the four fields the receipt bundle declares
    // (`memory_cpu_space_support.md` section 3.2). Read beside
    // `resources.rss.process_peak_bytes`: the first is the region, the second is the
    // process lifetime, and the difference is the fixture the driver holds outside
    // the timer. Nothing here is pinned.
    for (key, value) in [
        ("pipeline.rss_phase_peak_bytes", measured_rss.phase_peak_bytes),
        ("pipeline.rss_phase_baseline_bytes", measured_rss.baseline_bytes),
        (
            "pipeline.rss_phase_incremental_bytes",
            measured_rss.incremental_peak_bytes,
        ),
        ("pipeline.rss_phase_final_bytes", measured_rss.final_bytes),
        ("pipeline.rss_samples", measured_rss.sample_count),
        (
            "pipeline.rss_maximum_gap_ns",
            measured_rss.maximum_sample_gap_ns,
        ),
        (
            "pipeline.rss_sampling_interval_ns",
            measured_rss.sampling_interval_ns,
        ),
        (
            "pipeline.rss_unavailable_samples",
            measured_rss.unavailable_samples,
        ),
    ] {
        context.trace.write_number(
            Kind::Counter,
            key,
            value as i128,
            "bytes",
            "RssSampler over the measured closure, 10 ms nominal interval",
        )?;
    }
    // The bundle's own fail-closed rule, published rather than left for a reader to
    // re-derive: `unavailable_samples == 0`, more than one sample, and no gap over
    // twice the declared interval. A bundle that fails it is an unusable phase peak
    // and this row says so instead of publishing the number alone.
    context.trace.write_number(
        Kind::Counter,
        "pipeline.rss_phase_peak_usable",
        i128::from(measured_rss.peak_is_usable()),
        "bool",
        "RssBundle::peak_is_usable, the fail-closed rule over the bundle above",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "pipeline.largest_batch_bindings",
        largest_batch as i128,
        "bindings",
        "the largest single operation's binding count",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "pipeline.chain_objects",
        chain_objects as i128,
        "objects",
        "the pre-timer reader chain",
    )?;
    // The chain's per-batch keys, in identities retained rather than bytes: this is
    // the row's fixture cost on the axis the redundant snapshots used to occupy.
    // Published as counts because a key set has no canonical byte figure, and a
    // reader who wants bytes has the heap window beside it.
    for (key, value, note) in [
        (
            "pipeline.prefix_keys_total",
            prefix_keys_total as i128,
            "identities retained across every batch's prefix keys, the one chain counted once",
        ),
        (
            "pipeline.prefix_keys_largest",
            prefix_keys_largest as i128,
            "the largest single batch's prefix, in identities",
        ),
        (
            "pipeline.prefix_objects_served",
            prefix_served as i128,
            "objects the timed pass read out of the chain through a batch's own prefix",
        ),
        (
            "pipeline.prefix_objects_refused",
            prefix_refused as i128,
            "demands refused as outside the batch's prefix, which is how the keys bite",
        ),
    ] {
        context.trace.write_number(Kind::Counter, key, value, "objects", note)?;
    }
    context.trace.write_number(
        Kind::Counter,
        "pipeline.content_objects",
        content_objects as i128,
        "objects",
        "constructed before the timer, accepted inside it",
    )?;
    // The stream's own count, taken where it was consumed. `pipeline.content_objects`
    // is read off the store before the timer; this is read off the drain inside it,
    // and the gate below is what says the stream was offered whole rather than
    // partly - a drain that silently dropped objects would otherwise show up only as
    // a smaller `content_bytes`.
    context.trace.write_number(
        Kind::Counter,
        "pipeline.content_objects_offered",
        content_objects_offered as i128,
        "objects",
        "moved into the save inside the timer by the content drain",
    )?;
    // The untimed half, published in its two named parts. Outside the row's
    // formula by construction; `construct_ns + construct_noise_ns +
    // pipeline.operation_work_ns` is the figure that can be laid against a timer
    // which includes its own construction (see this round's pre-registration).
    context.trace.write_number(
        Kind::Counter,
        "pipeline.construct_ns",
        construct_ns as i128,
        "ns",
        "untimed: construct_bytes over every planned file, outside the formula",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "pipeline.construct_noise_ns",
        construct_noise_ns as i128,
        "ns",
        "untimed: fixture::noise byte generation, outside the formula",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "pipeline.metadata_objects",
        metadata_emitted as i128,
        "objects",
        "CountingConsumer on the SaveSink path, measured phase",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "pipeline.inserted",
        i128::from(outcome.inserted),
        "objects",
        "WriteOutcome.inserted",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "pipeline.reused",
        i128::from(outcome.reused),
        "objects",
        "WriteOutcome.reused",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "pipeline.content_bytes",
        inserted_bytes as i128,
        "bytes",
        "canonical bytes of the content objects accepted by the save",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "pipeline.objects_emitted",
        result.counters.objects.objects_emitted as i128,
        "objects",
        "ObjectWork.objects_emitted",
    )?;
    // The harness's own spans over the measured closure. Each is an `Instant` pair
    // taken by this driver, not a product timing node. The removed `SaveProfile`
    // split (`pipeline.profile_*`, `pipeline.diag_*`), the teardown it named
    // (`pipeline.teardown_ns`) and the formula that subtracted it
    // (`pipeline.operation_work_ns`) have no current source and are not emitted:
    // see `ops::store::UNAVAILABLE_COUNTERS`. Diagnostics only: none of these is
    // pinned in `tests/golden/expected.tsv`, because a wall-clock observation cannot
    // be a frozen constant.
    for (key, value) in [
        ("pipeline.accept_span_ns", accept_span_ns),
        ("pipeline.span_build_ns", build_span_ns),
        ("pipeline.span_content_ns", content_span_ns),
        ("pipeline.span_finish_ns", finish_span_ns),
        // `establishment_ns + accept_span_ns` is the whole measured closure.
        ("pipeline.establishment_ns", establishment_ns),
    ] {
        context.trace.write_number(
            Kind::Counter,
            key,
            value as i128,
            "ns",
            "harness Instant spans over the measured closure; an aggregate over the operation, never a span per object",
        )?;
    }
    // The save's own pack and record counters, from `WriteOutcome` and, for the
    // pack bytes, from the handle's cumulative `Diagnostics` across the one save
    // (`ops::store::COUNTER_MAPPINGS`). `pipeline.statements`,
    // `pipeline.presence_queries` and `pipeline.pack_appends` have no current
    // source and are not emitted.
    for (key, value, unit) in [
        ("pipeline.packs_created", outcome.packs, "packs"),
        // The physical pack body bytes this save's publications acknowledged.
        ("pipeline.pack_bytes_written", pack_bytes_written, "bytes"),
        ("pipeline.full_records", outcome.full_records, "objects"),
        ("pipeline.prefix_records", outcome.prefix_records, "objects"),
    ] {
        context.trace.write_number(
            Kind::Counter,
            key,
            i128::from(value),
            unit,
            "WriteOutcome's own pack and record counters, and Diagnostics.pack_write_bytes across the save",
        )?;
    }
    context.trace.write_number(
        Kind::Counter,
        "timing_json_bytes",
        timing_bytes as i128,
        "bytes",
        "product timing.json, byte-verbatim",
    )?;
    context.trace.write_number(
        Kind::Resource,
        "heap.peak_incremental_bytes",
        heap.peak_incremental_bytes as i128,
        "bytes",
        "counting GlobalAlloc, measured phase",
    )?;

    gates.push(c1::completeness_gate(&report, "g7.tree-complete"));
    crate::workload::expected::publish(
        context.trace,
        "filesystem_root",
        &result.root.0.to_string(),
    )?;
    gates.push(gates::require(
        GateClass::Correctness,
        "g1.o1-replay-root",
        replay.root.0 == result.root.0,
        &format!("replay root {}", replay.root.0),
        &format!("measured root {}", result.root.0),
    ));
    // The gate this row exists for: every existing pipeline gate is structural, so
    // a row that saved empty inodes would pass all of them.
    gates.push(gates::require(
        GateClass::Correctness,
        "g1.o5-content-bytes",
        inserted_bytes >= plan.total_bytes,
        &format!(
            "{inserted_bytes} canonical bytes inserted against {} declared",
            plan.total_bytes
        ),
        "the content the row accepted is at least the bytes the case declares",
    ));
    // The chain's reader is one complete chain plus one identity set per batch, so
    // the check that the substitution is faithful is that every batch but the first
    // was served objects **from its own prefix**. Batch 0 has no base and is served
    // nothing by construction; a later batch served nothing read no object it did not
    // emit, which would make `chain_objects` a statement about a chain nothing read.
    gates.push(gates::require(
        GateClass::Mechanism,
        "g6.prefix-keys",
        prefix_batches_served == batches.len().saturating_sub(1) as u64,
        &format!(
            "{prefix_batches_served} of {} batches served from their own prefix; \
             {prefix_served} objects served, {prefix_refused} refused as outside it",
            batches.len().saturating_sub(1)
        ),
        "every batch after the first reads its base out of the chain through its own \
         prefix keys",
    ));
    // The drain must have offered every constructed object. This is the reading that
    // separates "the copy was removed" from "objects stopped being offered", which
    // `content_bytes` alone would show as a smaller number only if the last objects
    // happened to be missing.
    // Under the streaming switch `content_objects` **is** the offered count, so the gate cannot be the
    // tautology it is on the store-held path. What it holds is the shape instead: every non-empty file
    // contributed at least one object, and the canonical bytes the save was handed stay inside the
    // envelope the logical bytes allow (an object carries framing, so canonical is above logical and
    // bounded above by logical plus one frame per object).
    let planned_files = planned_content_files(&plan);
    let envelope_ceiling = plan
        .total_bytes
        .saturating_add(content_objects_offered.saturating_mul(CONTENT_FRAME_CEILING_BYTES));
    gates.push(gates::require(
        GateClass::Mechanism,
        "g6.content-complete",
        content_objects_offered >= planned_files && content_bytes <= envelope_ceiling,
        &format!(
            "{content_objects_offered} objects offered for {planned_files} non-empty files; \
             {content_bytes} canonical bytes against a ceiling of {envelope_ceiling}"
        ),
        "every planned file contributed at least one object and the canonical bytes fit the envelope",
    ));
    gates.push(gates::require(
        GateClass::Mechanism,
        "g2.handoff",
        metadata_emitted > 0 && outcome.inserted + outcome.reused > 0,
        &format!(
            "{metadata_emitted} metadata objects crossed the handoff; {content_objects} content \
             objects were accepted; save acknowledged {} inserted + {} reused",
            outcome.inserted, outcome.reused
        ),
        "both streams reached the save operation and the save acknowledged them",
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
    // The measured closure closed its own Store. The sidecar gate is read on the
    // sealed file, so the sample is reopened and sealed here, outside the timer.
    match c2::open_untimed(&sample) {
        Ok(store) => gates.extend(c2::sealed_sidecar_gates(store, &sample)),
        Err(error) => gates.push(Gate::incomplete(
            GateClass::Cleanup,
            "g5.no-sidecars",
            &format!("{}: reopen for seal refused: {error}", sample.display()),
            "no -wal, -shm or -journal sidecar",
        )),
    }
    gates.push(gates::swap_gate(instruments::swaps()));

    Ok(OpOutcome {
        gates,
        notes: vec![
            store::profile_note(),
            store::unavailable_note(&[
                "pipeline.commits",
                "pipeline.statements",
                "pipeline.presence_queries",
                "pipeline.pack_appends",
                "pipeline.profile_*",
                "pipeline.diag_*",
                "pipeline.stored_records",
                "pipeline.teardown_ns",
                "pipeline.operation_work_ns",
                "pipeline.span_finish_child_ns",
            ]),
            format!("pipeline_op: {op:?}"),
            format!("declared_files: {}", config.files),
            format!("declared_directories: {}", config.directories),
            format!("declared_content_bytes: {}", plan.total_bytes),
            "measured_region: Store open + build_filesystem + content accept + save + acknowledgement"
                .to_string(),
            "content: constructed before the timer, accepted inside it (C2's supplied-object rule)"
                .to_string(),
            "handoff: layerfs_storage::SaveSink, the product's own C1-to-save adapter".to_string(),
        ],
    })
}
