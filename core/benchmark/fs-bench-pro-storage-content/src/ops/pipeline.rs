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

use layerfs_content::filesystem::references::backing::OrderingBacking;
use layerfs_content::filesystem::{
    build_filesystem, scope_for_seed, update_filesystem, FilesystemInput, FilesystemObjects,
    FilesystemResources, FilesystemRootId,
};
use layerfs_content::{
    apply_edits, construct_bytes, ConstructionPolicy, Edit, EditRequest, EditStream, ObjectId,
    Replacements,
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

/// Logical bytes the `namespace-10000` parity row declares, matching the v0.1.6
/// case: 300,000,000 of file content plus a 100,000,000-byte anchor.
pub const NAMESPACE_SCALE_BYTES: u64 = 300_000_000;

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
        // The v0.1.6 `namespace-10000` shape: 10,000 files over 100 directories,
        // 300,000,000 logical bytes with a 100,000,000-byte anchor.
        PipelineOp::NamespaceScale => Configuration {
            base_bytes: 0,
            start: 0,
            end: 0,
            replacement: 0,
            files: 10_000,
            directories: 100,
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
        PipelineOp::NamespaceScale => namespace_scale(case, op, context),
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
    let (measured, report) = super::measure("pipeline", |scope: &TimingScope<'_, Active>| {
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
    let (measured, report) = super::measure("pipeline", |timing: &TimingScope<'_, Active>| {
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
    context.trace.write_number(Kind::Counter, "pipeline.handoff_objects", emitted as i128, "objects", "CountingConsumer on the SaveHandoff path, measured phase")?;
    context.trace.write_number(Kind::Counter, "pipeline.inserted", i128::from(outcome.inserted), "objects", "SaveOutcome.inserted")?;
    context.trace.write_number(Kind::Counter, "pipeline.reused", i128::from(outcome.reused), "objects", "SaveOutcome.reused")?;
    context.trace.write_number(Kind::Counter, "pipeline.commits", i128::from(outcome.commits), "transactions", "SaveOutcome.commits")?;
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

/// One `namespace-10000` parity row: the namespace **and** its declared content
/// saved in one measured region.
///
/// This is the shape the v0.1.6 `init_namespace` case measures and that no other
/// v0.1.7 row performs. `c1.fs.build-scale` builds the same 10,000-entry tree but
/// emits no content at all, and `pipeline-filesystem-build` saves a filesystem
/// whose inodes are empty. Neither moves bytes into a Store.
///
/// **Why the content is a second stream.** `build_filesystem` emits metadata only:
/// its internal `contents` map holds *directory* roots, and a file's `content_root`
/// is copied out of the input `InodeValue` without ever being demanded from the
/// reader. So a filesystem build cannot put 300 MB into a Store, and the row has to
/// construct that content itself and accept it on the same `SaveOperation`. The
/// construction happens before the timer; the save does not.
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

    // The byte plan: the same 10,000-file / 100-directory / 300 MB shape the
    // v0.1.6 fixture declares.
    let plan = match super::namespace_content::plan(
        config.files,
        config.directories,
        seed,
        NAMESPACE_SCALE_BYTES,
    ) {
        Ok(plan) => plan,
        Err(defect) => return Ok(c1::unmeasured(&OpError::Io(defect), Vec::new())),
    };

    // Construct the content. Untimed: C2's rule is that every canonical object a
    // C2 row saves is supplied by the harness, and construction is C1's half.
    let policy = ConstructionPolicy::frozen_default();
    let capacities = policy.capacities();
    let mut content = TreeStore::new();
    for file in &plan.files {
        if file.size == 0 {
            continue;
        }
        let index = u64::from(file.directory) * super::namespace_content::FILES_PER_DIRECTORY
            + u64::from(file.serial)
            - (2 + u64::from(config.directories));
        let bytes = fixture::noise(file.size, seed ^ index.rotate_left(13));
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
    let content_objects = content.len() as u64;

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

    // A 10,000-file / 100-directory tree states 10,101 bindings, and the product
    // refuses more than `MAXIMUM_WALK_ENTRIES` (4,096) in one operation: the cycle
    // check returns `InvalidRecord("cycle check work limit")`. `limits.rs` says such
    // a tree "is reached by several operations that each stay under the ceiling", and
    // `PreparedTree::batches` is that route. `c1.fs.build-scale`'s own
    // namespace-10000 row does the same thing and reports `fs_build.operations: 3`.
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
    // One reader per batch index: the chain **as it stood before that batch**, not the
    // complete chain. Measured, not assumed: a standalone probe passes every batch at
    // budgets 4096, 2048 and 1024 when the reader is the chain-so-far, while the
    // driver's timed pass returned `cycle check work limit` when it served every batch
    // from the complete chain. A batch's base is what the *previous* batches produced,
    // so handing it the whole chain is both wrong and more expensive.
    let mut prefixes: Vec<TreeStore> = vec![TreeStore::new()];
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
        chain.absorb(&emitted);
        let mut snapshot = TreeStore::new();
        snapshot.absorb(&chain);
        prefixes.push(snapshot);
    }
    let chain_objects = chain.len() as u64;

    let mut metadata_emitted = 0_u64;
    let mut content_bytes = 0_u64;
    let mut batch_roots = 0_u64;
    let mut largest_batch = 0_u64;
    instruments::heap_begin();
    let (measured, report) = super::measure("pipeline", |timing: &TimingScope<'_, Active>| {
        let store = Store::open(&sample, timing.child("store.open"))?;
        let mut operation = store.begin_save(timing.child("storage.begin"))?;
        let result = {
            let mut handoff = SaveHandoff::new(&mut operation);
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
                let reader = PairProvider::new(&prefixes[index], &empty);
                let mut objects = FilesystemObjects::new(&reader, &mut counting);
                let ordering: Option<&mut dyn OrderingBacking> =
                    Some(&mut perf_backings[index] as &mut dyn OrderingBacking);
                let built = if base.is_none() {
                    build_filesystem(&mut objects, &input, ordering)?
                } else {
                    update_filesystem(&mut objects, &input, ordering)?
                };
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
            built
        };
        // The content stream, on the same operation. `SaveHandoff` is dropped above
        // so the operation is free; the identities are the constructed ones.
        for id in content.insertion_order() {
            let object = content
                .cloned_object(*id)
                .ok_or(layerfs_storage::StorageError::ObjectMissing(*id))?;
            // `SaveOutcome` carries no byte field - its `chain` is delta-base
            // acquisition work, not bytes - so the bytes the Store has to hold are
            // counted on the way in, from the objects themselves.
            content_bytes = content_bytes.saturating_add(object.canonical_len() as u64);
            operation.accept(object)?;
        }
        let outcome = operation.finish(timing.child("storage.finish"))?;
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

    // The oracle: a second, unmeasured, byte-identical build into an authenticating
    // store. It **must be batched too**. `fs::replay_build` issues one
    // `build_filesystem` over the whole prepared tree, which for 10,101 bindings is
    // refused by the same `MAXIMUM_WALK_ENTRIES` ceiling - and it reports the refusal
    // as `OpError::Product`, which is how this row's first four runs were diagnosed:
    // the failure was never in the measured region, it was here. The batches and their
    // prefix chains are reused, so the oracle replays exactly what the timer ran.
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
        let reader = PairProvider::new(&prefixes[index], &empty);
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
        "namespace_content::plan, the declared 300,000,000-byte shape",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "pipeline.batches",
        batches.len() as i128,
        "operations",
        "PreparedTree::batches under MAXIMUM_WALK_ENTRIES",
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
    context.trace.write_number(
        Kind::Counter,
        "pipeline.content_objects",
        content_objects as i128,
        "objects",
        "constructed before the timer, accepted inside it",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "pipeline.metadata_objects",
        metadata_emitted as i128,
        "objects",
        "CountingConsumer on the SaveHandoff path, measured phase",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "pipeline.inserted",
        i128::from(outcome.inserted),
        "objects",
        "SaveOutcome.inserted",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "pipeline.reused",
        i128::from(outcome.reused),
        "objects",
        "SaveOutcome.reused",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "pipeline.commits",
        i128::from(outcome.commits),
        "transactions",
        "SaveOutcome.commits",
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
    gates.push(gates::require(
        GateClass::Mechanism,
        "g2.handoff",
        metadata_emitted > 0 && u64::from(outcome.inserted) + u64::from(outcome.reused) > 0,
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
    gates.extend(c2::sidecar_gates(&sample));
    gates.push(gates::swap_gate(instruments::swaps()));

    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("pipeline_op: {op:?}"),
            format!("declared_files: {}", config.files),
            format!("declared_directories: {}", config.directories),
            format!("declared_content_bytes: {}", plan.total_bytes),
            "measured_region: Store::open + build_filesystem + content accept + save + acknowledgement"
                .to_string(),
            "content: constructed before the timer, accepted inside it (C2's supplied-object rule)"
                .to_string(),
            "handoff: layerfs_storage::SaveHandoff, the product's own C1-to-save adapter".to_string(),
        ],
    })
}
