//! C1 shape drivers: construction, chunk census, localized edit, transition.
//!
//! Every driver here follows the one rule stated in [`super`]: the timed phase runs
//! with a non-retaining consumer and the oracle is a second, unmeasured,
//! byte-identical operation. The two roots must agree, so a replay that failed to
//! reproduce the measured operation is caught rather than silently verified.

use std::io::{Cursor, Write};
use std::path::Path;

use layerfs_content::{
    apply_edits, construct_bytes, construct_stream, ConstructionPolicy, DiscardingConsumer,
    Edit, EditRequest, FileContent, FileView, ObjectId,
};
use layerfs_telemetry::timer::{Active, Timing, TimingScope};

use super::{seed_of, OpContext, OpError, OpOutcome, Phase};
use crate::fixture::{self, structured, STRUCTURED_ZERO_REGION};
use crate::gates::{self, Gate, GateClass};
use crate::registry::{Case, CountOp, EditOp, Route, Transition};
use crate::support::instruments;
use crate::support::trace::Kind;
use crate::workload::oracle::{self, Expectation};
use crate::workload::artifact::{Artifact, Member};
use crate::workload::edits::{Edits, Parts};
use crate::workload::providers::{PairProvider, TreeStore};

/// The 4 KiB replacement every edit family uses.
pub const REPLACEMENT_LEN: u64 = 4_096;

/// Writes the product's own timing tree, byte-verbatim, as `timing.json`.
pub(super) fn write_timing(directory: &Path, report: &layerfs_telemetry::timer::TimingReport) -> Result<u64, OpError> {
    let path = directory.join("timing.json");
    let mut file = std::fs::File::create(&path)
        .map_err(|error| OpError::Io(format!("{}: {error}", path.display())))?;
    report
        .write_json(&mut file)
        .map_err(|error| OpError::Io(format!("{}: {error}", path.display())))?;
    file.flush()
        .map_err(|error| OpError::Io(format!("{}: {error}", path.display())))?;
    Ok(file
        .metadata()
        .map(|metadata| metadata.len())
        .unwrap_or(0))
}

/// Records the product report's completeness as G7 evidence.
pub(super) fn completeness_gate(report: &layerfs_telemetry::timer::TimingReport, id: &'static str) -> Gate {
    if report.root().is_none() {
        return Gate::incomplete(
            GateClass::TimingPurity,
            id,
            "disabled report: no root",
            "report.is_incomplete() == false",
        );
    }
    gates::timing_purity(
        report.is_incomplete(),
        report.node_count() as u64,
        report.levels() as u64,
    )
}

/// The representation a root actually has, read through the public read path.
///
/// `FileView` is the product's own logical view of a root: it classifies the root
/// without the harness decoding a mapping page by hand, and it is a *read* path,
/// distinct from the operation that produced the root.
pub(super) fn representation_of(store: &dyn layerfs_content::AuthenticatedObjects, root: ObjectId) -> Result<FileContent, OpError> {
    let (result, _) = Timing::disabled("oracle.view", |scope: &TimingScope<'_, Active>| {
        let view = FileView::open(store, root, scope.child("view"))?;
        let logical_len = view.logical_len();
        Ok::<_, layerfs_content::ContentError>(match view.file_state()? {
            Some(state) => FileContent::Chunked(state),
            None => FileContent::WholeFile { logical_len },
        })
    });
    result.map_err(|error| OpError::Product(format!("{error:?}")))
}

/// Extents (chunks) the chunked representation of `root` holds; `0` for whole-file.
pub(super) fn extent_count_of(store: &dyn layerfs_content::AuthenticatedObjects, root: ObjectId) -> Result<u64, OpError> {
    match representation_of(store, root)? {
        FileContent::WholeFile { .. } => Ok(0),
        FileContent::Chunked(state) => Ok(state.extent_count),
    }
}

/// Closes a row that could not be measured, without pretending it passed.
pub(super) fn unmeasured(error: &OpError, gates: Vec<Gate>) -> OpOutcome {
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

/// C1-1/C1-2: complete-file construction through one of the two entry points.
pub fn construct(
    case: &Case,
    route: Route,
    context: &mut OpContext<'_>,
) -> Result<OpOutcome, OpError> {
    context.create_output()?;
    let seed = seed_of(case.id);
    let bytes = fixture::noise(case.bytes, seed);
    let expectation = Expectation::of(&bytes);
    let policy = ConstructionPolicy::frozen_default();
    let capacities = policy.capacities();
    let mut gates = Vec::new();

    instruments::heap_begin();
    let (measured, report) = super::measure("c1.construct", |scope: &TimingScope<'_, Active>| {
        let mut consumer = DiscardingConsumer::new();
        match route {
            Route::Bytes => construct_bytes(
                policy,
                &capacities,
                &bytes,
                &mut consumer,
                scope.child("content"),
            )
            .map(|file| (file, consumer)),
            Route::Stream => construct_stream(
                policy,
                &capacities,
                Cursor::new(bytes.as_slice()),
                &mut consumer,
                scope.child("content"),
            )
            .map(|file| (file, consumer)),
        }
    });
    let heap = instruments::heap_end();
    let timing_bytes = crate::support::phases::timing_json_bytes();
    let (measured_file, measured_consumer) = match measured {
        Ok(value) => value,
        Err(error) => {
            return Ok(unmeasured(
                &OpError::Product(format!("{error:?}")),
                gates,
            ))
        }
    };

    context.trace.write_number(
        Kind::Counter,
        "construct.objects",
        measured_consumer.objects() as i128,
        "objects",
        "DiscardingConsumer.objects, measured phase",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "construct.canonical_bytes",
        measured_consumer.canonical_bytes() as i128,
        "bytes",
        "DiscardingConsumer.canonical_bytes, measured phase",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "construct.peak_object_bytes",
        measured_consumer.peak_object_bytes() as i128,
        "bytes",
        "DiscardingConsumer.peak_object_bytes, measured phase",
    )?;
    context.trace.write_number(
        Kind::Resource,
        "heap.peak_incremental_bytes",
        heap.peak_incremental_bytes as i128,
        "bytes",
        "counting GlobalAlloc, measured phase",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "timing_json_bytes",
        timing_bytes as i128,
        "bytes",
        "product timing.json, byte-verbatim",
    )?;
    context
        .trace
        .write(Kind::Receipt, "route", &format!("{route:?}"), "", "registry row")?;

    gates.push(completeness_gate(&report, "g7.tree-complete"));
    gates.push(gates::require(
        GateClass::Resource,
        "g4.peak-object",
        measured_consumer.peak_object_bytes() <= 16 * 1024 * 1024,
        &format!("{} bytes", measured_consumer.peak_object_bytes()),
        "<= MAX_CANONICAL_OBJECT_BYTES (16777216)",
    ));
    gates.push(gates::swap_gate(instruments::swaps()));

    // Oracle: a second, unmeasured, byte-identical construction into a store that
    // authenticates every read.
    let mut store = TreeStore::new();
    let (replay, replay_report) = Timing::disabled("oracle.replay", |scope: &TimingScope<'_, Active>| {
        match route {
            Route::Bytes => construct_bytes(policy, &capacities, &bytes, &mut store, scope.child("content")),
            Route::Stream => construct_stream(
                policy,
                &capacities,
                Cursor::new(bytes.as_slice()),
                &mut store,
                scope.child("content"),
            ),
        }
    });
    let _ = replay_report;
    let replay = match replay {
        Ok(file) => file,
        Err(error) => return Ok(unmeasured(&OpError::Product(format!("{error:?}")), gates)),
    };
    // O1 for real: the measured root is published so the pinned-constant gate in
    // `main` can compare it with `tests/golden/expected.tsv`. The replay gate below
    // is self-consistency and stays as a second, weaker check.
    crate::workload::expected::publish(context.trace, "file_root", &measured_file.root.to_string())?;
    gates.push(gates::require(
        GateClass::Correctness,
        "g1.o1-replay-root",
        replay.root == measured_file.root,
        &format!("replay root {}", replay.root),
        &format!("measured root {}", measured_file.root),
    ));
    gates.push(gates::require(
        GateClass::Correctness,
        "g1.o1-logical-length",
        measured_file.logical_len == case.bytes,
        &format!("{} bytes", measured_file.logical_len),
        &format!("{} bytes declared by the tier", case.bytes),
    ));
    let (readback, read_scope_report) = Timing::disabled("oracle.readback", |scope: &TimingScope<'_, Active>| {
        oracle::read_back(&store, replay.root, &expectation, scope.child("content"))
    });
    let _ = read_scope_report;
    match readback {
        Ok(readback) => {
            context.trace.write(
                Kind::Oracle,
                "readback.digest",
                &readback.digest,
                "sha256",
                "oracle read-back through the public read path",
            )?;
            gates.push(gates::require(
                GateClass::Correctness,
                "g1.o2-readback",
                readback.matches(),
                &format!("{} bytes, {} bytes read", readback.bytes, readback.expected_bytes),
                &format!("{} bytes, sha256 {}", readback.expected_bytes, readback.expected_digest),
            ));
        }
        Err(error) => gates.push(Gate::incomplete(
            GateClass::Correctness,
            "g1.o2-readback",
            &format!("read-back failed: {error:?}"),
            "readback equals input",
        )),
    }

    // G2: the route the row declares is the route the counters show.
    let extents = extent_count_of(&store, replay.root)?;
    context.trace.write_number(
        Kind::Counter,
        "construct.extents",
        extents as i128,
        "extents",
        "FileState.extent_count decoded from the result root",
    )?;
    let expected_chunked = case.bytes >= policy.small_file_threshold_bytes();
    gates.push(gates::require(
        GateClass::Mechanism,
        "g2.representation",
        expected_chunked == (extents > 0),
        &format!("{extents} extents at {} bytes", case.bytes),
        &format!(
            "cutoff {} selects {}",
            policy.small_file_threshold_bytes(),
            if expected_chunked { "chunked" } else { "whole-file" }
        ),
    ));

    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("route: {route:?}"),
            format!("fixture_seed: {seed}"),
            format!("heap_charged_bytes: {}", heap.charged_bytes),
            format!("heap_allocations: {}", heap.allocations),
            format!("cache_state: warm-in-process-fixture; fixture bytes are read before the timed region"),
            "oracle_phase: separate-unmeasured-replay".to_string(),
        ],
    })
}

/// Builds a base into `store` without timers. Setup, never measurement.
pub(super) fn build_base(
    policy: ConstructionPolicy,
    capacities: &layerfs_content::ConstructionCapacities,
    bytes: &[u8],
    store: &mut TreeStore,
) -> Result<layerfs_content::ConstructedFile, OpError> {
    let (result, _) = Timing::disabled("setup.base", |scope: &TimingScope<'_, Active>| {
        construct_bytes(policy, capacities, bytes, store, scope.child("content"))
    });
    result.map_err(|error| OpError::Product(format!("{error:?}")))
}

/// The range one edit op rewrites, in the base's coordinates.
fn edit_range(op: EditOp, base_len: u64) -> (u64, u64, u64) {
    let middle = base_len / 2;
    match op {
        EditOp::OverwriteHead => (0, REPLACEMENT_LEN, REPLACEMENT_LEN),
        EditOp::OverwriteMiddle => (middle, middle + REPLACEMENT_LEN, REPLACEMENT_LEN),
        EditOp::OverwriteTail => (
            base_len - REPLACEMENT_LEN,
            base_len,
            REPLACEMENT_LEN,
        ),
        EditOp::InsertMiddle | EditOp::Grow => (middle, middle, REPLACEMENT_LEN),
        EditOp::DeleteMiddle => (middle, middle + REPLACEMENT_LEN, 0),
        EditOp::Shrink => (base_len - REPLACEMENT_LEN, base_len, 0),
        EditOp::AppendTail | EditOp::ZeroExtend => (base_len, base_len, REPLACEMENT_LEN),
        EditOp::PrependHead => (0, 0, REPLACEMENT_LEN),
        EditOp::Truncate => (base_len / 4 * 3, base_len, 0),
    }
}

/// Replacement bytes for one op: zeros for the two zero-filling ops, noise else.
fn edit_replacement(op: EditOp, seed: u64) -> Vec<u8> {
    match op {
        EditOp::ZeroExtend => fixture::zeros(REPLACEMENT_LEN),
        EditOp::DeleteMiddle | EditOp::Shrink | EditOp::Truncate => Vec::new(),
        _ => fixture::noise(REPLACEMENT_LEN, seed ^ 0x2222),
    }
}

/// C1-3: one fixed 64 KiB overwrite classified by its chunk-count effect.
///
/// **Phase split.** The base is a fixture, not the measurement: the row measures
/// `apply_edits` over a 64 KiB window, and building the base means generating the
/// file, running CDC over it and hashing 500 MiB to state the expectation the
/// oracle compares against. All of that is acquisition, so the registry declares
/// `Preparation::ObjectSet` and the base is acquired once.
pub fn chunk_count(
    case: &Case,
    op: CountOp,
    context: &mut OpContext<'_>,
) -> Result<OpOutcome, OpError> {
    match context.phase {
        Phase::Prepare => chunk_count_prepare(case, op, context),
        Phase::Perf => chunk_count_perf(case, op, context),
        Phase::Verify => Err(OpError::Io(
            "c1.cdc.chunk-count declares an in-process oracle and has no deferred verification phase"
                .to_string(),
        )),
    }
}

/// The fixture one chunk-count row is built from: the declared offsets, and the
/// base shape the row's own direction needs.
fn chunk_count_fixture(case: &Case, op: CountOp, seed: u64) -> (Vec<u8>, u64, u64, u64) {
    let start = crate::families::c1_cdc::START;
    let len = crate::families::c1_cdc::LEN;
    let end = start + len;
    // The specification fixes the offsets, not the base content. The two
    // directions need two different bases at the same offsets: a zero run to grow
    // the chunk count out of, and a chunk-dense window to shrink back into. The
    // choice is declared here rather than hidden in a driver.
    let base = match op {
        CountOp::Decrease => {
            let mut base = structured(case.bytes, seed, false);
            base[start as usize..end as usize].copy_from_slice(&fixture::chunk_dense(len));
            base
        }
        _ => structured(case.bytes, seed, true),
    };
    (base, start, end, len)
}

/// The replacement bytes one chunk-count row applies.
fn chunk_count_replacement(op: CountOp, base: &[u8], start: u64, end: u64, len: u64, seed: u64) -> Vec<u8> {
    match op {
        CountOp::Preserve => base[start as usize..end as usize].to_vec(),
        CountOp::Increase => fixture::noise(len, seed ^ 0x1111),
        CountOp::Decrease => fixture::zeros(len),
    }
}

/// C1-3 `prepare`: acquire the base once, with the expectation its oracle needs.
fn chunk_count_prepare(
    case: &Case,
    op: CountOp,
    context: &mut OpContext<'_>,
) -> Result<OpOutcome, OpError> {
    let directory = context.artifact_directory()?;
    let seed = seed_of(case.id);
    let policy = ConstructionPolicy::frozen_default();
    let capacities = policy.capacities();
    let (base, start, end, len) = chunk_count_fixture(case, op, seed);
    let replacement = chunk_count_replacement(op, &base, start, end, len, seed);
    let mut store = TreeStore::new();
    let base_file = build_base(policy, &capacities, &base, &mut store)?;
    let expectation = Expectation::spliced(&base, start, end, &replacement);
    let ids = store.insertion_order().to_vec();
    let mut artifact = Artifact::new(store);
    artifact.members.push(Member {
        ids,
        root: base_file.root,
        expectation: None,
    });
    artifact.values.set_scalar("base_len", base.len() as u64);
    artifact.values.set_scalar("start", start);
    artifact.values.set_scalar("end", end);
    artifact.values.set_scalar("edit_len", len);
    artifact.values.set_expectation("result", expectation);
    let written = artifact
        .write(&directory)
        .map_err(|error| OpError::Io(format!("{}: {error}", directory.display())))?;
    artifact
        .seal(&directory, case.id)
        .map_err(|error| OpError::Io(format!("{}: {error}", directory.display())))?;
    context.trace.write_number(
        Kind::Counter,
        "prepare.objects",
        written as i128,
        "objects",
        "distinct canonical objects the artifact holds",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "prepare.base_bytes",
        base.len() as i128,
        "bytes",
        "declared base bytes",
    )?;
    Ok(OpOutcome {
        gates: Vec::new(),
        notes: vec![
            format!("prepare_op: {op:?}"),
            format!("prepare_base_bytes: {}", base.len()),
            format!("artifact: {}", directory.display()),
            "acquisition: not a measured phase; charged to acquisition_wall_ns".to_string(),
        ],
    })
}

/// Reads one logical range of a prepared base through the product's read path.
///
/// Setup, never measurement: it recovers the fixture bytes a row's own recipe
/// needs and the artifact does not persist, and it runs before the timer opens.
/// The result feeds the mutation's *input*, so a wrong read is caught by the O2
/// read-back rather than hidden.
fn base_window(
    store: &TreeStore,
    root: ObjectId,
    start: u64,
    end: u64,
) -> Result<Vec<u8>, OpError> {
    let mut window: Vec<u8> = Vec::with_capacity((end - start) as usize);
    let (result, _) = Timing::disabled("setup.window", |scope: &TimingScope<'_, Active>| {
        layerfs_content::read_range(store, root, start..end, &mut window, scope.child("content"))
    });
    result.map_err(|error| OpError::Product(format!("{error:?}")))?;
    Ok(window)
}

/// C1-3 `perf`: the measured edit against the prepared base.
fn chunk_count_perf(
    case: &Case,
    op: CountOp,
    context: &mut OpContext<'_>,
) -> Result<OpOutcome, OpError> {
    context.create_output()?;
    let seed = seed_of(case.id);
    let policy = ConstructionPolicy::frozen_default();
    let capacities = policy.capacities();
    let mut gates = Vec::new();

    let directory = context.artifact_directory()?;
    let artifact = Artifact::read(&directory)
        .map_err(|error| OpError::Io(format!("{}: {error}", directory.display())))?;
    let base_member = artifact.base().map_err(OpError::Io)?;
    let scalar = |name: &str| -> Result<u64, OpError> {
        artifact
            .values
            .scalar(name)
            .ok_or_else(|| OpError::Io(format!("the artifact declares no scalar {name:?}")))
    };
    let base_len = scalar("base_len")?;
    let start = scalar("start")?;
    let len = scalar("edit_len")?;
    let end = start + len;
    let expectation = artifact
        .values
        .expectation("result")
        .ok_or_else(|| OpError::Io("the artifact declares no result expectation".to_string()))?;
    let store = &artifact.objects;
    // The replacement is the mutation's own input. `preserve` replaces the window
    // with itself, so it is recovered from the base through the product's read
    // path; the other two directions are pure functions of the declared recipe.
    let replacement = match op {
        CountOp::Preserve => base_window(store, base_member.root, start, end)?,
        CountOp::Increase => fixture::noise(len, seed ^ 0x1111),
        CountOp::Decrease => fixture::zeros(len),
    };
    let initial = extent_count_of(store, base_member.root)?;

    let stream = match Edits::new(base_len, vec![Edit::new(start, end, len)]) {
        Ok(stream) => stream,
        Err(error) => return Ok(unmeasured(&OpError::Product(format!("{error:?}")), gates)),
    };
    let mut source = Parts::new();
    source.push(replacement);

    instruments::heap_begin();
    let (measured, report) = super::measure("c1.chunk-count", |scope: &TimingScope<'_, Active>| {
        let mut consumer = DiscardingConsumer::new();
        let request = EditRequest {
            root: base_member.root,
            edits: &stream,
            source: &source,
        };
        apply_edits(
            policy,
            &capacities,
            store,
            request,
            &mut consumer,
            scope.child("edit"),
        )
        .map(|file| (file, consumer))
    });
    let heap = instruments::heap_end();
    let timing_bytes = crate::support::phases::timing_json_bytes();
    let (measured_file, measured_consumer) = match measured {
        Ok(value) => value,
        Err(error) => return Ok(unmeasured(&OpError::Product(format!("{error:?}")), gates)),
    };
    let counters = measured_file.counters;

    // Oracle: a second, unmeasured, identical edit into the store that already
    // holds the base, so the result's references all resolve.
    let mut result_store = TreeStore::new();
    let (replay, _) = Timing::disabled("oracle.replay", |scope: &TimingScope<'_, Active>| {
        let request = EditRequest {
            root: base_member.root,
            edits: &stream,
            source: &source,
        };
        apply_edits(
            policy,
            &capacities,
            store,
            request,
            &mut result_store,
            scope.child("edit"),
        )
    });
    let replay = match replay {
        Ok(file) => file,
        Err(error) => return Ok(unmeasured(&OpError::Product(format!("{error:?}")), gates)),
    };
    let oracle_provider = PairProvider::new(&result_store, store);
    let final_count = extent_count_of(&oracle_provider, replay.root)?;

    context.trace.write_number(Kind::Counter, "chunk_count.initial_count", initial as i128, "extents", "FileState.extent_count of the base root")?;
    context.trace.write_number(Kind::Counter, "chunk_count.final_count", final_count as i128, "extents", "FileState.extent_count of the result root")?;
    context.trace.write_number(Kind::Counter, "edit.nodes_read", counters.nodes_read as i128, "nodes", "EditCounters.nodes_read")?;
    context.trace.write_number(Kind::Counter, "edit.nodes_created", counters.nodes_created as i128, "nodes", "EditCounters.nodes_created")?;
    context.trace.write_number(Kind::Counter, "edit.payloads_created", counters.payloads_created as i128, "objects", "EditCounters.payloads_created")?;
    context.trace.write_number(Kind::Counter, "edit.payload_bytes", counters.payload_bytes as i128, "bytes", "EditCounters.payload_bytes")?;
    context.trace.write_number(Kind::Counter, "edit.peak_deferred_bytes", counters.peak_deferred_bytes as i128, "bytes", "EditCounters.peak_deferred_bytes")?;
    context.trace.write_number(Kind::Counter, "consumer.objects", measured_consumer.objects() as i128, "objects", "DiscardingConsumer.objects, measured phase")?;
    context.trace.write_number(Kind::Counter, "timing_json_bytes", timing_bytes as i128, "bytes", "product timing.json, byte-verbatim")?;
    context.trace.write_number(Kind::Resource, "heap.peak_incremental_bytes", heap.peak_incremental_bytes as i128, "bytes", "counting GlobalAlloc, measured phase")?;

    gates.push(completeness_gate(&report, "g7.tree-complete"));
    // O1 for real: the measured root is published so the pinned-constant gate in
    // `main` can compare it with `tests/golden/expected.tsv`. The replay gate below
    // is self-consistency and stays as a second, weaker check.
    crate::workload::expected::publish(context.trace, "file_root", &measured_file.root.to_string())?;
    gates.push(gates::require(
        GateClass::Correctness,
        "g1.o1-replay-root",
        replay.root == measured_file.root,
        &format!("replay root {}", replay.root),
        &format!("measured root {}", measured_file.root),
    ));
    gates.push(gates::require(
        GateClass::Correctness,
        "g1.o2-length",
        measured_file.logical_len == base_len,
        &format!("{} bytes", measured_file.logical_len),
        &format!("{base_len} bytes (a length-preserving overwrite)"),
    ));
    let (readback, _) = Timing::disabled("oracle.readback", |scope: &TimingScope<'_, Active>| {
        oracle::read_back(&oracle_provider, replay.root, &expectation, scope.child("content"))
    });
    match readback {
        Ok(readback) => gates.push(gates::require(
            GateClass::Correctness,
            "g1.o2-readback",
            readback.matches(),
            &format!("{} bytes read, {}", readback.bytes, readback.digest),
            &format!("{} bytes, sha256 {}", readback.expected_bytes, readback.expected_digest),
        )),
        Err(error) => gates.push(Gate::incomplete(
            GateClass::Correctness,
            "g1.o2-readback",
            &format!("read-back failed: {error:?}"),
            "readback equals input",
        )),
    }
    let direction = match op {
        CountOp::Preserve => final_count == initial,
        CountOp::Increase => final_count > initial,
        CountOp::Decrease => final_count < initial,
    };
    let direction_name = match op {
        CountOp::Preserve => "unchanged",
        CountOp::Increase => "increased",
        CountOp::Decrease => "decreased",
    };
    gates.push(gates::require(
        GateClass::Mechanism,
        "g2.chunk-count-direction",
        direction,
        &format!("{initial} -> {final_count} extents ({})", if direction { direction_name } else { "opposite direction" }),
        &format!("the row's own ID declares the count {direction_name}"),
    ));
    gates.push(gates::require(
        GateClass::Resource,
        "g4.peak-deferred",
        (counters.peak_deferred_bytes as usize) <= layerfs_content::file::edit::EDIT_DEFERRED_LIMIT,
        &format!("{} bytes", counters.peak_deferred_bytes),
        &format!("<= EDIT_DEFERRED_LIMIT ({})", layerfs_content::file::edit::EDIT_DEFERRED_LIMIT),
    ));
    gates.push(gates::swap_gate(instruments::swaps()));

    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("fixture_seed: {seed}"),
            match op {
                CountOp::Decrease => format!(
                    "base_shape: chunk-dense window {start}..{end} of the declared pair {:#04x?}",
                    crate::fixture::CHUNK_DENSE_PAIR
                ),
                _ => format!("base_shape: zero region {STRUCTURED_ZERO_REGION:?}"),
            },
            format!("edit_start: {start}"),
            format!("edit_len: {len}"),
            format!("heap_charged_bytes: {}", heap.charged_bytes),
            format!("heap_allocations: {}", heap.allocations),
            "cache_state: warm-in-process-fixture; the base is loaded from the prepared master before the timed region".to_string(),
            "oracle_phase: separate-unmeasured-replay".to_string(),
        ],
    })
}

/// C1-4/C1-5: one localized edit of a constructed base.
///
/// **Phase split.** The row measures `apply_edits` over a 4 KiB replacement; the
/// base is a fixture. Building it means generating up to 500 MiB, running CDC over
/// it and hashing it to state the expectation the oracle compares against — none
/// of which is the claim. The registry declares `Preparation::ObjectSet`, so the
/// base and its expectation are acquired once and every sample loads them.
pub fn edit(case: &Case, op: EditOp, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    match context.phase {
        Phase::Prepare => edit_prepare(case, op, context),
        Phase::Perf => edit_perf(case, op, context),
        Phase::Verify => Err(OpError::Io(
            "c1.edit declares an in-process oracle and has no deferred verification phase"
                .to_string(),
        )),
    }
}

/// C1-4/C1-5 `prepare`: acquire the base once, with the expectation its oracle needs.
fn edit_prepare(case: &Case, op: EditOp, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    let directory = context.artifact_directory()?;
    let seed = seed_of(case.id);
    let policy = ConstructionPolicy::frozen_default();
    let capacities = policy.capacities();
    let base = structured(case.bytes, seed, true);
    let (start, end, replacement_len) = edit_range(op, base.len() as u64);
    let replacement = edit_replacement(op, seed);
    let mut store = TreeStore::new();
    let base_file = build_base(policy, &capacities, &base, &mut store)?;
    let expected_len = match Edits::new(
        base.len() as u64,
        vec![Edit::new(start, end, replacement_len)],
    ) {
        Ok(stream) => stream.final_len(),
        Err(error) => return Ok(unmeasured(&OpError::Product(format!("{error:?}")), Vec::new())),
    };
    let expectation = Expectation::spliced(&base, start, end, &replacement);
    let ids = store.insertion_order().to_vec();
    let mut artifact = Artifact::new(store);
    artifact.members.push(Member {
        ids,
        root: base_file.root,
        expectation: None,
    });
    artifact.values.set_scalar("base_len", base.len() as u64);
    artifact.values.set_scalar("start", start);
    artifact.values.set_scalar("end", end);
    artifact.values.set_scalar("replacement_len", replacement_len);
    artifact.values.set_scalar("expected_len", expected_len);
    artifact.values.set_expectation("result", expectation);
    let written = artifact
        .write(&directory)
        .map_err(|error| OpError::Io(format!("{}: {error}", directory.display())))?;
    artifact
        .seal(&directory, case.id)
        .map_err(|error| OpError::Io(format!("{}: {error}", directory.display())))?;
    context.trace.write_number(
        Kind::Counter,
        "prepare.objects",
        written as i128,
        "objects",
        "distinct canonical objects the artifact holds",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "prepare.base_bytes",
        base.len() as i128,
        "bytes",
        "declared base bytes",
    )?;
    Ok(OpOutcome {
        gates: Vec::new(),
        notes: vec![
            format!("prepare_op: {op:?}"),
            format!("prepare_base_bytes: {}", base.len()),
            format!("edit_range: {start}..{end} replacement_len {replacement_len}"),
            format!("artifact: {}", directory.display()),
            "acquisition: not a measured phase; charged to acquisition_wall_ns".to_string(),
        ],
    })
}

/// C1-4/C1-5 `perf`: the measured edit against the prepared base.
fn edit_perf(case: &Case, op: EditOp, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    context.create_output()?;
    let seed = seed_of(case.id);
    let policy = ConstructionPolicy::frozen_default();
    let capacities = policy.capacities();
    let mut gates = Vec::new();

    let directory = context.artifact_directory()?;
    let artifact = Artifact::read(&directory)
        .map_err(|error| OpError::Io(format!("{}: {error}", directory.display())))?;
    let base_member = artifact.base().map_err(OpError::Io)?;
    let scalar = |name: &str| -> Result<u64, OpError> {
        artifact
            .values
            .scalar(name)
            .ok_or_else(|| OpError::Io(format!("the artifact declares no scalar {name:?}")))
    };
    let base_len = scalar("base_len")?;
    let start = scalar("start")?;
    let end = scalar("end")?;
    let replacement_len = scalar("replacement_len")?;
    let expected_len = scalar("expected_len")?;
    let expectation = artifact
        .values
        .expectation("result")
        .ok_or_else(|| OpError::Io("the artifact declares no result expectation".to_string()))?;
    let store = &artifact.objects;
    let replacement = edit_replacement(op, seed);
    let stream = match Edits::new(base_len, vec![Edit::new(start, end, replacement_len)]) {
        Ok(stream) => stream,
        Err(error) => return Ok(unmeasured(&OpError::Product(format!("{error:?}")), gates)),
    };
    let mut source = Parts::new();
    if replacement_len > 0 {
        source.push(replacement);
    }
    let base_extents = extent_count_of(store, base_member.root)?;

    instruments::heap_begin();
    let (measured, report) = super::measure("c1.edit", |scope: &TimingScope<'_, Active>| {
        let mut consumer = DiscardingConsumer::new();
        let request = EditRequest { root: base_member.root, edits: &stream, source: &source };
        apply_edits(policy, &capacities, store, request, &mut consumer, scope.child("edit"))
            .map(|file| (file, consumer))
    });
    let heap = instruments::heap_end();
    let timing_bytes = crate::support::phases::timing_json_bytes();
    let (measured_file, measured_consumer) = match measured {
        Ok(value) => value,
        Err(error) => return Ok(unmeasured(&OpError::Product(format!("{error:?}")), gates)),
    };
    let counters = measured_file.counters;

    let mut result_store = TreeStore::new();
    let (replay, _) = Timing::disabled("oracle.replay", |scope: &TimingScope<'_, Active>| {
        let request = EditRequest { root: base_member.root, edits: &stream, source: &source };
        apply_edits(policy, &capacities, store, request, &mut result_store, scope.child("edit"))
    });
    let replay = match replay {
        Ok(file) => file,
        Err(error) => return Ok(unmeasured(&OpError::Product(format!("{error:?}")), gates)),
    };
    let oracle_provider = PairProvider::new(&result_store, store);
    let final_extents = extent_count_of(&oracle_provider, replay.root)?;

    context.trace.write_number(Kind::Counter, "edit.base_bytes", base_len as i128, "bytes", "fixture recipe")?;
    context.trace.write_number(Kind::Counter, "edit.final_bytes", measured_file.logical_len as i128, "bytes", "ConstructedFile.logical_len")?;
    context.trace.write_number(Kind::Counter, "edit.expected_final_bytes", expected_len as i128, "bytes", "base - removed + replacement, computed by the oracle")?;
    context.trace.write_number(Kind::Counter, "edit.nodes_read", counters.nodes_read as i128, "nodes", "EditCounters.nodes_read")?;
    context.trace.write_number(Kind::Counter, "edit.nodes_created", counters.nodes_created as i128, "nodes", "EditCounters.nodes_created")?;
    context.trace.write_number(Kind::Counter, "edit.payloads_created", counters.payloads_created as i128, "objects", "EditCounters.payloads_created")?;
    context.trace.write_number(Kind::Counter, "edit.payload_bytes", counters.payload_bytes as i128, "bytes", "EditCounters.payload_bytes")?;
    context.trace.write_number(Kind::Counter, "edit.peak_deferred_bytes", counters.peak_deferred_bytes as i128, "bytes", "EditCounters.peak_deferred_bytes")?;
    context.trace.write_number(Kind::Counter, "edit.base_extents", base_extents as i128, "extents", "FileState.extent_count of the base root")?;
    context.trace.write_number(Kind::Counter, "edit.final_extents", final_extents as i128, "extents", "FileState.extent_count of the result root")?;
    context.trace.write_number(Kind::Counter, "consumer.objects", measured_consumer.objects() as i128, "objects", "DiscardingConsumer.objects, measured phase")?;
    context.trace.write_number(Kind::Counter, "timing_json_bytes", timing_bytes as i128, "bytes", "product timing.json, byte-verbatim")?;
    context.trace.write_number(Kind::Resource, "heap.peak_incremental_bytes", heap.peak_incremental_bytes as i128, "bytes", "counting GlobalAlloc, measured phase")?;

    gates.push(completeness_gate(&report, "g7.tree-complete"));
    // O1 for real: the measured root is published so the pinned-constant gate in
    // `main` can compare it with `tests/golden/expected.tsv`. The replay gate below
    // is self-consistency and stays as a second, weaker check.
    crate::workload::expected::publish(context.trace, "file_root", &measured_file.root.to_string())?;
    gates.push(gates::require(
        GateClass::Correctness,
        "g1.o1-replay-root",
        replay.root == measured_file.root,
        &format!("replay root {}", replay.root),
        &format!("measured root {}", measured_file.root),
    ));
    gates.push(gates::require(
        GateClass::Correctness,
        "g1.o2-length-equation",
        measured_file.logical_len == expected_len,
        &format!("{} bytes", measured_file.logical_len),
        &format!("{expected_len} bytes = base - removed + replacement"),
    ));
    if op.is_length_preserving() {
        gates.push(gates::require(
            GateClass::Correctness,
            "g1.o2-length-preserved",
            measured_file.logical_len == base_len,
            &format!("{} bytes", measured_file.logical_len),
            &format!("{base_len} bytes: the base length, byte-exactly"),
        ));
    }
    let (readback, _) = Timing::disabled("oracle.readback", |scope: &TimingScope<'_, Active>| {
        oracle::read_back(&oracle_provider, replay.root, &expectation, scope.child("content"))
    });
    match readback {
        Ok(readback) => gates.push(gates::require(
            GateClass::Correctness,
            "g1.o2-readback",
            readback.matches(),
            &format!("{} bytes read, {}", readback.bytes, readback.digest),
            &format!("{} bytes, sha256 {}", readback.expected_bytes, readback.expected_digest),
        )),
        Err(error) => gates.push(Gate::incomplete(
            GateClass::Correctness,
            "g1.o2-readback",
            &format!("read-back failed: {error:?}"),
            "readback equals input",
        )),
    }
    gates.push(gates::require(
        GateClass::Resource,
        "g4.peak-deferred",
        (counters.peak_deferred_bytes as usize) <= layerfs_content::file::edit::EDIT_DEFERRED_LIMIT,
        &format!("{} bytes", counters.peak_deferred_bytes),
        &format!("<= EDIT_DEFERRED_LIMIT ({})", layerfs_content::file::edit::EDIT_DEFERRED_LIMIT),
    ));
    gates.push(gates::swap_gate(instruments::swaps()));

    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("fixture_seed: {seed}"),
            format!("edit_op: {op:?}"),
            format!("edit_range: {start}..{end} replacement_len {replacement_len}"),
            format!("heap_charged_bytes: {}", heap.charged_bytes),
            format!("heap_allocations: {}", heap.allocations),
            "cache_state: warm-in-process-fixture; the base is loaded from the prepared master before the timed region".to_string(),
            "oracle_phase: separate-unmeasured-replay".to_string(),
        ],
    })
}

/// C1-6: a representation transition across the frozen cutoff.
pub fn transition(
    case: &Case,
    target: Transition,
    context: &mut OpContext<'_>,
) -> Result<OpOutcome, OpError> {
    context.create_output()?;
    let seed = seed_of(case.id);
    let policy = ConstructionPolicy::frozen_default();
    let capacities = policy.capacities();
    let cutoff = policy.small_file_threshold_bytes();
    let mut gates = Vec::new();

    let build_len = match target {
        Transition::SmallControl => crate::families::c1_transition::SMALL_CONTROL,
        Transition::Below => crate::families::c1_transition::BOUNDARY_BELOW,
        Transition::Exact => crate::families::c1_transition::BOUNDARY_EXACT,
        Transition::Above => crate::families::c1_transition::BOUNDARY_ABOVE,
        Transition::LargeControl => crate::families::c1_transition::LARGE_CONTROL,
        Transition::Roundtrip | Transition::AliasRoundtrip => crate::families::c1_transition::LARGE_CONTROL,
    };
    let bytes = fixture::noise(build_len, seed);
    let mut store = TreeStore::new();

    instruments::heap_begin();
    let (measured, report) = super::measure("c1.transition", |scope: &TimingScope<'_, Active>| {
        let mut consumer = DiscardingConsumer::new();
        construct_bytes(policy, &capacities, &bytes, &mut consumer, scope.child("content"))
            .map(|file| (file, consumer))
    });
    let heap = instruments::heap_end();
    let timing_bytes = crate::support::phases::timing_json_bytes();
    let (measured_file, measured_consumer) = match measured {
        Ok(value) => value,
        Err(error) => return Ok(unmeasured(&OpError::Product(format!("{error:?}")), gates)),
    };
    let base_file = build_base(policy, &capacities, &bytes, &mut store)?;
    let base_extents = extent_count_of(&store, base_file.root)?;
    let base_content = representation_of(&store, base_file.root)?;

    let mut result_store = TreeStore::new();
    let (result_root, result_len, transition_note) = match target {
        Transition::Roundtrip | Transition::AliasRoundtrip => {
            let start = cutoff - 1;
            let end = bytes.len() as u64;
            let stream = match Edits::new(bytes.len() as u64, vec![Edit::delete(start, end)]) {
                Ok(stream) => stream,
                Err(error) => return Ok(unmeasured(&OpError::Product(format!("{error:?}")), gates)),
            };
            let source = Parts::new();
            let (replayed, _) = Timing::disabled("oracle.replay", |scope: &TimingScope<'_, Active>| {
                let request = EditRequest { root: base_file.root, edits: &stream, source: &source };
                apply_edits(policy, &capacities, &store, request, &mut result_store, scope.child("edit"))
            });
            match replayed {
                Ok(file) => (
                    file.root,
                    file.logical_len,
                    format!("chunked base shrunk to {start} bytes, below the {cutoff} cutoff"),
                ),
                Err(error) => return Ok(unmeasured(&OpError::Product(format!("{error:?}")), gates)),
            }
        }
        _ => (
            base_file.root,
            base_file.logical_len,
            format!("constructed at {build_len} bytes"),
        ),
    };

    // The result's references resolve against the base it kept, so the oracle
    // reads through both stores rather than a copy of either.
    let oracle_store = PairProvider::new(&result_store, &store);
    let content = representation_of(&oracle_store, result_root)?;
    let (representation, extents) = match content {
        FileContent::WholeFile { .. } => ("whole-file".to_string(), 0_u64),
        FileContent::Chunked(state) => ("chunked".to_string(), state.extent_count),
    };
    let expected_representation = if result_len < cutoff { "whole-file" } else { "chunked" };

    // The alias check: identity is content-addressed, so a second construction of
    // the same bytes must produce the same root, and rewriting one must leave the
    // other readable.
    let mut alias_ok = true;
    let mut alias_note = "not applicable".to_string();
    if target == Transition::AliasRoundtrip {
        let mut alias_store = TreeStore::new();
        let alias = build_base(policy, &capacities, &bytes, &mut alias_store)?;
        alias_ok = alias.root == base_file.root;
        let expectation = Expectation::of(&bytes);
        let (readback, _) = Timing::disabled("oracle.alias", |scope: &TimingScope<'_, Active>| {
            oracle::read_back(&alias_store, alias.root, &expectation, scope.child("content"))
        });
        alias_ok = alias_ok && readback.map(|back| back.matches()).unwrap_or(false);
        alias_note = format!("alias root {} ", alias.root);
    }

    context.trace.write_number(Kind::Counter, "transition.build_bytes", build_len as i128, "bytes", "fixture recipe")?;
    context.trace.write_number(Kind::Counter, "transition.result_bytes", result_len as i128, "bytes", "ConstructedFile.logical_len")?;
    context.trace.write_number(Kind::Counter, "transition.extents", extents as i128, "extents", "FileState.extent_count of the result root")?;
    context.trace.write_number(Kind::Counter, "transition.base_extents", base_extents as i128, "extents", "FileState.extent_count of the base root")?;
    context.trace.write_number(Kind::Counter, "construct.objects", measured_consumer.objects() as i128, "objects", "DiscardingConsumer.objects, measured phase")?;
    context.trace.write_number(Kind::Counter, "timing_json_bytes", timing_bytes as i128, "bytes", "product timing.json, byte-verbatim")?;
    context.trace.write_number(Kind::Resource, "heap.peak_incremental_bytes", heap.peak_incremental_bytes as i128, "bytes", "counting GlobalAlloc, measured phase")?;

    gates.push(completeness_gate(&report, "g7.tree-complete"));
    crate::workload::expected::publish(context.trace, "file_root", &measured_file.root.to_string())?;
    gates.push(gates::require(
        GateClass::Correctness,
        "g1.o1-replay-root",
        base_file.root == measured_file.root,
        &format!("replay root {}", base_file.root),
        &format!("measured root {}", measured_file.root),
    ));
    gates.push(gates::require(
        GateClass::Mechanism,
        "g2.representation-transition",
        representation == expected_representation,
        &format!("{result_len} bytes routed to {representation}"),
        &format!("{expected_representation} at cutoff {cutoff}"),
    ));
    gates.push(gates::require(
        GateClass::Correctness,
        "g1.o2-base-representation",
        match target {
            Transition::Below | Transition::SmallControl => matches!(base_content, FileContent::WholeFile { .. }),
            _ => matches!(base_content, FileContent::Chunked(_)),
        },
        &format!("base {} at {build_len} bytes", match base_content { FileContent::WholeFile { .. } => "whole-file", FileContent::Chunked(_) => "chunked" }),
        &format!("the declared route at cutoff {cutoff}"),
    ));
    if target == Transition::AliasRoundtrip {
        gates.push(gates::require(
            GateClass::Correctness,
            "g1.alias-survives",
            alias_ok,
            &alias_note,
            "a second construction of the same bytes yields the same root and still reads back",
        ));
    }
    gates.push(gates::swap_gate(instruments::swaps()));

    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("fixture_seed: {seed}"),
            format!("transition: {transition_note}"),
            format!("heap_charged_bytes: {}", heap.charged_bytes),
            format!("heap_allocations: {}", heap.allocations),
            "cache_state: warm-in-process-fixture; the base is built before the timed region".to_string(),
            "oracle_phase: separate-unmeasured-replay".to_string(),
        ],
    })
}
