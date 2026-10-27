//! The payload chain-work budgets scale with the Store's construction cutoff
//! (#472): four cutoffs of canonical bytes and two of encoded bytes, which are
//! 512 KiB and 256 KiB at the default 128 KiB cutoff. The largest whole file a
//! cutoff admits therefore always fits a chain alone, so a Save never meets a
//! declared base whose own chain a read could not charge; a larger cutoff keeps
//! whole-file deltas for the larger whole files it creates.
#[path = "support/memory_metadata.rs"]
mod metadata;
mod support;

use layerfs_content::{AdvisoryPredecessors, FinalizedObject, ObjectId, ObjectRole};
use layerfs_storage::policy::SINGLETON_FRAMING_SLACK;
use layerfs_storage::{Storage, StorageCapacities, StoragePolicy, WriteOutcome};
use std::sync::Arc;

const KIB: u64 = 1024;
const CUTOFFS: [u64; 4] = [128 * KIB, 256 * KIB, 512 * KIB, 1024 * KIB];
/// Bytes the whole-file canonical envelope adds to a payload.
const ENVELOPE: usize = 23;

fn policy(cutoff: u64) -> StoragePolicy {
    StoragePolicy::new(1, cutoff, 16, 8)
}

fn capacities(cutoff: u64) -> StorageCapacities {
    StorageCapacities::from_policy(policy(cutoff)).unwrap()
}

/// Version `version` of a `len`-byte payload: one byte of it differs from
/// every other version's.
fn payload(len: usize, compressible: bool, version: usize) -> Vec<u8> {
    let mut raw = if compressible {
        support::patterned(len)
    } else {
        support::noise(len)
    };
    raw[version * 7] ^= 0x5a;
    raw
}

struct Saved {
    metadata: Arc<metadata::MemoryMetadata>,
    objects: Vec<FinalizedObject>,
    outcomes: Vec<WriteOutcome>,
}

impl Saved {
    /// Each version's stored chain depth.
    fn depths(&self) -> Vec<usize> {
        self.outcomes
            .iter()
            .map(|o| {
                assert_eq!(o.delta.whole_file_depths.iter().sum::<u64>(), 1, "{o:?}");
                o.delta
                    .whole_file_depths
                    .iter()
                    .position(|n| *n == 1)
                    .unwrap()
            })
            .collect()
    }

    /// Every version reads back exactly, through a fresh reader.
    fn assert_reads_back(&self) {
        let reader = Storage::new(self.metadata.clone()).unwrap();
        let reader = reader.reader().unwrap();
        for object in &self.objects {
            assert_eq!(
                reader.read_objects(&[object.id()]).unwrap(),
                vec![object.canonical().to_vec()]
            );
        }
    }
}

/// Saves one payload per entry of `versions` (`(len, compressible)`), each in its
/// own Save and each declaring the previous one as its base.
fn chain(cutoff: u64, versions: &[(usize, bool)]) -> Saved {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    metadata.state.lock().unwrap().policy = policy(cutoff);
    let storage = Storage::new(metadata.clone()).unwrap();
    let mut previous: Option<ObjectId> = None;
    let mut objects = Vec::new();
    let mut outcomes = Vec::new();
    for (version, &(len, compressible)) in versions.iter().enumerate() {
        let raw = payload(len, compressible, version);
        let mut object =
            FinalizedObject::new(ObjectRole::WholeFile, support::assembled_small_object(&raw))
                .unwrap();
        if let Some(base) = previous {
            object = object.with_predecessors(AdvisoryPredecessors::explicit(base).unwrap());
        }
        previous = Some(object.id());
        let save = storage.begin_save().unwrap();
        save.accept(object.clone()).unwrap();
        outcomes.push(save.finish().unwrap());
        objects.push(object);
    }
    Saved {
        metadata,
        objects,
        outcomes,
    }
}

#[test]
fn default_store_budgets_are_unchanged() {
    let default = StorageCapacities::from_policy(StoragePolicy::frozen_default()).unwrap();
    assert_eq!(default.small_file_threshold_bytes, 131_072);
    assert_eq!(default.chain_canonical_limit, 524_288);
    assert_eq!(default.chain_encoded_limit, 262_144);
}

#[test]
fn budgets_scale_with_every_cutoff() {
    for cutoff in CUTOFFS {
        let capacities = capacities(cutoff);
        assert_eq!(capacities.chain_canonical_limit, 4 * cutoff, "{cutoff}");
        assert_eq!(capacities.chain_encoded_limit, 2 * cutoff, "{cutoff}");
        // A depth never raises them.
        let shallow = StorageCapacities::from_policy(StoragePolicy::new(1, cutoff, 1, 1)).unwrap();
        assert_eq!(
            (shallow.chain_canonical_limit, shallow.chain_encoded_limit),
            (
                capacities.chain_canonical_limit,
                capacities.chain_encoded_limit
            )
        );
    }
}

#[test]
fn largest_whole_file_fits_a_chain_alone_at_every_cutoff() {
    // The property #472 needs: a stored whole file is never a base that a read
    // could not charge by itself, whatever the cutoff.
    for cutoff in CUTOFFS {
        let capacities = capacities(cutoff);
        assert!(
            capacities.whole_file_canonical_limit as u64 <= capacities.chain_canonical_limit,
            "{cutoff}: {capacities:?}"
        );
        assert!(
            (capacities.whole_file_frame_limit + SINGLETON_FRAMING_SLACK) as u64
                <= capacities.chain_encoded_limit,
            "{cutoff}: {capacities:?}"
        );
    }
}

#[test]
fn issue_472_second_versions_are_deltas() {
    // At a 1 MiB cutoff the first version alone was above the fixed budgets:
    // 300,000 incompressible bytes above 256 KiB encoded, 600,000 compressible
    // bytes above 512 KiB canonical. The second version failed the Save.
    for (len, compressible) in [(300_000, false), (600_000, true)] {
        let saved = chain(1024 * KIB, &[(len, compressible), (len, compressible)]);
        assert_eq!(saved.depths(), [0, 1], "{len}");
        assert_eq!(saved.outcomes[1].delta.prefix_selected, 1, "{len}");
        assert_eq!(saved.outcomes[1].delta.work_exceeded, 0, "{len}");
        assert!(
            saved.outcomes[1].stored_bytes < 4 * KIB,
            "{len}: {:?}",
            saved.outcomes[1]
        );
        saved.assert_reads_back();
    }
}

#[test]
fn largest_file_at_each_cutoff_saves_its_next_version() {
    for cutoff in CUTOFFS {
        let len = cutoff as usize - 1;
        // A compressible base stores few encoded bytes: the next version chains.
        let saved = chain(cutoff, &[(len, true), (len, true)]);
        assert_eq!(saved.depths(), [0, 1], "{cutoff}");
        saved.assert_reads_back();
        // An incompressible base stores about one cutoff, and the prospective
        // charge adds the dependent's canonical length: about 22 bytes above two
        // cutoffs, so the next version is stored independently, by policy.
        let saved = chain(cutoff, &[(len, false), (len, false)]);
        assert_eq!(saved.depths(), [0, 0], "{cutoff}");
        assert_eq!(saved.outcomes[1].delta.work_exceeded, 1, "{cutoff}");
        saved.assert_reads_back();
    }
}

#[test]
fn a_chain_restarts_at_the_scaled_canonical_budget() {
    // 600,000-byte versions at a 1 MiB cutoff: the 4 MiB canonical budget holds
    // six of them, the base chain and the dependent together.
    let cutoff = 1024 * KIB;
    let len = 600_000;
    let canonical = (len + ENVELOPE) as u64;
    let fit = (capacities(cutoff).chain_canonical_limit / canonical) as usize;
    assert_eq!(fit, 6);
    let saved = chain(cutoff, &vec![(len, true); fit + 2]);
    let mut expected: Vec<usize> = (0..fit).collect();
    expected.extend([0, 1]);
    assert_eq!(saved.depths(), expected);
    assert_eq!(saved.outcomes[fit].delta.work_exceeded, 1);
    assert_eq!(
        saved
            .outcomes
            .iter()
            .map(|o| o.delta.work_exceeded)
            .sum::<u64>(),
        1
    );
    // The deepest version reconstructs six objects, far above the fixed 512 KiB,
    // and within the scaled budget the reader enforces.
    assert!(fit as u64 * canonical > 512 * KIB);
    assert!(fit as u64 * canonical <= capacities(cutoff).chain_canonical_limit);
    saved.assert_reads_back();
}
