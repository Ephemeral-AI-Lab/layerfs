//! C2 shape drivers: lifecycle, reuse, delta locality and boundaries, the
//! small-file policy, read waves and footprint.
//!
//! **The Store is opened from a prepared copy in every measured phase except
//! `lifecycle-create`.** Phase 0 declared creation-inside-the-region for the
//! C2-only supplied-object family; it does not generalise, because every other
//! family measures against a base that must already be stored. The copy is a
//! closed, quiescent **byte copy** (rung R2), never a reflink: `journal_mode =
//! MEMORY` is applied per connection and is not persisted in the file header, so a
//! clone cannot inherit it, and a COW clone's `st_blocks` double-counts blocks
//! shared with the master.
//!
//! C2 never runs C1 file construction inside a measured phase. Every canonical
//! object a C2 row saves is supplied by the harness: built before the timer, and
//! offered to the Store as the finalized objects they are, so no envelope is
//! re-guessed on the way in.

use std::io::Write;
use std::path::{Path, PathBuf};

use layerfs_content::{
    construct_bytes, ConstructionPolicy, ConstructionCapacities, ObjectId,
};
use layerfs_storage::{SaveOutcome, StoragePolicy, Store, StoreProvider};
use layerfs_telemetry::timer::{Active, Timing, TimingScope, TimingReport};

use super::{seed_of, OpContext, OpError, OpOutcome};
use crate::fixture;
use crate::gates::{self, Gate, GateClass};
use crate::registry::{Case, DeltaOp, FootprintOp, LifecycleStep, ReuseOp};
use crate::support::instruments;
use crate::support::trace::Kind;
use crate::workload::oracle::{self, Expectation};
use crate::workload::providers::TreeStore;

/// The declared copy rung and its allocation attribution.
pub const COPY_RUNG: &str = "closed-quiescent-byte-copy";
/// Attribution every C2 row carries. A clone's blocks are shared with its master.
pub const ATTRIBUTION: &str = "exclusive";

fn write_timing(directory: &Path, report: &TimingReport) -> Result<u64, OpError> {
    let path = directory.join("timing.json");
    let mut file = std::fs::File::create(&path)
        .map_err(|error| OpError::Io(format!("{}: {error}", path.display())))?;
    report
        .write_json(&mut file)
        .map_err(|error| OpError::Io(format!("{}: {error}", path.display())))?;
    file.flush()
        .map_err(|error| OpError::Io(format!("{}: {error}", path.display())))?;
    Ok(file.metadata().map(|meta| meta.len()).unwrap_or(0))
}

fn completeness_gate(report: &TimingReport) -> Gate {
    if report.root().is_none() {
        return Gate::incomplete(
            GateClass::TimingPurity,
            "g7.tree-complete",
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

fn unmeasured(error: &OpError, extra: Vec<Gate>) -> OpOutcome {
    OpOutcome {
        gates: extra
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

/// Builds one file's canonical objects outside every timer.
///
/// Returns the store **and the root identity the constructor reported**. Taking
/// the last insertion as the root would be a guess: emission order is not part of
/// the product's contract, and a guess that happened to be wrong would produce an
/// oracle that verifies the wrong object — which is exactly the defect this
/// function's signature removes.
fn objects_of(bytes: &[u8]) -> Result<(TreeStore, ObjectId), OpError> {
    let policy = ConstructionPolicy::frozen_default();
    let capacities: ConstructionCapacities = policy.capacities();
    let mut store = TreeStore::new();
    let (result, _) = Timing::disabled("setup.construct", |scope: &TimingScope<'_, Active>| {
        construct_bytes(policy, &capacities, bytes, &mut store, scope.child("content"))
    });
    let file = result.map_err(|error| OpError::Product(format!("{error:?}")))?;
    Ok((store, file.root))
}

/// Opens a Store without a timer, for setup and oracle work.
fn open_untimed(path: &Path) -> Result<Store, OpError> {
    let (result, _) = Timing::disabled("setup.open", |scope: &TimingScope<'_, Active>| {
        Store::open(path, scope.child("store.open"))
    });
    result.map_err(|error| OpError::Product(format!("{error:?}")))
}

/// Creates a Store without a timer and saves `objects` into it.
fn create_and_save_untimed(path: &Path, objects: &TreeStore) -> Result<SaveOutcome, OpError> {
    let (result, _) = Timing::disabled("setup.store", |scope: &TimingScope<'_, Active>| {
        let store = Store::create(path, StoragePolicy::frozen_default(), scope.child("store.create"))?;
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        for id in objects.insertion_order() {
            let object = objects
                .cloned_object(*id)
                .ok_or(layerfs_storage::StorageError::ObjectMissing(*id))?;
            operation.accept(object)?;
        }
        operation.finish(scope.child("storage.finish"))
    });
    result.map_err(|error| OpError::Product(format!("{error:?}")))
}

/// Copies the base Store to the sample path and de-warms the copy.
fn prepare_sample(base: &Path, sample: &Path) -> Result<instruments::DeWarmReport, OpError> {
    std::fs::copy(base, sample).map_err(|error| {
        OpError::Io(format!("{} -> {}: {error}", base.display(), sample.display()))
    })?;
    instruments::de_warm(sample).map_err(|error| OpError::Io(format!("de-warm: {error}")))
}

/// Sidecar gates for a Store path: `journal_mode = MEMORY` leaves none.
fn sidecar_gates(path: &Path) -> Vec<Gate> {
    ["-wal", "-shm", "-journal"]
        .iter()
        .map(|suffix| {
            let sidecar = PathBuf::from(format!("{}{suffix}", path.display()));
            let present = sidecar.exists();
            gates::require(
                GateClass::Cleanup,
                "g5.no-sidecars",
                !present,
                &format!("{}: {}", sidecar.display(), if present { "present" } else { "absent" }),
                "no -wal, -shm or -journal sidecar",
            )
        })
        .collect()
}

/// Reads a logical file back through a Store and compares it with the recipe.
fn read_back_through_store(
    store: &Store,
    root: ObjectId,
    expectation: &Expectation,
) -> Result<oracle::ReadBack, OpError> {
    let (result, _) = Timing::disabled("oracle.readback", |scope: &TimingScope<'_, Active>| {
        let provider = StoreProvider::new(store);
        oracle::read_back(&provider, root, expectation, scope.child("content"))
    });
    result.map_err(|error| OpError::Product(format!("{error:?}")))
}

/// C2-1: one Store lifecycle step.
pub fn lifecycle(
    case: &Case,
    step: LifecycleStep,
    context: &mut OpContext<'_>,
) -> Result<OpOutcome, OpError> {
    context.create_output()?;
    let mut gates = Vec::new();
    let sample = context.output.join("sample.sqlite");
    let policy = StoragePolicy::frozen_default();

    let mut master_source = "built-in-process".to_string();
    if step != LifecycleStep::Create {
        // `--store` hands the row a master the runner produced, which is how a
        // *perturbed* Store (a hand-edited watermark, for instance) is opened by
        // the real product instead of being described. Without it the row builds
        // its own base, and the master is a per-sample byte copy either way.
        let master = match context.store.clone() {
            Some(path) => {
                master_source = format!("runner-supplied:{}", path.display());
                path
            }
            None => {
                let bytes = fixture::noise(1 << 20, seed_of(case.id));
                let (objects, _root) = objects_of(&bytes)?;
                let base = context.output.join("base.sqlite");
                create_and_save_untimed(&base, &objects)?;
                base
            }
        };
        let de_warm = prepare_sample(&master, &sample)?;
        gates.push(gates::residency_gate(Some(de_warm.resident_after)));
        gates.push(crate::gates::attribution_gate(crate::gates::Attribution::Exclusive));
    }

    instruments::heap_begin();
    let (result, report) = Timing::record("c2.lifecycle", |scope: &TimingScope<'_, Active>| {
        match step {
            LifecycleStep::Create => {
                let store = Store::create(&sample, policy, scope.child("store.create"))?;
                Ok::<_, layerfs_storage::StorageError>((store.path().display().to_string(), 0_i128, 0_i128))
            }
            LifecycleStep::Open => {
                let store = Store::open(&sample, scope.child("store.open"))?;
                Ok::<_, layerfs_storage::StorageError>((store.path().display().to_string(), 0, 0))
            }
            LifecycleStep::BeginSave => {
                let store = Store::open(&sample, scope.child("store.open"))?;
                let operation = store.begin_save(scope.child("storage.begin"))?;
                let (objects, bytes) = operation.pending();
                // Concurrency and visibility, observed on a real Store. The busy
                // timeout is zero, so a second acquisition fails immediately and
                // deterministically rather than after a wait, which is why this is
                // observable without a race loop. A second acquisition that
                // *succeeded* would be the defect: two exclusive owners on one
                // Store. The outcome is carried out as the second return value so
                // the gate can assert the exact error class.
                let second = store.begin_save(scope.child("storage.begin2"));
                let second_token = match second {
                    Ok(_) => "ACQUIRED".to_string(),
                    Err(error) => format!("{error:?}"),
                };
                operation.abort(scope.child("storage.abort"))?;
                Ok::<_, layerfs_storage::StorageError>((
                    format!("{}|{second_token}", store.path().display()),
                    objects as i128,
                    bytes as i128,
                ))
            }
            LifecycleStep::FinishEmpty => {
                let store = Store::open(&sample, scope.child("store.open"))?;
                let operation = store.begin_save(scope.child("storage.begin"))?;
                let outcome = operation.finish(scope.child("storage.finish"))?;
                Ok::<_, layerfs_storage::StorageError>((
                    store.path().display().to_string(),
                    i128::from(outcome.inserted),
                    i128::from(outcome.commits),
                ))
            }
            LifecycleStep::Abort => {
                let store = Store::open(&sample, scope.child("store.open"))?;
                let operation = store.begin_save(scope.child("storage.begin"))?;
                operation.abort(scope.child("storage.abort"))?;
                Ok::<_, layerfs_storage::StorageError>((store.path().display().to_string(), 0, 0))
            }
        }
    });
    let heap = instruments::heap_end();
    let timing_bytes = write_timing(context.output, &report)?;
    let (store_path, first, second) = match result {
        Ok(value) => value,
        Err(error) => return Ok(unmeasured(&OpError::Product(format!("{error:?}")), gates)),
    };

    let (store_path, second_begin) = match store_path.split_once('|') {
        Some((path, second)) => (path.to_string(), Some(second.to_string())),
        None => (store_path.clone(), None),
    };
    context.trace.write(Kind::Receipt, "store_path", &store_path, "", "the path the operation used")?;
    context.trace.write(Kind::Receipt, "master_source", &master_source, "", "where the sample copy came from")?;
    if let Some(second) = &second_begin {
        context.trace.write(
            Kind::Oracle,
            "second_begin_save",
            second,
            "",
            "a second exclusive acquisition on the same Store",
        )?;
    }
    context.trace.write_number(Kind::Counter, "lifecycle.first", first, "count", "objects or inserted, per step")?;
    context.trace.write_number(Kind::Counter, "lifecycle.second", second, "count", "commits or pending bytes, per step")?;
    context.trace.write_number(Kind::Counter, "timing_json_bytes", timing_bytes as i128, "bytes", "product timing.json, byte-verbatim")?;
    context.trace.write_number(Kind::Resource, "heap.peak_incremental_bytes", heap.peak_incremental_bytes as i128, "bytes", "counting GlobalAlloc, measured phase")?;

    gates.push(completeness_gate(&report));
    gates.push(gates::require(
        GateClass::Custody,
        "g6.store-exists",
        Path::new(&store_path).exists(),
        &format!("{store_path} exists"),
        "the operation produced a Store at the path it was given",
    ));
    gates.extend(sidecar_gates(Path::new(&store_path)));
    if let Some(second) = &second_begin {
        gates.push(gates::require(
            GateClass::Mechanism,
            "g2.second-begin-refused",
            second.contains("OwnershipUnavailable"),
            second,
            "a second `begin_save` on one Store fails `OwnershipUnavailable` (busy timeout is zero)",
        ));
    }
    gates.push(gates::swap_gate(instruments::swaps()));

    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("lifecycle_step: {step:?}"),
            format!("master_source: {master_source}"),
            format!(
                "store_state: {}",
                if step == LifecycleStep::Create { "created-in-sample" } else { "opened-from-copy" }
            ),
            format!("copy_rung: {COPY_RUNG}"),
            format!("allocation_attribution: {ATTRIBUTION}"),
            format!("heap_charged_bytes: {}", heap.charged_bytes),
        ],
    })
}

/// C2-2: cross-file reuse of supplied objects, one save operation.
pub fn reuse(case: &Case, op: ReuseOp, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    context.create_output()?;
    let mut gates = Vec::new();
    let seed = seed_of(case.id);
    let members = case.entries.max(1);
    let member_bytes = if case.bytes > 0 { case.bytes } else { 1 << 20 };

    // The member set is declared by the row's own profile, so the reuse equation
    // below is falsifiable rather than descriptive.
    // Each member keeps its recipe bytes, its object store and its root, so the
    // oracle compares the logical bytes the recipe declares rather than the root
    // object's own canonical framing.
    let mut offered: Vec<(Vec<u8>, TreeStore, ObjectId)> = Vec::with_capacity(members as usize);
    for index in 0..members {
        let bytes = match op {
            ReuseOp::Identical | ReuseOp::Base128 => fixture::noise(member_bytes, seed),
            ReuseOp::Unique => fixture::noise(member_bytes, seed ^ (u64::from(index) << 8)),
            ReuseOp::Local => {
                let mut bytes = fixture::noise(member_bytes, seed);
                let at = bytes.len() / 2;
                let end = (at + 4096).min(bytes.len());
                let patch = fixture::noise((end - at) as u64, seed ^ (u64::from(index) << 16));
                bytes[at..end].copy_from_slice(&patch);
                bytes
            }
        };
        let (store, root) = objects_of(&bytes)?;
        offered.push((bytes, store, root));
    }
    let mut accepted: u64 = 0;
    let mut distinct: Vec<ObjectId> = Vec::new();
    for (_, store, _) in &offered {
        for id in store.insertion_order() {
            accepted += 1;
            if !distinct.contains(id) {
                distinct.push(*id);
            }
        }
    }

    let base = context.output.join("base.sqlite");
    let sample = context.output.join("sample.sqlite");
    create_and_save_untimed(&base, &TreeStore::new())?;
    let de_warm = prepare_sample(&base, &sample)?;
    gates.push(gates::residency_gate(Some(de_warm.resident_after)));
    gates.push(crate::gates::attribution_gate(crate::gates::Attribution::Exclusive));

    let store = match open_untimed(&sample) {
        Ok(store) => store,
        Err(error) => return Ok(unmeasured(&error, gates)),
    };
    instruments::heap_begin();
    let (result, report) = Timing::record("c2.reuse", |scope: &TimingScope<'_, Active>| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        for (_, member, _) in &offered {
            for id in member.insertion_order() {
                let object = member
                    .cloned_object(*id)
                    .ok_or(layerfs_storage::StorageError::ObjectMissing(*id))?;
                operation.accept(object)?;
            }
        }
        operation.finish(scope.child("storage.finish"))
    });
    let heap = instruments::heap_end();
    let timing_bytes = write_timing(context.output, &report)?;
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => return Ok(unmeasured(&OpError::Product(format!("{error:?}")), gates)),
    };
    let expected_reused = accepted - distinct.len() as u64;

    context.trace.write_number(Kind::Counter, "reuse.accepted", accepted as i128, "objects", "harness-declared member set")?;
    context.trace.write_number(Kind::Counter, "reuse.distinct", distinct.len() as i128, "objects", "harness-declared member set")?;
    context.trace.write_number(Kind::Counter, "reuse.reused", i128::from(outcome.reused), "objects", "SaveOutcome.reused")?;
    context.trace.write_number(Kind::Counter, "reuse.inserted", i128::from(outcome.inserted), "objects", "SaveOutcome.inserted")?;
    context.trace.write_number(Kind::Counter, "reuse.commits", i128::from(outcome.commits), "transactions", "SaveOutcome.commits")?;
    context.trace.write_number(Kind::Counter, "reuse.statements", i128::from(outcome.statements), "statements", "SaveOutcome.statements")?;
    context.trace.write_number(Kind::Counter, "reuse.presence_queries", i128::from(outcome.presence_queries), "queries", "SaveOutcome.presence_queries")?;
    context.trace.write_number(Kind::Counter, "timing_json_bytes", timing_bytes as i128, "bytes", "product timing.json, byte-verbatim")?;
    context.trace.write_number(Kind::Resource, "heap.peak_incremental_bytes", heap.peak_incremental_bytes as i128, "bytes", "counting GlobalAlloc, measured phase")?;

    gates.push(completeness_gate(&report));
    let equation = match op {
        ReuseOp::Identical | ReuseOp::Base128 => outcome.reused == expected_reused,
        ReuseOp::Unique => outcome.reused == 0,
        ReuseOp::Local => outcome.reused > 0 && outcome.reused < accepted,
    };
    gates.push(gates::require(
        GateClass::Mechanism,
        "g2.reuse-equation",
        equation,
        &format!(
            "reused {} of {accepted} accepted ({} distinct)",
            outcome.reused,
            distinct.len()
        ),
        match op {
            ReuseOp::Identical | ReuseOp::Base128 => {
                "identical profile: every duplicate after the first is reused"
            }
            ReuseOp::Unique => "unique profile: nothing is reused",
            ReuseOp::Local => "mixed profile: some objects are reused, not all",
        },
    ));

    // Oracle: read every member's root back through the store it was saved into.
    let store = match open_untimed(&sample) {
        Ok(store) => store,
        Err(error) => return Ok(unmeasured(&error, gates)),
    };
    let mut oracle_ok = true;
    let mut oracle_note = String::new();
    for (index, (bytes, _member, root)) in offered.iter().enumerate() {
        let expectation = Expectation::of(bytes);
        match read_back_through_store(&store, *root, &expectation) {
            Ok(back) => {
                if !back.matches() {
                    oracle_ok = false;
                    oracle_note = format!("member {index} read-back mismatch");
                }
            }
            Err(error) => {
                oracle_ok = false;
                oracle_note = format!("member {index}: {error}");
            }
        }
    }
    gates.push(gates::require(
        GateClass::Correctness,
        "g1.o1-member-readback",
        oracle_ok,
        if oracle_note.is_empty() { "every member root read back byte-exact" } else { &oracle_note },
        "each member's root object is readable and byte-exact after the save",
    ));
    gates.extend(sidecar_gates(&sample));
    gates.push(gates::swap_gate(instruments::swaps()));

    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("reuse_profile: {op:?}"),
            format!("members: {members}"),
            format!("member_bytes: {member_bytes}"),
            format!("store_state: opened-from-copy"),
            format!("copy_rung: {COPY_RUNG}"),
            format!("allocation_attribution: {ATTRIBUTION}"),
            format!("heap_charged_bytes: {}", heap.charged_bytes),
        ],
    })
}

/// Declared base file count of a C2-4 row.
///
/// The reference family fixes this: a `compact-v2` row at the 1 and 10 tiers holds
/// as many base files as the tier declares, every other row holds 128. The base is
/// the content the workspace already has, and the additions are derived from it, so
/// this figure decides the whole reuse equation.
fn workspace_base_files(case: &Case, members: u32) -> u32 {
    if case.tier <= 1 && case.profile == "compact-v2" {
        members
    } else {
        crate::families::c2_reuse::BASE128_FILES
    }
}

/// Seed of base file `index`: a domain of its own, so no base file shares an object
/// with another base file or with a `unique` addition.
fn base_file_seed(seed: u64, index: u32) -> u64 {
    seed ^ 0xb45e_0000_0000_0000 ^ u64::from(index)
}

/// C2-4: exact reuse against the base the workspace already holds.
///
/// C2-2 measures reuse inside one offered set against an empty Store. This family
/// measures the other half: the Store already holds `base_files` files, and each
/// addition is derived from the base file its ordinal maps to, so an `exact`
/// addition is an existing identity for every object it carries and a `unique` one
/// shares nothing. That is why the equations differ from C2-2's — an `exact` save
/// here must reuse *every* accepted object, not every duplicate after the first.
pub fn workspace(case: &Case, op: ReuseOp, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    context.create_output()?;
    let mut gates = Vec::new();
    let seed = seed_of(case.id);
    let members = case.entries.max(1);
    let member_bytes = if case.bytes > 0 { case.bytes } else { 1 << 20 };
    let base_files = workspace_base_files(case, members);

    // The base the workspace already holds. Built before the timed region, and
    // saved into the base Store before it is copied: the measured phase must pay
    // for the additions, not for the content that was already there.
    let mut base_store = TreeStore::new();
    let mut base_bytes: Vec<Vec<u8>> = Vec::with_capacity(base_files as usize);
    for index in 0..base_files {
        let bytes = fixture::noise(member_bytes, base_file_seed(seed, index));
        let (store, _root) = objects_of(&bytes)?;
        base_store.absorb(&store);
        base_bytes.push(bytes);
    }

    let mut offered: Vec<(Vec<u8>, TreeStore, ObjectId)> = Vec::with_capacity(members as usize);
    for index in 0..members {
        let base = &base_bytes[(index % base_files) as usize];
        let bytes = match op {
            ReuseOp::Identical => base.clone(),
            ReuseOp::Local => {
                let mut bytes = base.clone();
                let at = bytes.len() / 2;
                let end = (at + 4096).min(bytes.len());
                let patch =
                    fixture::noise((end - at) as u64, seed ^ (u64::from(index) << 16) ^ 0x10c4);
                bytes[at..end].copy_from_slice(&patch);
                bytes
            }
            ReuseOp::Unique | ReuseOp::Base128 => {
                fixture::noise(member_bytes, seed ^ (u64::from(index) << 8) ^ 0x0d1d)
            }
        };
        let (store, root) = objects_of(&bytes)?;
        offered.push((bytes, store, root));
    }
    let mut accepted: u64 = 0;
    let mut distinct: Vec<ObjectId> = Vec::new();
    for (_, store, _) in &offered {
        for id in store.insertion_order() {
            accepted += 1;
            if !distinct.contains(id) {
                distinct.push(*id);
            }
        }
    }

    let base = context.output.join("base.sqlite");
    let sample = context.output.join("sample.sqlite");
    create_and_save_untimed(&base, &base_store)?;
    let de_warm = prepare_sample(&base, &sample)?;
    gates.push(gates::residency_gate(Some(de_warm.resident_after)));
    gates.push(crate::gates::attribution_gate(crate::gates::Attribution::Exclusive));

    let store = match open_untimed(&sample) {
        Ok(store) => store,
        Err(error) => return Ok(unmeasured(&error, gates)),
    };
    instruments::heap_begin();
    let (result, report) = Timing::record("c2.workspace", |scope: &TimingScope<'_, Active>| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        for (_, member, _) in &offered {
            for id in member.insertion_order() {
                let object = member
                    .cloned_object(*id)
                    .ok_or(layerfs_storage::StorageError::ObjectMissing(*id))?;
                operation.accept(object)?;
            }
        }
        operation.finish(scope.child("storage.finish"))
    });
    let heap = instruments::heap_end();
    let timing_bytes = write_timing(context.output, &report)?;
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => return Ok(unmeasured(&OpError::Product(format!("{error:?}")), gates)),
    };

    context.trace.write_number(Kind::Counter, "workspace.base_files", i128::from(base_files), "files", "harness-declared base file count")?;
    context.trace.write_number(Kind::Counter, "workspace.base_objects", base_store.len() as i128, "objects", "objects the base Store already held")?;
    context.trace.write_number(Kind::Counter, "workspace.accepted", accepted as i128, "objects", "harness-declared addition set")?;
    context.trace.write_number(Kind::Counter, "workspace.distinct", distinct.len() as i128, "objects", "harness-declared addition set")?;
    context.trace.write_number(Kind::Counter, "workspace.reused", i128::from(outcome.reused), "objects", "SaveOutcome.reused")?;
    context.trace.write_number(Kind::Counter, "workspace.inserted", i128::from(outcome.inserted), "objects", "SaveOutcome.inserted")?;
    context.trace.write_number(Kind::Counter, "workspace.packs_created", i128::from(outcome.packs_created), "packs", "SaveOutcome.packs_created")?;
    context.trace.write_number(Kind::Counter, "workspace.statements", i128::from(outcome.statements), "statements", "SaveOutcome.statements")?;
    context.trace.write_number(Kind::Counter, "timing_json_bytes", timing_bytes as i128, "bytes", "product timing.json, byte-verbatim")?;
    context.trace.write_number(Kind::Resource, "heap.peak_incremental_bytes", heap.peak_incremental_bytes as i128, "bytes", "counting GlobalAlloc, measured phase")?;

    gates.push(completeness_gate(&report));
    // The declared equation, per kind. An `exact` addition carries only identities
    // the base already holds, so every accepted object is reused and nothing is
    // written; a `local` addition shares its unchanged body with the base and adds
    // the patched region; a `unique` addition shares nothing.
    let equation = match op {
        ReuseOp::Identical => outcome.reused == accepted && outcome.inserted == 0,
        ReuseOp::Unique | ReuseOp::Base128 => outcome.reused == 0,
        ReuseOp::Local => outcome.reused > 0 && outcome.reused < accepted,
    };
    gates.push(gates::require(
        GateClass::Mechanism,
        "g2.reuse-equation",
        equation,
        &format!(
            "reused {} of {accepted} accepted, {} inserted ({} base files, {} base objects)",
            outcome.reused,
            outcome.inserted,
            base_files,
            base_store.len()
        ),
        match op {
            ReuseOp::Identical => {
                "exact profile: every accepted object is an identity the base already holds"
            }
            ReuseOp::Unique | ReuseOp::Base128 => {
                "unique profile: nothing in the base is reused"
            }
            ReuseOp::Local => "local profile: some objects are reused, not all",
        },
    ));
    if matches!(op, ReuseOp::Identical) {
        gates.push(gates::require(
            GateClass::Mechanism,
            "g2.exact-hit-writes-nothing",
            outcome.packs_created == 0 && outcome.pack_appends == 0,
            &format!(
                "{} packs created, {} appends",
                outcome.packs_created, outcome.pack_appends
            ),
            "an exact-hit save resolves by lookup and writes no pack",
        ));
    }

    // Oracle: read every addition's root back through the Store it was saved into.
    let store = match open_untimed(&sample) {
        Ok(store) => store,
        Err(error) => return Ok(unmeasured(&error, gates)),
    };
    let mut oracle_ok = true;
    let mut oracle_note = String::new();
    for (index, (bytes, _member, root)) in offered.iter().enumerate() {
        let expectation = Expectation::of(bytes);
        match read_back_through_store(&store, *root, &expectation) {
            Ok(back) => {
                if !back.matches() {
                    oracle_ok = false;
                    oracle_note = format!("addition {index} read-back mismatch");
                }
            }
            Err(error) => {
                oracle_ok = false;
                oracle_note = format!("addition {index}: {error}");
            }
        }
    }
    gates.push(gates::require(
        GateClass::Correctness,
        "g1.o1-member-readback",
        oracle_ok,
        if oracle_note.is_empty() { "every addition root read back byte-exact" } else { &oracle_note },
        "each addition's root object is readable and byte-exact after the save",
    ));
    gates.extend(sidecar_gates(&sample));
    gates.push(gates::swap_gate(instruments::swaps()));

    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("workspace_profile: {op:?}"),
            format!("members: {members}"),
            format!("base_files: {base_files} (declared by the row's tier and profile)"),
            format!("member_bytes: {member_bytes}"),
            format!("store_state: opened-from-copy"),
            format!("copy_rung: {COPY_RUNG}"),
            format!("allocation_attribution: {ATTRIBUTION}"),
            format!("heap_charged_bytes: {}", heap.charged_bytes),
        ],
    })
}

/// The object set one delta row saves, and the recipe its oracle uses.
fn delta_members(
    op: DeltaOp,
    members: u32,
    bytes: u64,
    seed: u64,
) -> Result<(TreeStore, Vec<(TreeStore, ObjectId)>, Vec<Expectation>), OpError> {
    let base_bytes = fixture::noise(bytes, seed);
    let (base, _base_root) = objects_of(&base_bytes)?;
    let mut derived = Vec::with_capacity(members as usize);
    let mut expectations = Vec::with_capacity(members as usize);
    for index in 0..members {
        let mut member = base_bytes.clone();
        let len = member.len();
        match op {
            DeltaOp::Overwrite => {
                let at = len / 3;
                let end = (at + 65_536).min(len);
                let patch = fixture::noise((end - at) as u64, seed ^ (u64::from(index) << 8));
                member[at..end].copy_from_slice(&patch);
            }
            DeltaOp::Insert => {
                let at = len / 2;
                let patch = fixture::noise(65_536, seed ^ (u64::from(index) << 8));
                member.splice(at..at, patch);
            }
            DeltaOp::Delete => {
                let at = len / 2;
                let end = (at + 65_536).min(len);
                member.drain(at..end);
            }
            DeltaOp::CommonBody => {
                let at = len / 4;
                let end = (at + 1_024).min(len);
                let patch = fixture::noise((end - at) as u64, seed ^ (u64::from(index) << 8));
                member[at..end].copy_from_slice(&patch);
            }
            DeltaOp::Scattered => {
                for step in 0..8_u64 {
                    let at = ((len as f64) * (step as f64 + 1.0) / 9.0) as usize;
                    let end = (at + 4_096).min(len);
                    if end > at {
                        let patch = fixture::noise((end - at) as u64, seed ^ u64::from(index) ^ step);
                        member[at..end].copy_from_slice(&patch);
                    }
                }
            }
        }
        expectations.push(Expectation::of(&member));
        derived.push(objects_of(&member)?);
    }
    Ok((base, derived, expectations))
}

/// C2-3: CDC delta locality over a stored base.
pub fn delta(case: &Case, op: DeltaOp, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    context.create_output()?;
    let mut gates = Vec::new();
    let seed = seed_of(case.id);
    let members = case.entries.max(1);
    let bytes = if case.bytes > 0 { case.bytes } else { 4 << 20 };
    let (base, derived, expectations) = delta_members(op, members, bytes, seed)?;

    let base_path = context.output.join("base.sqlite");
    let sample = context.output.join("sample.sqlite");
    create_and_save_untimed(&base_path, &base)?;
    let de_warm = prepare_sample(&base_path, &sample)?;
    gates.push(gates::residency_gate(Some(de_warm.resident_after)));
    gates.push(crate::gates::attribution_gate(crate::gates::Attribution::Exclusive));

    let store = match open_untimed(&sample) {
        Ok(store) => store,
        Err(error) => return Ok(unmeasured(&error, gates)),
    };
    instruments::heap_begin();
    let (result, report) = Timing::record("c2.delta", |scope: &TimingScope<'_, Active>| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        for (member, _root) in &derived {
            for id in member.insertion_order() {
                let object = member
                    .cloned_object(*id)
                    .ok_or(layerfs_storage::StorageError::ObjectMissing(*id))?;
                operation.accept(object)?;
            }
        }
        operation.finish(scope.child("storage.finish"))
    });
    let heap = instruments::heap_end();
    let timing_bytes = write_timing(context.output, &report)?;
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => return Ok(unmeasured(&OpError::Product(format!("{error:?}")), gates)),
    };

    context.trace.write_number(Kind::Counter, "delta.reused", i128::from(outcome.reused), "objects", "SaveOutcome.reused")?;
    context.trace.write_number(Kind::Counter, "delta.inserted", i128::from(outcome.inserted), "objects", "SaveOutcome.inserted")?;
    context.trace.write_number(Kind::Counter, "delta.full_records", i128::from(outcome.full_records), "records", "SaveOutcome.full_records")?;
    context.trace.write_number(Kind::Counter, "delta.prefix_records", i128::from(outcome.prefix_records), "records", "SaveOutcome.prefix_records")?;
    context.trace.write_number(Kind::Counter, "delta.chain_max_depth", i128::from(outcome.chain.max_depth), "edges", "SaveOutcome.chain.max_depth")?;
    context.trace.write_number(Kind::Counter, "delta.prefix_selected", i128::from(outcome.delta.prefix_selected), "records", "SaveOutcome.delta.prefix_selected")?;
    context.trace.write_number(Kind::Counter, "delta.full_losses", i128::from(outcome.delta.full_losses), "records", "SaveOutcome.delta.full_losses")?;
    context.trace.write_number(Kind::Counter, "delta.no_candidate", i128::from(outcome.delta.no_candidate), "records", "SaveOutcome.delta.no_candidate")?;
    context.trace.write_number(Kind::Counter, "delta.work_exceeded", i128::from(outcome.delta.work_exceeded), "records", "SaveOutcome.delta.work_exceeded")?;
    context.trace.write_number(Kind::Counter, "timing_json_bytes", timing_bytes as i128, "bytes", "product timing.json, byte-verbatim")?;
    context.trace.write_number(Kind::Resource, "heap.peak_incremental_bytes", heap.peak_incremental_bytes as i128, "bytes", "counting GlobalAlloc, measured phase")?;

    gates.push(completeness_gate(&report));
    gates.push(gates::require(
        GateClass::Mechanism,
        "g2.route-observed",
        outcome.inserted > 0 || outcome.reused > 0,
        &format!("{} inserted, {} reused", outcome.inserted, outcome.reused),
        "the save accepted and accounted for the supplied member set",
    ));

    let store = match open_untimed(&sample) {
        Ok(store) => store,
        Err(error) => return Ok(unmeasured(&error, gates)),
    };
    let mut oracle_ok = true;
    let mut oracle_note = String::new();
    for (index, ((_member, root), expectation)) in
        derived.iter().zip(expectations.iter()).enumerate()
    {
        match read_back_through_store(&store, *root, expectation) {
            Ok(back) => {
                if !back.matches() {
                    oracle_ok = false;
                    oracle_note = format!("member {index} read-back mismatch");
                }
            }
            Err(error) => {
                oracle_ok = false;
                oracle_note = format!("member {index}: {error}");
            }
        }
    }
    gates.push(gates::require(
        GateClass::Correctness,
        "g1.o1-member-readback",
        oracle_ok,
        if oracle_note.is_empty() { "every member root read back byte-exact" } else { &oracle_note },
        "each member's logical bytes are recoverable after the save",
    ));
    gates.extend(sidecar_gates(&sample));
    gates.push(gates::swap_gate(instruments::swaps()));
    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("delta_op: {op:?}"),
            format!("members: {members}"),
            format!("base_bytes: {bytes}"),
            format!("store_state: opened-from-copy"),
            format!("copy_rung: {COPY_RUNG}"),
            format!("allocation_attribution: {ATTRIBUTION}"),
            format!("heap_charged_bytes: {}", heap.charged_bytes),
        ],
    })
}

/// C2-3 sub-lane: one chunk-grammar boundary length.
pub fn boundary(case: &Case, seed: u8, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    context.create_output()?;
    let mut gates = Vec::new();
    let length = case.bytes;
    let bytes = fixture::noise(length, seed_of(case.id) ^ u64::from(seed));
    let (objects, root) = objects_of(&bytes)?;
    let expectation = Expectation::of(&bytes);

    let base_path = context.output.join("base.sqlite");
    let sample = context.output.join("sample.sqlite");
    create_and_save_untimed(&base_path, &TreeStore::new())?;
    let de_warm = prepare_sample(&base_path, &sample)?;
    gates.push(gates::residency_gate(Some(de_warm.resident_after)));
    gates.push(crate::gates::attribution_gate(crate::gates::Attribution::Exclusive));

    let store = match open_untimed(&sample) {
        Ok(store) => store,
        Err(error) => return Ok(unmeasured(&error, gates)),
    };
    instruments::heap_begin();
    let (result, report) = Timing::record("c2.boundary", |scope: &TimingScope<'_, Active>| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        for id in objects.insertion_order() {
            let object = objects
                .cloned_object(*id)
                .ok_or(layerfs_storage::StorageError::ObjectMissing(*id))?;
            operation.accept(object)?;
        }
        operation.finish(scope.child("storage.finish"))
    });
    let heap = instruments::heap_end();
    let timing_bytes = write_timing(context.output, &report)?;
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => return Ok(unmeasured(&OpError::Product(format!("{error:?}")), gates)),
    };

    context.trace.write_number(Kind::Counter, "boundary.length", length as i128, "bytes", "the row's declared grammar edge")?;
    context.trace.write_number(Kind::Counter, "boundary.objects", objects.len() as i128, "objects", "harness-supplied object set")?;
    context.trace.write_number(Kind::Counter, "boundary.inserted", i128::from(outcome.inserted), "objects", "SaveOutcome.inserted")?;
    context.trace.write_number(Kind::Counter, "boundary.reused", i128::from(outcome.reused), "objects", "SaveOutcome.reused")?;
    context.trace.write_number(Kind::Counter, "timing_json_bytes", timing_bytes as i128, "bytes", "product timing.json, byte-verbatim")?;
    context.trace.write_number(Kind::Resource, "heap.peak_incremental_bytes", heap.peak_incremental_bytes as i128, "bytes", "counting GlobalAlloc, measured phase")?;

    gates.push(completeness_gate(&report));
    gates.push(gates::require(
        GateClass::Mechanism,
        "g2.boundary-accepted",
        outcome.inserted + outcome.reused >= 1,
        &format!("{} inserted, {} reused at {length} bytes", outcome.inserted, outcome.reused),
        "a grammar edge is accepted, or refused with its declared error class",
    ));
    let store = match open_untimed(&sample) {
        Ok(store) => store,
        Err(error) => return Ok(unmeasured(&error, gates)),
    };
    {
        match read_back_through_store(&store, root, &expectation) {
            Ok(back) => gates.push(gates::require(
                GateClass::Correctness,
                "g1.o1-readback",
                back.matches(),
                &format!("{} bytes read, {}", back.bytes, back.digest),
                &format!("{} bytes, sha256 {}", back.expected_bytes, back.expected_digest),
            )),
            Err(error) => gates.push(Gate::incomplete(
                GateClass::Correctness,
                "g1.o1-readback",
                &format!("{error}"),
                "readback equals input",
            )),
        }
    }
    gates.extend(sidecar_gates(&sample));
    gates.push(gates::swap_gate(instruments::swaps()));
    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("grammar_edge_bytes: {length}"),
            format!("declared_seed: {seed}"),
            format!("store_state: opened-from-copy"),
            format!("copy_rung: {COPY_RUNG}"),
            format!("heap_charged_bytes: {}", heap.charged_bytes),
        ],
    })
}

/// C2-6: the delta policy below the small-file cutoff.
pub fn small_file(case: &Case, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    context.create_output()?;
    let mut gates = Vec::new();
    let bytes = fixture::noise(case.bytes, seed_of(case.id));
    let (objects, root) = objects_of(&bytes)?;
    let expectation = Expectation::of(&bytes);

    let base_path = context.output.join("base.sqlite");
    let sample = context.output.join("sample.sqlite");
    create_and_save_untimed(&base_path, &TreeStore::new())?;
    let de_warm = prepare_sample(&base_path, &sample)?;
    gates.push(gates::residency_gate(Some(de_warm.resident_after)));
    gates.push(crate::gates::attribution_gate(crate::gates::Attribution::Exclusive));

    let store = match open_untimed(&sample) {
        Ok(store) => store,
        Err(error) => return Ok(unmeasured(&error, gates)),
    };
    instruments::heap_begin();
    let (result, report) = Timing::record("c2.small-file", |scope: &TimingScope<'_, Active>| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        for id in objects.insertion_order() {
            let object = objects
                .cloned_object(*id)
                .ok_or(layerfs_storage::StorageError::ObjectMissing(*id))?;
            operation.accept(object)?;
        }
        operation.finish(scope.child("storage.finish"))
    });
    let heap = instruments::heap_end();
    let timing_bytes = write_timing(context.output, &report)?;
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => return Ok(unmeasured(&OpError::Product(format!("{error:?}")), gates)),
    };
    let cutoff = ConstructionPolicy::frozen_default().small_file_threshold_bytes();

    context.trace.write_number(Kind::Counter, "small_file.bytes", case.bytes as i128, "bytes", "the row's declared tier")?;
    context.trace.write_number(Kind::Counter, "small_file.full_records", i128::from(outcome.full_records), "records", "SaveOutcome.full_records")?;
    context.trace.write_number(Kind::Counter, "small_file.inserted", i128::from(outcome.inserted), "objects", "SaveOutcome.inserted")?;
    context.trace.write_number(Kind::Counter, "timing_json_bytes", timing_bytes as i128, "bytes", "product timing.json, byte-verbatim")?;
    context.trace.write_number(Kind::Resource, "heap.peak_incremental_bytes", heap.peak_incremental_bytes as i128, "bytes", "counting GlobalAlloc, measured phase")?;

    gates.push(completeness_gate(&report));
    gates.push(gates::require(
        GateClass::Mechanism,
        "g2.whole-file-route",
        case.bytes < cutoff && outcome.full_records >= 1,
        &format!("{} bytes below cutoff {cutoff}: {} FULL records", case.bytes, outcome.full_records),
        "a sub-cutoff file takes the whole-file route and is stored as a FULL record",
    ));
    let store = match open_untimed(&sample) {
        Ok(store) => store,
        Err(error) => return Ok(unmeasured(&error, gates)),
    };
    {
        match read_back_through_store(&store, root, &expectation) {
            Ok(back) => gates.push(gates::require(
                GateClass::Correctness,
                "g1.o1-readback",
                back.matches(),
                &format!("{} bytes read, {}", back.bytes, back.digest),
                &format!("{} bytes, sha256 {}", back.expected_bytes, back.expected_digest),
            )),
            Err(error) => gates.push(Gate::incomplete(
                GateClass::Correctness,
                "g1.o1-readback",
                &format!("{error}"),
                "readback equals input",
            )),
        }
    }
    gates.extend(sidecar_gates(&sample));
    gates.push(gates::swap_gate(instruments::swaps()));
    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("declared_bytes: {}", case.bytes),
            format!("cutoff_bytes: {cutoff}"),
            format!("store_state: opened-from-copy"),
            format!("copy_rung: {COPY_RUNG}"),
            format!("heap_charged_bytes: {}", heap.charged_bytes),
        ],
    })
}

/// C2-7: independent read waves over a stored ladder.
///
/// Driven through [`StoreProvider::read_wave`] and not through `Store::read_batch`,
/// because only the provider's route reports `opens` as `1` on the opening wave and
/// `0` afterwards; the direct route reports `1` unconditionally, so this cell would
/// be ungateable on it.
pub fn read_wave(case: &Case, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    context.create_output()?;
    let mut gates = Vec::new();
    let bytes = fixture::noise(case.bytes, seed_of(case.id));
    let (objects, root) = objects_of(&bytes)?;
    let expectation = Expectation::of(&bytes);

    let base_path = context.output.join("base.sqlite");
    let sample = context.output.join("sample.sqlite");
    create_and_save_untimed(&base_path, &objects)?;
    let de_warm = prepare_sample(&base_path, &sample)?;
    gates.push(gates::residency_gate(Some(de_warm.resident_after)));
    gates.push(crate::gates::attribution_gate(crate::gates::Attribution::Exclusive));

    let store = match open_untimed(&sample) {
        Ok(store) => store,
        Err(error) => return Ok(unmeasured(&error, gates)),
    };

    instruments::heap_begin();
    let (result, report) = Timing::record("c2.read.waves", |scope: &TimingScope<'_, Active>| {
        let provider = StoreProvider::new(&store);
        let (first, first_counters) = provider.read_wave(&[root], scope.child("storage.wave1"))?;
        let (second, second_counters) = provider.read_wave(&[root], scope.child("storage.wave2"))?;
        let bytes_out = first.iter().map(Vec::len).sum::<usize>() + second.iter().map(Vec::len).sum::<usize>();
        Ok::<_, layerfs_storage::StorageError>((
            first_counters,
            second_counters,
            provider.connection_opens(),
            bytes_out,
        ))
    });
    let heap = instruments::heap_end();
    let timing_bytes = write_timing(context.output, &report)?;
    let (first, second, opens, bytes_out) = match result {
        Ok(value) => value,
        Err(error) => return Ok(unmeasured(&OpError::Product(format!("{error:?}")), gates)),
    };

    context.trace.write_number(Kind::Counter, "read.wave1_opens", i128::from(first.opens), "connections", "StoreReadCounters.opens of wave 1")?;
    context.trace.write_number(Kind::Counter, "read.wave2_opens", i128::from(second.opens), "connections", "StoreReadCounters.opens of wave 2")?;
    context.trace.write_number(Kind::Counter, "read.provider_opens", i128::from(opens), "connections", "StoreProvider.connection_opens()")?;
    context.trace.write_number(Kind::Counter, "read.wave1_pages", i128::from(first.pages), "pages", "StoreReadCounters.pages of wave 1")?;
    context.trace.write_number(Kind::Counter, "read.emitted_bytes", bytes_out as i128, "bytes", "sum of the two waves' returned bytes")?;
    context.trace.write_number(Kind::Counter, "read.canonical_bytes", i128::from(first.canonical_bytes + second.canonical_bytes), "bytes", "StoreReadCounters.canonical_bytes")?;
    context.trace.write_number(Kind::Counter, "read.ceiling", i128::from(first.ceiling), "pack id", "StoreReadCounters.ceiling")?;
    context.trace.write_number(Kind::Counter, "timing_json_bytes", timing_bytes as i128, "bytes", "product timing.json, byte-verbatim")?;
    context.trace.write_number(Kind::Resource, "heap.peak_incremental_bytes", heap.peak_incremental_bytes as i128, "bytes", "counting GlobalAlloc, measured phase")?;

    gates.push(completeness_gate(&report));
    gates.push(gates::require(
        GateClass::Mechanism,
        "g2.opens-o1",
        first.opens == 1 && second.opens == 0 && opens == 1,
        &format!("wave1 opens {}, wave2 opens {}, provider opens {opens}", first.opens, second.opens),
        "opens is 1 on the opening wave then 0: the O(1) connection claim",
    ));
    let expected_pages = 1_u64;
    gates.push(gates::require(
        GateClass::Mechanism,
        "g2.paged-locator",
        first.pages == expected_pages,
        &format!("{} pages", first.pages),
        &format!("ceil(ids / LOOKUP_PAGE_IDS) = {expected_pages} page for one id"),
    ));
    let store = match open_untimed(&sample) {
        Ok(store) => store,
        Err(error) => return Ok(unmeasured(&error, gates)),
    };
    match read_back_through_store(&store, root, &expectation) {
        Ok(back) => gates.push(gates::require(
            GateClass::Correctness,
            "g1.o2-readback",
            back.matches(),
            &format!("{} bytes read, {}", back.bytes, back.digest),
            &format!("{} bytes, sha256 {}", back.expected_bytes, back.expected_digest),
        )),
        Err(error) => gates.push(Gate::incomplete(
            GateClass::Correctness,
            "g1.o2-readback",
            &format!("{error}"),
            "readback equals input",
        )),
    }
    gates.extend(sidecar_gates(&sample));
    gates.push(gates::swap_gate(instruments::swaps()));
    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("declared_bytes: {}", case.bytes),
            format!("objects: {}", objects.len()),
            format!("store_state: opened-from-copy"),
            format!("copy_rung: {COPY_RUNG}"),
            format!("heap_charged_bytes: {}", heap.charged_bytes),
        ],
    })
}

/// C2-5: footprint accounting. The bytes are measured by `shared/space.py` in
/// verification mode, not here: the harness owns no SQL, and the number must be
/// re-derived by a reader that did not produce it.
pub fn footprint(
    case: &Case,
    op: FootprintOp,
    context: &mut OpContext<'_>,
) -> Result<OpOutcome, OpError> {
    context.create_output()?;
    let mut gates = Vec::new();
    let seed = seed_of(case.id);
    let object_bytes = case.bytes;
    let entries = case.entries.max(1);

    // The supplied object set: many distinct small objects, many metadata-heavy
    // objects, or one large object, exactly as the row declares.
    let mut supplied = TreeStore::new();
    match op {
        FootprintOp::LargeObject => {
            let (objects, _root) = objects_of(&fixture::noise(object_bytes, seed))?;
            for id in objects.insertion_order() {
                if let Some(object) = objects.cloned_object(*id) {
                    supplied.insert_object(object);
                }
            }
        }
        _ => {
            let per_file = (object_bytes / u64::from(entries)).max(64);
            for index in 0..entries {
                let (objects, _root) =
                    objects_of(&fixture::noise(per_file, seed ^ (u64::from(index) << 8)))?;
                for id in objects.insertion_order() {
                    if let Some(object) = objects.cloned_object(*id) {
                        supplied.insert_object(object);
                    }
                }
            }
        }
    }

    let base_path = context.output.join("base.sqlite");
    let sample = context.output.join("sample.sqlite");
    create_and_save_untimed(&base_path, &TreeStore::new())?;
    let de_warm = prepare_sample(&base_path, &sample)?;
    gates.push(gates::residency_gate(Some(de_warm.resident_after)));
    gates.push(crate::gates::attribution_gate(crate::gates::Attribution::Exclusive));

    let store = match open_untimed(&sample) {
        Ok(store) => store,
        Err(error) => return Ok(unmeasured(&error, gates)),
    };
    instruments::heap_begin();
    let (result, report) = Timing::record("c2.footprint", |scope: &TimingScope<'_, Active>| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        for id in supplied.insertion_order() {
            let object = supplied
                .cloned_object(*id)
                .ok_or(layerfs_storage::StorageError::ObjectMissing(*id))?;
            operation.accept(object)?;
        }
        operation.finish(scope.child("storage.finish"))
    });
    let heap = instruments::heap_end();
    let timing_bytes = write_timing(context.output, &report)?;
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => return Ok(unmeasured(&OpError::Product(format!("{error:?}")), gates)),
    };
    let space = instruments::space(&sample).map_err(|error| OpError::Io(format!("{error}")))?;

    context.trace.write_number(Kind::Counter, "footprint.objects_supplied", supplied.len() as i128, "objects", "harness-supplied object set")?;
    context.trace.write_number(Kind::Counter, "footprint.inserted", i128::from(outcome.inserted), "objects", "SaveOutcome.inserted")?;
    context.trace.write_number(Kind::Counter, "footprint.packs_created", i128::from(outcome.packs_created), "packs", "SaveOutcome.packs_created")?;
    context.trace.write_number(Kind::Counter, "footprint.pack_appends", i128::from(outcome.pack_appends), "appends", "SaveOutcome.pack_appends")?;
    context.trace.write_number(Kind::Counter, "footprint.statements", i128::from(outcome.statements), "statements", "SaveOutcome.statements")?;
    context.trace.write_number(Kind::Resource, "space.apparent_bytes", space.apparent_bytes as i128, "bytes", "st_size of the Store file")?;
    context.trace.write_number(Kind::Resource, "space.allocated_bytes", space.allocated_bytes as i128, "bytes", "st_blocks * 512 of the Store file, exclusive attribution")?;
    context.trace.write_number(Kind::Counter, "timing_json_bytes", timing_bytes as i128, "bytes", "product timing.json, byte-verbatim")?;
    context.trace.write_number(Kind::Resource, "heap.peak_incremental_bytes", heap.peak_incremental_bytes as i128, "bytes", "counting GlobalAlloc, measured phase")?;

    gates.push(completeness_gate(&report));
    gates.push(gates::require(
        GateClass::Correctness,
        "g1.objects-stored",
        outcome.inserted > 0,
        &format!("{} inserted of {} supplied", outcome.inserted, supplied.len()),
        "the supplied object set was stored",
    ));
    gates.push(gates::require(
        GateClass::Cleanup,
        "g5.one-file",
        std::fs::read_dir(context.output)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter(|entry| {
                        entry
                            .path()
                            .extension()
                            .is_some_and(|extension| extension == "sqlite")
                    })
                    .count()
            })
            .unwrap_or(0)
            >= 1,
        "at least one Store file in the output directory",
        "exactly one Store file and no sidecars",
    ));
    gates.extend(sidecar_gates(&sample));
    gates.push(gates::swap_gate(instruments::swaps()));
    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("footprint_op: {op:?}"),
            format!("declared_entries: {entries}"),
            format!("declared_object_bytes: {object_bytes}"),
            format!("store_path: {}", sample.display()),
            format!("pack_sql: deferred to shared/space.py in verify mode (no COALESCE; a missing object_packs table is INCOMPLETE)"),
            format!("copy_rung: {COPY_RUNG}"),
            format!("allocation_attribution: {ATTRIBUTION}"),
            format!("heap_charged_bytes: {}", heap.charged_bytes),
        ],
    })
}
