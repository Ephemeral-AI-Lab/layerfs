//! The chain-work budgets decide whether a declared base is used.
//!
//! Reading a stored object as a dependency charges the chain budgets (4 and 2
//! cutoffs: 512 KiB canonical and 256 KiB encoded at the default cutoff) and
//! fails the read when that object's own chain exceeds them. Under its own
//! policy a Store never holds such a chain, since the largest whole file fits
//! alone (#472); a base whose chain exceeds them anyway is refused, not fatal: the
//! new object is stored independently and counted as `work_exceeded`, exactly as
//! for a chain that outgrows the budget. A base whose own chain fits is acquired
//! and authenticated before the dependent is judged against the budget, so a
//! corrupt one fails the Save even when the dependent then tips the chain over.
#[path = "support/memory_metadata.rs"]
mod metadata;
mod support;

use layerfs_content::{AdvisoryPredecessors, FinalizedObject, ObjectId, ObjectRole};
use layerfs_storage::port::ObjectKey;
use layerfs_storage::{Storage, StorageError, StoragePolicy, WriteOutcome};
use metadata::MemoryMetadata;
use std::sync::Arc;

const ONE_MIB: u64 = 1_048_576;
const DEFAULT_CUTOFF: u64 = 131_072;

/// A canonical whole-file object has 23 bytes of envelope around its payload.
const ENVELOPE: usize = 23;

/// One whole file of `len` payload bytes, incompressible or compressible and
/// changed at a byte that depends on `index`, declaring `base` as its predecessor.
fn version(
    len: usize,
    compressible: bool,
    index: usize,
    base: Option<ObjectId>,
) -> FinalizedObject {
    let mut raw = match compressible {
        true => support::patterned(len),
        false => support::noise(len),
    };
    raw[index * 7] ^= 0x5a;
    let object =
        FinalizedObject::new(ObjectRole::WholeFile, support::assembled_small_object(&raw)).unwrap();
    match base {
        Some(base) => object.with_predecessors(AdvisoryPredecessors::explicit(base).unwrap()),
        None => object,
    }
}

/// A chain of versions, each declaring the one before it, and the result of
/// every Save that stored one.
struct Chain {
    metadata: Arc<MemoryMetadata>,
    objects: Vec<FinalizedObject>,
    saves: Vec<Result<WriteOutcome, StorageError>>,
}

fn mebibyte_cutoff() -> StoragePolicy {
    StoragePolicy::new(1, ONE_MIB, 16, 8).validated().unwrap()
}

fn default_cutoff() -> StoragePolicy {
    StoragePolicy::new(1, DEFAULT_CUTOFF, 16, 8)
        .validated()
        .unwrap()
}

fn store(policy: StoragePolicy) -> Arc<MemoryMetadata> {
    let metadata = Arc::new(MemoryMetadata::default());
    metadata.state.lock().unwrap().policy = policy;
    metadata
}

/// Saves one file per `(payload length, compressible)`, each in its own Save.
/// The first has no base, so its Save must succeed.
fn chain(policy: StoragePolicy, versions: &[(usize, bool)]) -> Chain {
    let metadata = store(policy);
    let storage = Storage::new(metadata.clone()).unwrap();
    let mut objects: Vec<FinalizedObject> = Vec::new();
    let mut saves = Vec::new();
    for (index, (len, compressible)) in versions.iter().enumerate() {
        let base = objects.last().map(FinalizedObject::id);
        let object = version(*len, *compressible, index, base);
        objects.push(object.clone());
        let save = storage.begin_save().unwrap();
        save.accept(object).unwrap();
        saves.push(save.finish());
    }
    let first = saves.first().expect("a version to save");
    assert!(first.is_ok(), "the first version has no base: {first:?}");
    Chain {
        metadata,
        objects,
        saves,
    }
}

/// The same chain of versions, all in one Save.
fn chain_in_one_save(policy: StoragePolicy, versions: &[(usize, bool)]) -> WriteOutcome {
    let storage = Storage::new(store(policy)).unwrap();
    let save = storage.begin_save().unwrap();
    let mut base = None;
    for (index, (len, compressible)) in versions.iter().enumerate() {
        let object = version(*len, *compressible, index, base);
        base = Some(object.id());
        save.accept(object).unwrap();
    }
    save.finish().unwrap()
}

impl Chain {
    /// The outcome of Save `index`; a failed Save panics naming `what`.
    fn save(&self, index: usize, what: &str) -> &WriteOutcome {
        match &self.saves[index] {
            Ok(outcome) => outcome,
            Err(error) => panic!("{what}: save {index} failed: {error:?}"),
        }
    }

    /// Save `index` stored its object independently because the base chain plus
    /// the dependent would exceed a budget, and tried no delta.
    fn assert_refused_by_budget(&self, index: usize, what: &str) {
        let outcome = self.save(index, what);
        assert_eq!(outcome.delta.work_exceeded, 1, "{what}: {outcome:?}");
        assert_eq!(outcome.delta.trials, 0, "{what}: {outcome:?}");
        assert_eq!(outcome.delta.depth_capped, 0, "{what}: {outcome:?}");
        assert_eq!(
            (outcome.full_records, outcome.prefix_records),
            (1, 0),
            "{what}: {outcome:?}"
        );
    }

    /// Save `index` tried a delta against its base.
    fn assert_admitted(&self, index: usize, what: &str) {
        let outcome = self.save(index, what);
        assert_eq!(outcome.delta.work_exceeded, 0, "{what}: {outcome:?}");
        assert_eq!(outcome.delta.trials, 1, "{what}: {outcome:?}");
    }

    /// Every version reads back as the canonical bytes that were offered.
    fn assert_reads_back(&self) {
        let storage = Storage::new(self.metadata.clone()).unwrap();
        let ids: Vec<ObjectId> = self.objects.iter().map(FinalizedObject::id).collect();
        let read = storage.reader().unwrap().read_objects(&ids).unwrap();
        for (object, bytes) in self.objects.iter().zip(read) {
            assert_eq!(bytes.as_slice(), object.canonical());
        }
    }
}

#[test]
fn a_stored_chain_above_the_budgets_is_refused_not_fatal() {
    // A Store's policy never changes, and under its own budgets the producer
    // never stores a chain a read could not charge. To reach the refusal, the
    // base is stored under a 1 MiB cutoff and the dependent is saved after the
    // fixture's policy drops to the default cutoff, whose budgets the base alone
    // exceeds: 300,000 incompressible bytes above 256 KiB encoded, 600,000
    // compressible bytes above 512 KiB canonical.
    for (len, compressible) in [(300_000, false), (600_000, true)] {
        let metadata = store(mebibyte_cutoff());
        let base = version(len, compressible, 0, None);
        save_one(&metadata, base.clone()).unwrap();
        metadata.state.lock().unwrap().policy = default_cutoff();
        let dependent = version(10_000, compressible, 1, Some(base.id()));
        let outcome = save_one(&metadata, dependent.clone()).unwrap();
        assert_eq!(outcome.delta.work_exceeded, 1, "{len}: {outcome:?}");
        assert_eq!(outcome.delta.trials, 0, "{len}: {outcome:?}");
        assert_eq!(
            (outcome.full_records, outcome.prefix_records),
            (1, 0),
            "{len}: {outcome:?}"
        );
        let storage = Storage::new(metadata.clone()).unwrap();
        assert_eq!(
            storage
                .reader()
                .unwrap()
                .read_objects(&[dependent.id()])
                .unwrap(),
            vec![dependent.canonical().to_vec()]
        );
    }
}

#[test]
fn a_dependent_that_fills_the_canonical_budget_exactly_is_admitted() {
    // At the default cutoff: three 131,072-byte canonical objects (131,049
    // payload bytes) chained, and a fourth of the same size, fill the 524,288-byte
    // budget exactly. One dependent byte more is refused.
    let len = 131_072 - ENVELOPE;
    let fits = chain(default_cutoff(), &[(len, true); 4]);
    let depth = fits.save(2, "third version").delta.whole_file_depths;
    assert_eq!(depth.iter().position(|n| *n == 1), Some(2), "{depth:?}");
    fits.assert_admitted(3, "canonical budget filled exactly");
    fits.assert_reads_back();

    let over = chain(
        default_cutoff(),
        &[(len, true), (len, true), (len, true), (len + 1, true)],
    );
    over.assert_refused_by_budget(3, "one canonical byte past the budget");
}

#[test]
fn a_dependent_that_fills_the_encoded_budget_exactly_is_admitted() {
    // A verbatim record in the compact whole-file lane is its payload plus a
    // 1-byte tag. Base 131,056 + 1, dependent 131,064 + 23 canonical = 262,144,
    // the default cutoff's budget. One dependent byte more is refused.
    let fits = chain(default_cutoff(), &[(131_056, false), (131_064, false)]);
    fits.assert_admitted(1, "encoded budget filled exactly");
    fits.assert_reads_back();

    let over = chain(default_cutoff(), &[(131_056, false), (131_065, false)]);
    over.assert_refused_by_budget(1, "one encoded byte past the budget");
}

#[test]
fn a_chain_built_inside_one_save_is_judged_like_one_built_over_several() {
    // Four 130,000-byte versions, then a fifth, at the default cutoff. While the
    // Save still holds the fourth, its chain cost is the one the Save recorded
    // when it chose its record; once it is stored, the cost is read back from the
    // stored record. Both must put the fifth version's refusal at the same size.
    let refused = |len: usize, together: bool| {
        let versions = [
            (130_000, true),
            (130_000, true),
            (130_000, true),
            (130_000, true),
            (len, true),
        ];
        let work_exceeded = if together {
            chain_in_one_save(default_cutoff(), &versions)
                .delta
                .work_exceeded
        } else {
            chain(default_cutoff(), &versions)
                .save(4, "fifth version")
                .delta
                .work_exceeded
        };
        work_exceeded == 1
    };
    let (mut fits, mut over) = (1_000, 10_000);
    assert!(!refused(fits, false) && refused(over, false));
    while over - fits > 1 {
        let middle = (fits + over) / 2;
        match refused(middle, false) {
            true => over = middle,
            false => fits = middle,
        }
    }
    // 4 x 130,023 + (fits + 23) canonical bytes fill the 524,288-byte budget.
    assert_eq!(fits, 524_288 - 4 * 130_023 - ENVELOPE);
    assert!(!refused(fits, true), "{fits} bytes fit when stored");
    assert!(refused(over, true), "{over} bytes do not fit when stored");
}

#[test]
fn control_a_base_inside_both_budgets_still_chains() {
    let saved = chain(
        StoragePolicy::new(1, 131_072, 16, 8),
        &[(100 * 1024, true); 3],
    );
    let depth = |index: usize| {
        let outcome = saved.save(index, "inside both budgets");
        assert_eq!(outcome.delta.work_exceeded, 0, "{outcome:?}");
        outcome.delta.whole_file_depths.iter().position(|n| *n == 1)
    };
    assert_eq!([depth(0), depth(1), depth(2)], [0, 1, 2].map(Some));
    saved.assert_reads_back();
}

#[test]
fn control_with_whole_file_deltas_off_the_same_saves_succeed() {
    for versions in [[(600_000, true); 2], [(300_000, false); 2]] {
        let policy = StoragePolicy::new(1, ONE_MIB, 0, 8).validated().unwrap();
        let saved = chain(policy, &versions);
        let outcome = saved.save(1, "deltas off");
        assert_eq!(outcome.delta.work_exceeded, 0, "{outcome:?}");
        saved.assert_reads_back();
    }
}

/// One Save of `object` on a fresh handle of the Store, so nothing is cached.
fn save_one(
    metadata: &Arc<MemoryMetadata>,
    object: FinalizedObject,
) -> Result<WriteOutcome, StorageError> {
    let storage = Storage::new(metadata.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    save.accept(object).unwrap();
    save.finish()
}

/// Flips one payload byte inside the stored record of `id` and re-seals the pack's
/// digest, so the damage is only visible to object authentication. The record
/// stores its payload verbatim: `window` is 64 bytes of it.
fn corrupt_payload(metadata: &MemoryMetadata, id: ObjectId, window: &[u8]) {
    let mut state = metadata.state.lock().unwrap();
    let pack_id = state.objects[&id].pack_id;
    let pack = state.packs.get_mut(&pack_id).unwrap();
    let mut body = pack.body.as_ref().clone();
    let at = body
        .windows(window.len())
        .position(|candidate| candidate == window)
        .expect("the record stores its payload verbatim");
    body[at] ^= 0x01;
    pack.info.key = ObjectKey::for_bytes(&body);
    pack.body = Arc::new(body);
}

#[test]
fn control_a_corrupt_base_inside_both_budgets_still_fails_the_save() {
    let metadata = store(StoragePolicy::new(1, 131_072, 16, 8));
    let raw = support::noise(100 * 1024);
    let first =
        FinalizedObject::new(ObjectRole::WholeFile, support::assembled_small_object(&raw)).unwrap();
    save_one(&metadata, first.clone()).unwrap();
    corrupt_payload(&metadata, first.id(), &raw[50_000..50_064]);
    let mut changed = raw;
    changed[0] ^= 0x5a;
    let second = FinalizedObject::new(
        ObjectRole::WholeFile,
        support::assembled_small_object(&changed),
    )
    .unwrap()
    .with_predecessors(AdvisoryPredecessors::explicit(first.id()).unwrap());
    let error = save_one(&metadata, second).unwrap_err();
    assert!(
        matches!(error, StorageError::Integrity("dependency identity")),
        "{error:?}"
    );
}

#[test]
fn a_corrupt_base_that_fits_is_authenticated_even_when_the_dependent_tips_the_chain_over() {
    // At the default cutoff the base fits both budgets alone: 131,057 encoded and 131,079 canonical
    // bytes. The dependent is 131,088 canonical bytes, so its prospective encoded
    // charge is 131,057 + 131,088 = 262,145, one byte over the budget: with an
    // intact base the Save is the budget refusal. The base is acquired and
    // authenticated before that is decided, so a corrupt one fails the Save.
    let intact = chain(default_cutoff(), &[(131_056, false), (131_065, false)]);
    intact.assert_refused_by_budget(1, "intact base, one encoded byte past the budget");

    let metadata = store(default_cutoff());
    let first = version(131_056, false, 0, None);
    save_one(&metadata, first.clone()).unwrap();
    let raw = support::noise(131_056);
    corrupt_payload(&metadata, first.id(), &raw[50_000..50_064]);
    let second = version(131_065, false, 1, Some(first.id()));
    let error = save_one(&metadata, second).unwrap_err();
    assert!(
        matches!(error, StorageError::Integrity("dependency identity")),
        "{error:?}"
    );
}
