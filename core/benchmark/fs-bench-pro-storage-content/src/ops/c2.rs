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

use std::path::{Path, PathBuf};

use layerfs_content::{
    construct_bytes, ConstructionPolicy, ConstructionCapacities, ObjectId,
};
use layerfs_storage::{SaveOutcome, StoragePolicy, Store, StoreProvider};
use layerfs_telemetry::timer::{Active, Timing, TimingScope, TimingReport};

use super::{seed_of, OpContext, OpError, OpOutcome, Phase};
use crate::fixture;
use crate::gates::{self, Gate, GateClass};
use crate::registry::{Case, DeltaOp, FootprintOp, LifecycleStep, ReuseOp};
use crate::support::instruments;
use crate::support::trace::Kind;
use crate::workload::oracle::{self, Expectation};
use crate::workload::artifact::{Artifact, Member, Values};
use crate::workload::providers::TreeStore;

/// The declared copy rung and its allocation attribution.
pub const COPY_RUNG: &str = "closed-quiescent-byte-copy";
/// Attribution every C2 row carries. A clone's blocks are shared with its master.
pub const ATTRIBUTION: &str = "exclusive";

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
pub(super) fn open_untimed(path: &Path) -> Result<Store, OpError> {
    let (result, _) = Timing::disabled("setup.open", |scope: &TimingScope<'_, Active>| {
        Store::open(path, scope.child("store.open"))
    });
    result.map_err(|error| OpError::Product(format!("{error:?}")))
}

/// Creates a Store without a timer and saves `objects` into it.
pub(super) fn create_and_save_untimed(path: &Path, objects: &TreeStore) -> Result<SaveOutcome, OpError> {
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
///
/// This is the per-sample **acquisition**, and owner decision D2 makes its cost a
/// mandatory published field rather than an invisible part of setup. Its span is
/// charged to `acquisition_wall_ns`, which is a subset of `preparation_wall_ns`:
/// the copy is preparation, and it is also the one part of preparation the rules
/// single out by name.
pub(super) fn prepare_sample(base: &Path, sample: &Path) -> Result<instruments::DeWarmReport, OpError> {
    let started = std::time::Instant::now();
    std::fs::copy(base, sample).map_err(|error| {
        OpError::Io(format!("{} -> {}: {error}", base.display(), sample.display()))
    })?;
    // `std::fs::copy` copies the source's permission bits, and a sealed master is
    // 0o444 by construction (`test_setup_and_cache_discipline.md` section 3). The
    // sample is the row's own writable copy — section 4 step 4 of the same document
    // is `chmod 0600` — so the copy is made writable here rather than left as a
    // read-only alias of an immutable master, which the product correctly refuses.
    set_owner_writable(sample)?;
    let report =
        instruments::de_warm(sample).map_err(|error| OpError::Io(format!("de-warm: {error}")))?;
    crate::support::phases::add_acquisition(started.elapsed().as_nanos() as u64);
    Ok(report)
}

/// Makes one file owner-readable and owner-writable, refusing anything else.
fn set_owner_writable(path: &Path) -> Result<(), OpError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(path)
            .map_err(|error| OpError::Io(format!("{}: {error}", path.display())))?
            .permissions()
            .mode();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode | 0o600))
            .map_err(|error| OpError::Io(format!("{}: {error}", path.display())))?;
    }
    #[cfg(not(unix))]
    {
        let mut permissions = std::fs::metadata(path)
            .map_err(|error| OpError::Io(format!("{}: {error}", path.display())))?
            .permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        std::fs::set_permissions(path, permissions)
            .map_err(|error| OpError::Io(format!("{}: {error}", path.display())))?;
    }
    Ok(())
}

/// Sidecar gates for a Store path: `journal_mode = MEMORY` leaves none.
pub(super) fn sidecar_gates(path: &Path) -> Vec<Gate> {
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
pub(super) fn read_back_through_store(
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
    let (result, report) = super::measure("c2.lifecycle", |scope: &TimingScope<'_, Active>| {
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
                // The declared wait state a calibration arm can ask for. The save
                // is held, not aborted, until the arm's killer takes the process.
                let held = context
                    .hold_at(crate::ops::HoldPoint::Begin)
                    .map_err(|_| layerfs_storage::StorageError::Integrity("hold marker"))?;
                let _ = held;
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
    let timing_bytes = crate::support::phases::timing_json_bytes();
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
///
/// **Phase split.** The row measures one `begin_save`/`accept`/`finish` against an
/// empty Store; the member set is a fixture, and building it means generating up to
/// 500 MiB and hashing every member to state the expectation the oracle compares
/// against. The registry declares `Preparation::ObjectSet`, so the member set and
/// its expectations are acquired once.
pub fn reuse(case: &Case, op: ReuseOp, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    match context.phase {
        Phase::Prepare => reuse_prepare(case, op, context),
        Phase::Perf => reuse_perf(case, op, context),
        Phase::Verify => Err(OpError::Io(
            "c2.reuse.cross-file declares an in-process oracle and has no deferred verification phase"
                .to_string(),
        )),
    }
}

/// The member bytes one cross-file row offers, by profile.
fn reuse_member_bytes(op: ReuseOp, index: u32, member_bytes: u64, seed: u64) -> Vec<u8> {
    match op {
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
    }
}

/// C2-2 `prepare`: acquire the member set once, with every expectation it gates.
fn reuse_prepare(case: &Case, op: ReuseOp, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    let directory = context.artifact_directory()?;
    let seed = seed_of(case.id);
    let members = case.entries.max(1);
    let member_bytes = if case.bytes > 0 { case.bytes } else { 1 << 20 };
    let mut objects = TreeStore::new();
    let mut list: Vec<Member> = Vec::with_capacity(members as usize);
    for index in 0..members {
        let bytes = reuse_member_bytes(op, index, member_bytes, seed);
        let (store, root) = objects_of(&bytes)?;
        let ids = store.insertion_order().to_vec();
        objects.absorb(&store);
        list.push(Member {
            ids,
            root,
            expectation: Some(Expectation::of(&bytes)),
        });
    }
    let mut artifact = Artifact::new(objects);
    artifact.members = list;
    artifact.values.set_scalar("members", u64::from(members));
    artifact.values.set_scalar("member_bytes", member_bytes);
    let written = artifact
        .write(&directory)
        .map_err(|error| OpError::Io(format!("{}: {error}", directory.display())))?;
    artifact
        .seal(&directory, case.id)
        .map_err(|error| OpError::Io(format!("{}: {error}", directory.display())))?;
    context.trace.write_number(Kind::Counter, "prepare.objects", written as i128, "objects", "distinct canonical objects the artifact holds")?;
    context.trace.write_number(Kind::Counter, "prepare.members", artifact.members.len() as i128, "members", "member set the artifact declares")?;
    Ok(OpOutcome {
        gates: Vec::new(),
        notes: vec![
            format!("prepare_op: {op:?}"),
            format!("prepare_members: {members}"),
            format!("prepare_member_bytes: {member_bytes}"),
            format!("artifact: {}", directory.display()),
            "acquisition: not a measured phase; charged to acquisition_wall_ns".to_string(),
        ],
    })
}

/// C2-2 `perf`: the measured save against the prepared member set.
fn reuse_perf(case: &Case, op: ReuseOp, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    context.create_output()?;
    let mut gates = Vec::new();
    let directory = context.artifact_directory()?;
    let artifact = Artifact::read(&directory)
        .map_err(|error| OpError::Io(format!("{}: {error}", directory.display())))?;
    let members = case.entries.max(1);
    let member_bytes = artifact
        .values
        .scalar("member_bytes")
        .ok_or_else(|| OpError::Io("the artifact declares no member_bytes".to_string()))?;

    // The member set is declared by the row's own profile, so the reuse equation
    // below is falsifiable rather than descriptive. `accepted` and `distinct` are
    // read off the artifact's own member lists: an identity is one the member
    // offers, and the count is the same whether it is computed here or at
    // acquisition.
    let mut accepted: u64 = 0;
    let mut distinct: Vec<ObjectId> = Vec::new();
    for member in &artifact.members {
        for id in &member.ids {
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
    let (result, report) = super::measure("c2.reuse", |scope: &TimingScope<'_, Active>| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        for member in &artifact.members {
            for id in &member.ids {
                let object = artifact
                    .objects
                    .cloned_object(*id)
                    .ok_or(layerfs_storage::StorageError::ObjectMissing(*id))?;
                operation.accept(object)?;
            }
        }
        // The second declared kill point: every offered object is accepted and the
        // publication watermark transaction has not committed, so the Store holds
        // packs that are neither published nor deleted. `--hold-save` is harness
        // argv; a row that was not asked to hold passes straight through.
        context
            .hold_at(crate::ops::HoldPoint::Accept)
            .map_err(|_| layerfs_storage::StorageError::Integrity("hold marker"))?;
        operation.finish(scope.child("storage.finish"))
    });
    let heap = instruments::heap_end();
    let timing_bytes = crate::support::phases::timing_json_bytes();
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

    // Oracle: read each member's root back through the Store it was saved into.
    //
    // The frozen oracle for this family **does** include the byte-exact read-back,
    // so it stays - but a read-back is a pure function of `(root, expectation)`, and
    // an `identical` profile offers the same member 128 times. Verifying every
    // distinct pair once and then asserting that every member's pair is one of the
    // verified ones is **complete**: it covers every member, and it does not decode
    // the same 1 MiB 128 times to say so.
    //
    // The expectation is the artifact's, derived from the fixture recipe at
    // acquisition. Recomputing it here cost a second SHA-256 over every member's
    // bytes - 1 GiB of harness hashing on the 500-entry tier - to arrive at a digest
    // that was already known.
    let roots: Vec<ObjectId> = artifact.members.iter().map(|member| member.root).collect();
    crate::workload::expected::publish(
        context.trace,
        "additions",
        &crate::workload::expected::identity_digest(&roots),
    )?;
    let store = match open_untimed(&sample) {
        Ok(store) => store,
        Err(error) => return Ok(unmeasured(&error, gates)),
    };
    let mut pairs: Vec<(ObjectId, Expectation)> = Vec::new();
    for member in &artifact.members {
        let Some(expectation) = member.expectation else {
            return Ok(unmeasured(
                &OpError::Io(format!(
                    "member {} carries no expectation; this row's oracle needs one",
                    member.root
                )),
                gates,
            ));
        };
        if !pairs
            .iter()
            .any(|(seen, seen_expectation)| *seen == member.root && *seen_expectation == expectation)
        {
            pairs.push((member.root, expectation));
        }
    }
    let mut oracle_ok = true;
    let mut oracle_note = String::new();
    for (index, (root, expectation)) in pairs.iter().enumerate() {
        match read_back_through_store(&store, *root, expectation) {
            Ok(back) => {
                if !back.matches() {
                    oracle_ok = false;
                    oracle_note = format!("distinct member {index} read-back mismatch");
                }
            }
            Err(error) => {
                oracle_ok = false;
                oracle_note = format!("distinct member {index}: {error}");
            }
        }
    }
    let covered = artifact.members.iter().all(|member| {
        member.expectation.is_some_and(|expectation| {
            pairs
                .iter()
                .any(|(seen, seen_expectation)| *seen == member.root && *seen_expectation == expectation)
        })
    });
    context.trace.write_number(
        Kind::Counter,
        "reuse.distinct_readbacks",
        pairs.len() as i128,
        "read-backs",
        "distinct (root, expectation) pairs the oracle verified",
    )?;
    let summary = format!(
        "{} distinct pair(s) read back byte-exact; every one of {} members covered",
        pairs.len(),
        artifact.members.len()
    );
    gates.push(gates::require(
        GateClass::Correctness,
        "g1.o1-member-readback",
        oracle_ok && covered,
        if oracle_note.is_empty() { &summary } else { &oracle_note },
        "every member's logical bytes are recoverable after the save",
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
///
/// **Phase split.** The row measures one save against a prepared Store; both the
/// base the Store already holds and the additions are fixtures. The registry
/// declares `Preparation::BaseStore`, so the base Store, the addition set and every
/// addition's expectation are acquired once.
pub fn workspace(case: &Case, op: ReuseOp, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    match context.phase {
        Phase::Prepare => workspace_prepare(case, op, context),
        Phase::Perf => workspace_perf(case, op, context),
        Phase::Verify => Err(OpError::Io(
            "c2.reuse.workspace declares an in-process oracle and has no deferred verification phase"
                .to_string(),
        )),
    }
}

/// The addition bytes one workspace row offers, by profile.
fn workspace_member_bytes(
    op: ReuseOp,
    index: u32,
    base: &[u8],
    member_bytes: u64,
    seed: u64,
) -> Vec<u8> {
    match op {
        ReuseOp::Identical => base.to_vec(),
        ReuseOp::Local => {
            let mut bytes = base.to_vec();
            let at = bytes.len() / 2;
            let end = (at + 4096).min(bytes.len());
            let patch = fixture::noise((end - at) as u64, seed ^ (u64::from(index) << 16) ^ 0x10c4);
            bytes[at..end].copy_from_slice(&patch);
            bytes
        }
        ReuseOp::Unique | ReuseOp::Base128 => {
            fixture::noise(member_bytes, seed ^ (u64::from(index) << 8) ^ 0x0d1d)
        }
    }
}

/// C2-4 `prepare`: acquire the base Store and the addition set once.
fn workspace_prepare(case: &Case, op: ReuseOp, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    let directory = context.artifact_directory()?;
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

    let mut objects = TreeStore::new();
    objects.absorb(&base_store);
    let mut list: Vec<Member> = Vec::with_capacity(members as usize);
    for index in 0..members {
        let base = &base_bytes[(index % base_files) as usize];
        let bytes = workspace_member_bytes(op, index, base, member_bytes, seed);
        let (store, root) = objects_of(&bytes)?;
        let ids = store.insertion_order().to_vec();
        objects.absorb(&store);
        list.push(Member {
            ids,
            root,
            // The frozen oracle for this family is **O1 + O5** - *"root matches"*
            // and the reuse counts - so no member expectation is read here either.
            expectation: None,
        });
    }
    create_and_save_untimed(&Artifact::store_path(&directory), &base_store)?;
    let mut artifact = Artifact::new(objects);
    artifact.members = list;
    artifact.values.set_scalar("members", u64::from(members));
    artifact.values.set_scalar("member_bytes", member_bytes);
    artifact.values.set_scalar("base_files", u64::from(base_files));
    artifact.values.set_scalar("base_objects", base_store.len() as u64);
    let written = artifact
        .write(&directory)
        .map_err(|error| OpError::Io(format!("{}: {error}", directory.display())))?;
    artifact
        .seal(&directory, case.id)
        .map_err(|error| OpError::Io(format!("{}: {error}", directory.display())))?;
    context.trace.write_number(Kind::Counter, "prepare.objects", written as i128, "objects", "distinct canonical objects the artifact holds")?;
    context.trace.write_number(Kind::Counter, "prepare.members", artifact.members.len() as i128, "members", "member set the artifact declares")?;
    Ok(OpOutcome {
        gates: Vec::new(),
        notes: vec![
            format!("prepare_op: {op:?}"),
            format!("prepare_members: {members}"),
            format!("prepare_base_files: {base_files}"),
            format!("prepare_member_bytes: {member_bytes}"),
            format!("artifact: {}", directory.display()),
            "acquisition: not a measured phase; charged to acquisition_wall_ns".to_string(),
        ],
    })
}

/// C2-4 `perf`: the measured save against the prepared base Store.
fn workspace_perf(case: &Case, op: ReuseOp, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    context.create_output()?;
    let mut gates = Vec::new();
    let directory = context.artifact_directory()?;
    let artifact = Artifact::read(&directory)
        .map_err(|error| OpError::Io(format!("{}: {error}", directory.display())))?;
    let members = case.entries.max(1);
    let member_bytes = artifact
        .values
        .scalar("member_bytes")
        .ok_or_else(|| OpError::Io("the artifact declares no member_bytes".to_string()))?;
    let base_files = artifact
        .values
        .scalar("base_files")
        .ok_or_else(|| OpError::Io("the artifact declares no base_files".to_string()))?;
    let base_objects = artifact
        .values
        .scalar("base_objects")
        .ok_or_else(|| OpError::Io("the artifact declares no base_objects".to_string()))?;

    let mut accepted: u64 = 0;
    let mut distinct: Vec<ObjectId> = Vec::new();
    for member in &artifact.members {
        for id in &member.ids {
            accepted += 1;
            if !distinct.contains(id) {
                distinct.push(*id);
            }
        }
    }

    let sample = context.output.join("sample.sqlite");
    let de_warm = prepare_sample(&Artifact::store_path(&directory), &sample)?;
    gates.push(gates::residency_gate(Some(de_warm.resident_after)));
    gates.push(crate::gates::attribution_gate(crate::gates::Attribution::Exclusive));

    let store = match open_untimed(&sample) {
        Ok(store) => store,
        Err(error) => return Ok(unmeasured(&error, gates)),
    };
    instruments::heap_begin();
    let (result, report) = super::measure("c2.workspace", |scope: &TimingScope<'_, Active>| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        for member in &artifact.members {
            for id in &member.ids {
                let object = artifact
                    .objects
                    .cloned_object(*id)
                    .ok_or(layerfs_storage::StorageError::ObjectMissing(*id))?;
                operation.accept(object)?;
            }
        }
        operation.finish(scope.child("storage.finish"))
    });
    let heap = instruments::heap_end();
    let timing_bytes = crate::support::phases::timing_json_bytes();
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => return Ok(unmeasured(&OpError::Product(format!("{error:?}")), gates)),
    };

    context.trace.write_number(Kind::Counter, "workspace.base_files", base_files as i128, "files", "harness-declared base file count")?;
    context.trace.write_number(Kind::Counter, "workspace.base_objects", base_objects as i128, "objects", "objects the base Store already held")?;
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
            base_objects
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

    // O1, and only O1. The frozen oracle for this family is **O1 + O5** - *"root
    // matches"* and the reuse counts - so the byte-exact read-back of every addition
    // was answering a question the oracle does not pose. The mechanism claim is
    // already gated above by `g2.reuse-equation`.
    let roots: Vec<ObjectId> = artifact.members.iter().map(|member| member.root).collect();
    crate::workload::expected::publish(
        context.trace,
        "additions",
        &crate::workload::expected::identity_digest(&roots),
    )?;
    let store = match open_untimed(&sample) {
        Ok(store) => store,
        Err(error) => return Ok(unmeasured(&error, gates)),
    };
    let distinct = crate::workload::expected::distinct(&roots);
    let (present, _) = Timing::disabled("oracle.contains", |scope: &TimingScope<'_, Active>| {
        crate::workload::expected::present_all(&store, &distinct, scope)
    });
    match present {
        Ok(present) => gates.push(gates::require(
            GateClass::Correctness,
            "g1.o1-addition-presence",
            present.len() == distinct.len(),
            &format!("{} of {} distinct pinned addition roots present", present.len(), distinct.len()),
            "every addition root the recipe declares is present after the save",
        )),
        Err(error) => gates.push(Gate::incomplete(
            GateClass::Correctness,
            "g1.o1-addition-presence",
            &format!("presence query failed: {error:?}"),
            "every addition root the recipe declares is present after the save",
        )),
    }
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

/// The object set one delta row offers, the base it is derived from, and the
/// expectation each member's logical bytes must satisfy.
///
/// The artifact holds the **union** of every distinct object rather than one store
/// per member: 500 members derived from one 4 MiB base share almost every chunk,
/// so the union is the base plus the members' new material, while 500 separate
/// stores would be 500 copies of the same bytes on disk.
fn delta_fixture(
    op: DeltaOp,
    members: u32,
    bytes: u64,
    seed: u64,
) -> Result<(TreeStore, Artifact), OpError> {
    let base_bytes = fixture::noise(bytes, seed);
    let (base, _base_root) = objects_of(&base_bytes)?;
    let mut objects = TreeStore::new();
    objects.absorb(&base);
    let mut list = Vec::with_capacity(members as usize);
    // One scratch buffer for every member. `clone_from` reuses the allocation, so
    // the per-member cost is the patch and the chunking rather than a fresh
    // allocation of the whole base; 500 members of a 4 MiB base used to allocate
    // and free 2 GB of scratch.
    let mut member = base_bytes.clone();
    for index in 0..members {
        member.clone_from(&base_bytes);
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
                        let patch =
                            fixture::noise((end - at) as u64, seed ^ u64::from(index) ^ step);
                        member[at..end].copy_from_slice(&patch);
                    }
                }
            }
        }
        let (store, root) = objects_of(&member)?;
        let ids = store.insertion_order().to_vec();
        objects.absorb(&store);
        list.push(Member {
            ids,
            root,
            // The frozen oracle for this family is **O1 + O3** - *"root matches;
            // chunk counts match"* - and its read-back was removed in round 5
            // because the specification never asked for O2. No member expectation is
            // therefore read, and hashing every member to record one cost the
            // acquisition 2 GB of scalar SHA-256 for a value nothing consults.
            expectation: None,
        });
    }
    Ok((base, Artifact { objects, members: list, values: Values::new() }))
}

/// C2-3 `prepare`: acquire the fixture once and write the artifact.
///
/// Nothing here is measured. The row's own receipt is written by the performance
/// invocation; this one writes the acquisition record beside the artifact, and the
/// runner publishes its wall as `acquisition_wall_ns` outside the row's admission
/// decision, exactly as owner decision D2 requires.
fn delta_prepare(case: &Case, op: DeltaOp, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    let directory = context.artifact_directory()?;
    let seed = seed_of(case.id);
    let members = case.entries.max(1);
    let bytes = if case.bytes > 0 { case.bytes } else { 4 << 20 };
    let (base, artifact) = delta_fixture(op, members, bytes, seed)?;
    create_and_save_untimed(&Artifact::store_path(&directory), &base)?;
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
        "prepare.members",
        artifact.members.len() as i128,
        "members",
        "member set the artifact declares",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "prepare.base_bytes",
        bytes as i128,
        "bytes",
        "declared base bytes",
    )?;
    Ok(OpOutcome {
        gates: Vec::new(),
        notes: vec![
            format!("prepare_op: {op:?}"),
            format!("prepare_members: {members}"),
            format!("prepare_base_bytes: {bytes}"),
            format!("artifact: {}", directory.display()),
            "acquisition: not a measured phase; charged to acquisition_wall_ns".to_string(),
        ],
    })
}

/// C2-3 `perf`: the measured save against the prepared artifact.
///
/// The base is a copy of the artifact's Store, de-warmed; the offered objects come
/// from the artifact's in-memory union, which is a declared
/// `warm-in-process-fixture`. The read-back oracle is **not** here: it is a
/// second, unmeasured operation and it runs in the `verify` phase, where its wall
/// is charged to the 60 s verification budget rather than to this row's 15 s
/// complete-command budget.
fn delta_perf(case: &Case, op: DeltaOp, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    let directory = context.artifact_directory()?;
    context.create_output()?;
    let mut gates = Vec::new();
    let members = case.entries.max(1);
    let bytes = if case.bytes > 0 { case.bytes } else { 4 << 20 };
    let artifact = Artifact::read(&directory)
        .map_err(|error| OpError::Io(format!("{}: {error}", directory.display())))?;

    let sample = context.output.join("sample.sqlite");
    let de_warm = prepare_sample(&Artifact::store_path(&directory), &sample)?;
    gates.push(gates::residency_gate(Some(de_warm.resident_after)));
    gates.push(crate::gates::attribution_gate(crate::gates::Attribution::Exclusive));

    let store = match open_untimed(&sample) {
        Ok(store) => store,
        Err(error) => return Ok(unmeasured(&error, gates)),
    };
    instruments::heap_begin();
    let (result, report) = super::measure("c2.delta", |scope: &TimingScope<'_, Active>| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        for member in &artifact.members {
            for id in &member.ids {
                let object = artifact
                    .objects
                    .cloned_object(*id)
                    .ok_or(layerfs_storage::StorageError::ObjectMissing(*id))?;
                operation.accept(object)?;
            }
        }
        operation.finish(scope.child("storage.finish"))
    });
    let heap = instruments::heap_end();
    let timing_bytes = crate::support::phases::timing_json_bytes();
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
    gates.extend(sidecar_gates(&sample));
    gates.push(gates::swap_gate(instruments::swaps()));
    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("delta_op: {op:?}"),
            format!("members: {members}"),
            format!("base_bytes: {bytes}"),
            format!("artifact_members: {}", artifact.members.len()),
            "store_state: opened-from-copy".to_string(),
            "measured_region: begin_save + accept + finish; no fixture construction".to_string(),
            "oracle_phase: verify-invocation".to_string(),
            format!("copy_rung: {COPY_RUNG}"),
            format!("allocation_attribution: {ATTRIBUTION}"),
            format!("heap_charged_bytes: {}", heap.charged_bytes),
        ],
    })
    .map(|outcome| {
        let _ = case;
        outcome
    })
}

/// C2-3 `verify`: the read-back oracle, unmeasured, in its own invocation.
///
/// Every member root is read back through the Store the performance invocation
/// wrote and compared with the expectation the artifact carries, which was derived
/// from the fixture recipe before the product saw it. This is the second,
/// unmeasured, byte-identical operation the measurement contract requires, and it
/// is why the performance receipt can be honest about not containing one.
fn delta_verify(case: &Case, op: DeltaOp, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    let directory = context.artifact_directory()?;
    let artifact = Artifact::read(&directory)
        .map_err(|error| OpError::Io(format!("{}: {error}", directory.display())))?;
    let sample = context.output.join("sample.sqlite");
    if !sample.exists() {
        return Ok(unmeasured(
            &OpError::Io(format!(
                "{}: the performance invocation did not write a sample",
                sample.display()
            )),
            Vec::new(),
        ));
    }
    let store = match open_untimed(&sample) {
        Ok(store) => store,
        Err(error) => return Ok(unmeasured(&error, Vec::new())),
    };
    // O1, and only O1. `gates_and_oracles.md` section 5 freezes this family's oracle
    // as **O1 + O3**: *"root matches; chunk counts match"*. It does not ask for a
    // byte-exact read-back of 500 members, which is what this phase used to do: 12
    // seconds of decoding a 2.10 GB logical set to answer a question the oracle never
    // posed. Removing it is compliance, not relaxation: no contract changed, no
    // sample was taken, and the pinned constants below are stronger than the
    // self-consistency they replace.
    let roots: Vec<ObjectId> = artifact.members.iter().map(|member| member.root).collect();
    crate::workload::expected::publish(
        context.trace,
        "members",
        &crate::workload::expected::identity_digest(&roots),
    )?;
    let all = crate::workload::expected::distinct(&roots);
    // The mode ladder's `sample` rung. The row declares its unit (the distinct
    // member identities) and takes the declared 10% of it, selected by
    // `index % 10 == 0`, so the sample spreads across the whole set rather than
    // being a prefix. A sampled row is `INCOMPLETE` in the receipt; the mode is
    // published there and in the report header.
    let sampled = match context.verify_sample {
        None => None,
        Some(requested) => Some(crate::ops::sampled_indices(all.len(), requested)),
    };
    let distinct: Vec<ObjectId> = match &sampled {
        None => all.clone(),
        Some(indices) => indices.iter().map(|index| all[*index]).collect(),
    };
    context.trace.write_number(
        Kind::Counter,
        "verify.units",
        all.len() as i128,
        "identities",
        "distinct member identities the row declares as its verification unit",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "verify.sampled",
        distinct.len() as i128,
        "identities",
        "identities this invocation verified, by the declared index % 10 == 0 rule",
    )?;
    let (present, _) = Timing::disabled("oracle.contains", |scope: &TimingScope<'_, Active>| {
        crate::workload::expected::present_all(&store, &distinct, scope)
    });
    let mut gates = Vec::new();
    match present {
        Ok(present) => gates.push(gates::require(
            GateClass::Correctness,
            "g1.o1-member-presence",
            present.len() == distinct.len(),
            &format!(
                "{} of {} distinct pinned member roots present{}",
                present.len(),
                distinct.len(),
                if sampled.is_some() {
                    format!(" (a declared sample of {})", all.len())
                } else {
                    String::new()
                }
            ),
            "every member root the recipe declares is present after the save",
        )),
        Err(error) => gates.push(Gate::incomplete(
            GateClass::Correctness,
            "g1.o1-member-presence",
            &format!("presence query failed: {error:?}"),
            "every member root the recipe declares is present after the save",
        )),
    }
    context.trace.write_number(
        Kind::Counter,
        "verify.members",
        artifact.members.len() as i128,
        "members",
        "members the artifact declares and this phase confirmed present",
    )?;
    gates.push(gates::require(
        GateClass::Custody,
        "g6.artifact-members",
        artifact.members.len() == case.entries.max(1) as usize,
        &format!("{} members verified", artifact.members.len()),
        &format!("{} members declared", case.entries.max(1)),
    ));
    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("verify_op: {op:?}"),
            "verification_phase: separate unmeasured invocation".to_string(),
            "verification_budget: charged to the 60 s verification budget".to_string(),
        ],
    })
}

/// C2-3: CDC delta locality over a stored base.
///
/// The row runs in three phases (`Phase::Prepare`, `Phase::Perf`,
/// `Phase::Verify`). A caller that hands it no artifact directory gets the
/// performance phase alone, which is what a selected single-case run without a
/// prepared artifact would ask for - and it fails closed with the reason rather
/// than silently building the fixture inside the timer.
pub fn delta(case: &Case, op: DeltaOp, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    match context.phase {
        crate::ops::Phase::Prepare => delta_prepare(case, op, context),
        crate::ops::Phase::Perf => delta_perf(case, op, context),
        crate::ops::Phase::Verify => delta_verify(case, op, context),
    }
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
    let (result, report) = super::measure("c2.boundary", |scope: &TimingScope<'_, Active>| {
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
    let timing_bytes = crate::support::phases::timing_json_bytes();
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
    crate::workload::expected::publish(context.trace, "file_root", &root.to_string())?;
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
    let (result, report) = super::measure("c2.small-file", |scope: &TimingScope<'_, Active>| {
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
    let timing_bytes = crate::support::phases::timing_json_bytes();
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
    crate::workload::expected::publish(context.trace, "file_root", &root.to_string())?;
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
/// The O(1) claim gates on `opens`: one on the opening wave and zero afterwards,
/// **but only through `StoreProvider::read_wave`**. The direct `Store::read_batch`
/// route reports `opens: 1` unconditionally, so this cell is ungateable on that
/// route and the case is driven through the provider. `pages` must equal
/// `ceil(ids / 128)`.
///
/// **Phase split.** The row measures two read waves over a prepared Store. Building
/// that Store means generating up to 500 MiB, chunking it and saving it — all
/// setup — and hashing the same 500 MiB to state the expectation the oracle
/// compares against. The registry declares `Preparation::BaseStore`, so the Store,
/// the root identity and the expectation are acquired once. The object set is
/// **not** persisted: the measured phase reads through the Store, and a harness
/// provider it never consults has no business in its artifact.
pub fn read_wave(case: &Case, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    match context.phase {
        Phase::Prepare => read_wave_prepare(case, context),
        Phase::Perf => read_wave_perf(case, context),
        Phase::Verify => Err(OpError::Io(
            "c2.read.waves declares an in-process oracle and has no deferred verification phase"
                .to_string(),
        )),
    }
}

/// C2-7 `prepare`: acquire the base Store and the identity its oracle reads.
fn read_wave_prepare(case: &Case, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    let directory = context.artifact_directory()?;
    let bytes = fixture::noise(case.bytes, seed_of(case.id));
    let (objects, root) = objects_of(&bytes)?;
    let expectation = Expectation::of(&bytes);
    let count = objects.len();
    create_and_save_untimed(&Artifact::store_path(&directory), &objects)?;
    let mut artifact = Artifact::new(TreeStore::new());
    artifact.values.set_identity("root", root);
    artifact.values.set_expectation("content", expectation);
    artifact.values.set_scalar("objects", count as u64);
    artifact.values.set_scalar("declared_bytes", case.bytes);
    artifact
        .write(&directory)
        .map_err(|error| OpError::Io(format!("{}: {error}", directory.display())))?;
    artifact
        .seal(&directory, case.id)
        .map_err(|error| OpError::Io(format!("{}: {error}", directory.display())))?;
    context.trace.write_number(Kind::Counter, "prepare.objects", count as i128, "objects", "distinct canonical objects the base Store holds")?;
    context.trace.write_number(Kind::Counter, "prepare.base_bytes", case.bytes as i128, "bytes", "declared base bytes")?;
    Ok(OpOutcome {
        gates: Vec::new(),
        notes: vec![
            format!("prepare_objects: {count}"),
            format!("prepare_base_bytes: {}", case.bytes),
            format!("artifact: {}", directory.display()),
            "acquisition: not a measured phase; charged to acquisition_wall_ns".to_string(),
        ],
    })
}

/// C2-7 `perf`: the measured waves against a per-sample copy of the prepared Store.
fn read_wave_perf(case: &Case, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    context.create_output()?;
    let mut gates = Vec::new();
    let directory = context.artifact_directory()?;
    let artifact = Artifact::read(&directory)
        .map_err(|error| OpError::Io(format!("{}: {error}", directory.display())))?;
    let root = artifact
        .values
        .identity("root")
        .ok_or_else(|| OpError::Io("the artifact declares no root identity".to_string()))?;
    let expectation = artifact
        .values
        .expectation("content")
        .ok_or_else(|| OpError::Io("the artifact declares no content expectation".to_string()))?;
    let objects = artifact.values.scalar("objects").unwrap_or(0);

    let sample = context.output.join("sample.sqlite");
    let de_warm = prepare_sample(&Artifact::store_path(&directory), &sample)?;
    gates.push(gates::residency_gate(Some(de_warm.resident_after)));
    gates.push(crate::gates::attribution_gate(crate::gates::Attribution::Exclusive));

    let store = match open_untimed(&sample) {
        Ok(store) => store,
        Err(error) => return Ok(unmeasured(&error, gates)),
    };

    instruments::heap_begin();
    let (result, report) = super::measure("c2.read.waves", |scope: &TimingScope<'_, Active>| {
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
    let timing_bytes = crate::support::phases::timing_json_bytes();
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
    crate::workload::expected::publish(context.trace, "file_root", &root.to_string())?;
    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("declared_bytes: {}", case.bytes),
            format!("objects: {objects}"),
            format!("store_state: opened-from-copy"),
            format!("copy_rung: {COPY_RUNG}"),
            format!("heap_charged_bytes: {}", heap.charged_bytes),
        ],
    })
}

/// The pooled metadata lane's fixture: distinct inode leaves, each at the row
/// ceiling.
///
/// `c2-families.md` section 1 excludes C1 file construction from every C2 family
/// ("C2 families accept canonical objects from the harness directly"), so the
/// leaves are built with the product's own leaf encoder rather than by running a
/// filesystem build. Each leaf holds `POOLED_LEAF_ROWS_LIMIT` rows in strictly
/// increasing serials with a **distinct** value per row, which is what makes the
/// ordinal equation falsifiable: a cold save must assign one new ordinal per row
/// and a warm save must reuse every one.
fn pooled_leaves(
    leaves: u32,
    rows: u32,
    seed: u64,
    serial_base: u64,
) -> Result<TreeStore, OpError> {
    use layerfs_content::inode_leaf::{InodeKind, InodeLeaf, InodeLeafRow, InodeValue};
    use layerfs_content::{FinalizedObject, ObjectRole};

    let mut store = TreeStore::new();
    for leaf in 0..leaves {
        let mut page: Vec<InodeLeafRow> = Vec::with_capacity(rows as usize);
        for row in 0..rows {
            let label = format!("layerfs/pool/{seed}/{leaf}/{row}");
            let value = InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 1,
                content_root: ObjectId::for_bytes(label.as_bytes()),
                metadata_root: ObjectId::for_bytes(format!("{label}/metadata").as_bytes()),
            };
            page.push(InodeLeafRow {
                serial: serial_base
                    + u64::from(leaf) * u64::from(rows)
                    + u64::from(row),
                value: layerfs_content::inode_leaf::encode_inode_value(value),
            });
        }
        let canonical = InodeLeaf {
            subtree_bytes: u64::from(rows) * 81,
            rows: page,
        }
        .encode()
        .map_err(|error| OpError::Product(format!("{error:?}")))?;
        let object = FinalizedObject::new(ObjectRole::InodeLeaf, canonical)
            .map_err(|error| OpError::Product(format!("{error:?}")))?;
        store.insert_object(object);
    }
    Ok(store)
}

/// Serial base of the measured object set. Distinct from the base Store's, so a
/// measured leaf is a new canonical object whose values the catalogue may still
/// know.
const MEASURED_SERIAL_BASE: u64 = 10_000_001;
/// Serial base of the warm row's base save.
const BASE_SERIAL_BASE: u64 = 1;

/// C2-8: the pooled metadata lane, cold against warm.
///
/// The two rows are one equation read in both directions. A **cold** row saves
/// the declared leaves into a Store that holds no pooled values, so every one of
/// the `leaves x rows` values must receive a new ordinal. A **warm** row saves the
/// same leaves into a Store that already holds them, so every value must reuse an
/// existing ordinal. The Store-owned state that decides which happens is the
/// pooled index, published here as `Store::pool_index_entries` and
/// `Store::pool_index_bytes`; the catalogue it is synchronized from is
/// `metadata_value_groups`, counted through `SaveOutcome.pool.groups`. Cold and
/// warm are reported separately and are never pooled into one number.
pub fn pool(case: &Case, cold: bool, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    context.create_output()?;
    let mut gates = Vec::new();
    let seed = seed_of(case.id);
    let leaves = case.entries.max(1);
    let rows = crate::families::c2_pool::ROWS;
    let declared_values = u64::from(leaves) * u64::from(rows);
    // The two rows offer the **same** measured object set; only the Store's
    // pooled-index state differs, which is what makes the pair a controlled
    // comparison rather than two different workloads. The warm row's base holds
    // the same values under different serials, so every measured leaf is a new
    // canonical object that the object-level exact-hit lookup cannot answer and
    // the pooled lane has to admit: a base that reused the same serials would be
    // 512 exact hits and would never reach the lane this family measures.
    let objects = pooled_leaves(leaves, rows, seed, MEASURED_SERIAL_BASE)?;
    let warm_base = pooled_leaves(leaves, rows, seed, BASE_SERIAL_BASE)?;

    let base = context.output.join("base.sqlite");
    let sample = context.output.join("sample.sqlite");
    let empty = TreeStore::new();
    let prepared: &TreeStore = if cold { &empty } else { &warm_base };
    create_and_save_untimed(&base, prepared)?;
    let de_warm = prepare_sample(&base, &sample)?;
    gates.push(gates::residency_gate(Some(de_warm.resident_after)));
    gates.push(crate::gates::attribution_gate(crate::gates::Attribution::Exclusive));

    let store = match open_untimed(&sample) {
        Ok(store) => store,
        Err(error) => return Ok(unmeasured(&error, gates)),
    };
    let index_before = store.pool_index_entries();
    instruments::heap_begin();
    let (result, report) = super::measure("c2.pool", |scope: &TimingScope<'_, Active>| {
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
    let timing_bytes = crate::support::phases::timing_json_bytes();
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => return Ok(unmeasured(&OpError::Product(format!("{error:?}")), gates)),
    };
    // Read after the timer: the index is Store-owned state, and reading it is a
    // published observation rather than part of the operation.
    let index_entries = store.pool_index_entries();
    let index_bytes = store.pool_index_bytes();

    let pool = outcome.pool;
    context.trace.write_number(Kind::Counter, "pool.leaves", pool.leaves as i128, "leaves", "SaveOutcome.pool.leaves")?;
    context.trace.write_number(Kind::Counter, "pool.reused_values", pool.reused_values as i128, "values", "SaveOutcome.pool.reused_values")?;
    context.trace.write_number(Kind::Counter, "pool.new_values", pool.new_values as i128, "values", "SaveOutcome.pool.new_values")?;
    context.trace.write_number(Kind::Counter, "pool.groups", pool.groups as i128, "groups", "SaveOutcome.pool.groups, the metadata_value_groups catalogue")?;
    context.trace.write_number(Kind::Counter, "pool.delta_leaves", pool.delta_leaves as i128, "leaves", "SaveOutcome.pool.delta_leaves")?;
    context.trace.write_number(Kind::Counter, "pool.full_leaves", pool.full_leaves as i128, "leaves", "SaveOutcome.pool.full_leaves")?;
    context.trace.write_number(Kind::Counter, "pool.trials", pool.trials as i128, "trials", "SaveOutcome.pool.trials")?;
    context.trace.write_number(Kind::Counter, "pool.work_exceeded", pool.work_exceeded as i128, "leaves", "SaveOutcome.pool.work_exceeded")?;
    context.trace.write_number(Kind::Counter, "pool.declared_values", declared_values as i128, "values", "case configuration: leaves x rows")?;
    context.trace.write_number(Kind::Counter, "pool.inserted", i128::from(outcome.inserted), "objects", "SaveOutcome.inserted")?;
    context.trace.write_number(Kind::Counter, "pool.reused", i128::from(outcome.reused), "objects", "SaveOutcome.reused")?;
    context.trace.write_number(Kind::Counter, "pool.commits", i128::from(outcome.commits), "transactions", "SaveOutcome.commits")?;
    context.trace.write_number(Kind::Counter, "pool.index_entries_before", index_before as i128, "entries", "Store::pool_index_entries before the measured save")?;
    context.trace.write_number(Kind::Counter, "pool.index_entries", index_entries as i128, "entries", "Store::pool_index_entries after the measured save")?;
    context.trace.write_number(Kind::Counter, "pool.index_bytes", index_bytes as i128, "bytes", "Store::pool_index_bytes after the measured save")?;
    context.trace.write_number(Kind::Counter, "timing_json_bytes", timing_bytes as i128, "bytes", "product timing.json, byte-verbatim")?;
    context.trace.write_number(Kind::Resource, "heap.peak_incremental_bytes", heap.peak_incremental_bytes as i128, "bytes", "counting GlobalAlloc, measured phase")?;

    gates.push(completeness_gate(&report));
    gates.push(gates::require(
        GateClass::Mechanism,
        "g2.pool-leaves",
        pool.leaves == u64::from(leaves),
        &format!("{} leaves admitted", pool.leaves),
        &format!("{leaves} inode leaves offered"),
    ));
    gates.push(gates::require(
        GateClass::Mechanism,
        "g2.pool-ordinal-equation",
        if cold {
            pool.new_values == declared_values && pool.reused_values == 0
        } else {
            pool.reused_values == declared_values && pool.new_values == 0
        },
        &format!(
            "{} new, {} reused of {declared_values}",
            pool.new_values, pool.reused_values
        ),
        if cold {
            "cold index: every declared value receives a new ordinal and none is reused"
        } else {
            "warm index: every declared value reuses an existing ordinal and none is new"
        },
    ));
    // A reopened Store starts with an empty in-memory index either way: it is
    // re-synchronized from the catalogue on the first pooled save after a reopen.
    // What separates the rows is therefore not the index's *starting* count but
    // whether the catalogue could answer the values, which is `reused_values`.
    gates.push(gates::require(
        GateClass::Mechanism,
        "g2.pool-index",
        index_before == 0 && index_entries as u64 == declared_values && index_bytes > 0,
        &format!("index {index_before} -> {index_entries} entries, {index_bytes} bytes"),
        "the reopened index starts empty and the save leaves every declared value retained",
    ));
    gates.push(gates::require(
        GateClass::Mechanism,
        "g2.pool-groups",
        if cold {
            pool.groups > 0
        } else {
            pool.groups == 0
        },
        &format!("{} value groups written", pool.groups),
        if cold {
            "a cold save writes the value groups its new ordinals need"
        } else {
            "a warm save finds every value in the catalogue and writes no group"
        },
    ));

    // Oracle: every leaf read back through the Store's own authenticated read
    // path and compared byte-for-byte with the canonical object that was offered.
    // A pooled leaf is stored as a transformed body against a base, so this is
    // the gate that says the reconstruction is the identity and not merely
    // something the Store is willing to return.
    let store = match open_untimed(&sample) {
        Ok(store) => store,
        Err(error) => return Ok(unmeasured(&error, gates)),
    };
    // O1, and only O1. The frozen oracle for this family is **O1 + O3** - *"leaf
    // identity matches; pooled row count matches"* - and the identity check it asks
    // for is a presence question, not a byte-exact decode of 51,200 values.
    let ids: Vec<ObjectId> = objects.insertion_order().to_vec();
    crate::workload::expected::publish(
        context.trace,
        "leaves",
        &crate::workload::expected::identity_digest(&ids),
    )?;
    let distinct = crate::workload::expected::distinct(&ids);
    let (present, _) = Timing::disabled("oracle.contains", |scope: &TimingScope<'_, Active>| {
        crate::workload::expected::present_all(&store, &distinct, scope)
    });
    match present {
        Ok(present) => gates.push(gates::require(
            GateClass::Correctness,
            "g1.o1-leaf-presence",
            present.len() == distinct.len(),
            &format!("{} of {} distinct pinned leaf identities present", present.len(), distinct.len()),
            "every pooled leaf identity the row offered is present after the save",
        )),
        Err(error) => gates.push(Gate::incomplete(
            GateClass::Correctness,
            "g1.o1-leaf-presence",
            &format!("presence query failed: {error:?}"),
            "every pooled leaf identity the row offered is present after the save",
        )),
    }
    gates.extend(sidecar_gates(&sample));
    gates.push(gates::swap_gate(instruments::swaps()));

    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("pool_index_state: {}", if cold { "cold" } else { "warm" }),
            format!("declared_leaves: {leaves}"),
            format!("declared_rows_per_leaf: {rows}"),
            format!("declared_values: {declared_values}"),
            format!("pool_index_entries_before: {index_before}"),
            "cold and warm rows are reported separately and are never pooled".to_string(),
            format!("store_state: opened-from-copy"),
            format!("copy_rung: {COPY_RUNG}"),
            format!("allocation_attribution: {ATTRIBUTION}"),
            format!("heap_charged_bytes: {}", heap.charged_bytes),
        ],
    })
    .map(|outcome| {
        let _ = case;
        outcome
    })
}

/// C2-5: footprint accounting. The bytes are measured by `shared/space.py` in
/// verification mode, not here: the harness owns no SQL, and the number must be
/// re-derived by a reader that did not produce it.
///
/// **Phase split.** The row measures one save of a supplied object set into an
/// empty Store; the supplied set is a fixture. The `metadata-cardinality-100000`
/// control builds 100,000 small objects, so the registry declares
/// `Preparation::ObjectSet` and the set is acquired once. The Store is **not**
/// prepared: it is empty, and an empty `Store::create` is part of the acquisition
/// the row already pays for per sample.
pub fn footprint(
    case: &Case,
    op: FootprintOp,
    context: &mut OpContext<'_>,
) -> Result<OpOutcome, OpError> {
    match context.phase {
        Phase::Prepare => footprint_prepare(case, op, context),
        Phase::Perf => footprint_perf(case, op, context),
        Phase::Verify => Err(OpError::Io(
            "c2.footprint declares an in-process oracle and has no deferred verification phase"
                .to_string(),
        )),
    }
}

/// The supplied object set one footprint control declares.
fn footprint_supplied(case: &Case, op: FootprintOp, seed: u64) -> Result<TreeStore, OpError> {
    let object_bytes = case.bytes;
    let entries = case.entries.max(1);
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
    Ok(supplied)
}

/// C2-5 `prepare`: acquire the supplied object set once.
fn footprint_prepare(case: &Case, op: FootprintOp, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    let directory = context.artifact_directory()?;
    let supplied = footprint_supplied(case, op, seed_of(case.id))?;
    let count = supplied.len();
    let artifact = Artifact::new(supplied);
    let written = artifact
        .write(&directory)
        .map_err(|error| OpError::Io(format!("{}: {error}", directory.display())))?;
    artifact
        .seal(&directory, case.id)
        .map_err(|error| OpError::Io(format!("{}: {error}", directory.display())))?;
    context.trace.write_number(Kind::Counter, "prepare.objects", written as i128, "objects", "distinct canonical objects the artifact holds")?;
    Ok(OpOutcome {
        gates: Vec::new(),
        notes: vec![
            format!("prepare_op: {op:?}"),
            format!("prepare_objects: {count}"),
            format!("artifact: {}", directory.display()),
            "acquisition: not a measured phase; charged to acquisition_wall_ns".to_string(),
        ],
    })
}

/// C2-5 `perf`: the measured save of the prepared object set.
fn footprint_perf(case: &Case, op: FootprintOp, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    context.create_output()?;
    let mut gates = Vec::new();
    let directory = context.artifact_directory()?;
    let artifact = Artifact::read(&directory)
        .map_err(|error| OpError::Io(format!("{}: {error}", directory.display())))?;
    let supplied = &artifact.objects;
    let object_bytes = case.bytes;
    let entries = case.entries.max(1);

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
    let (result, report) = super::measure("c2.footprint", |scope: &TimingScope<'_, Active>| {
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
    let timing_bytes = crate::support::phases::timing_json_bytes();
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
    // O1: the supplied identities, pinned, answered by the product's presence path.
    let supplied_ids: Vec<ObjectId> = supplied.insertion_order().to_vec();
    crate::workload::expected::publish(
        context.trace,
        "supplied",
        &crate::workload::expected::identity_digest(&supplied_ids),
    )?;
    let distinct = crate::workload::expected::distinct(&supplied_ids);
    let (present, _) = Timing::disabled("oracle.contains", |scope: &TimingScope<'_, Active>| {
        crate::workload::expected::present_all(&store, &distinct, scope)
    });
    match present {
        Ok(present) => gates.push(gates::require(
            GateClass::Correctness,
            "g1.o1-supplied-presence",
            present.len() == distinct.len(),
            &format!("{} of {} distinct pinned supplied identities present", present.len(), distinct.len()),
            "every supplied identity is present after the save",
        )),
        Err(error) => gates.push(Gate::incomplete(
            GateClass::Correctness,
            "g1.o1-supplied-presence",
            &format!("presence query failed: {error:?}"),
            "every supplied identity is present after the save",
        )),
    }
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
