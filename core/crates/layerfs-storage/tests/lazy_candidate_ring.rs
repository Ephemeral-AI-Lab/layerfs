//! The delta-candidate ring loads at its first use, and selection is the same
//! as with a ring loaded before the Save began (#352 record 034, S1-b).
//!
//! Two Stores run one Save sequence:
//! - the lazy arm opens a fresh Storage handle for each Save (as a one-shot
//!   daemon does), so every Save begins with an unloaded ring;
//! - the loaded arm keeps one handle (as a warm daemon does), so after the
//!   first Save every Save begins with the ring already loaded.
//!
//! The sequence is a FULL new-file Save, a Save whose file finds its declared
//! base, a Save whose file finds its base through the ring, a Save that fails
//! at publication after admitting a new file into the ring, and a Save after
//! the failure on the same handle.
//!
//! After each Save the two Stores hold identical packs, locators and ring rows,
//! so they hold the same object ids, representations and delta bases. Each
//! Save reports the same delta outcomes in both arms. A Save that never uses
//! the ring does not read the table in the lazy arm.
#[path = "support/memory_metadata.rs"]
mod metadata;
mod support;

use layerfs_content::{AdvisoryPredecessors, FinalizedObject, ObjectId, ObjectRole};
use layerfs_storage::encoding::delta::select::DeltaCounters;
use layerfs_storage::port::PersistenceError;
use layerfs_storage::{Storage, StoragePolicy};
use metadata::MemoryMetadata;
use std::sync::Arc;

/// 8 KiB of content distinct for each `seed`.
fn file(seed: u64) -> Vec<u8> {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64 ^ seed.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    (0..8 * 1024)
        .map(|_| {
            state ^= state.wrapping_shl(7);
            state ^= state.wrapping_shr(9);
            state ^= state.wrapping_shl(8);
            state as u8
        })
        .collect()
}

/// `file(seed)` with one byte changed: a near copy.
fn near(seed: u64) -> Vec<u8> {
    let mut raw = file(seed);
    raw[4_000] ^= 0x5a;
    raw
}

fn object(raw: &[u8], base: Option<ObjectId>) -> FinalizedObject {
    let object =
        FinalizedObject::new(ObjectRole::WholeFile, support::assembled_small_object(raw)).unwrap();
    match base {
        Some(base) => object.with_predecessors(AdvisoryPredecessors::explicit(base).unwrap()),
        None => object,
    }
}

fn id(raw: &[u8]) -> ObjectId {
    object(raw, None).id()
}

struct Arm {
    metadata: Arc<MemoryMetadata>,
    handle: Option<Storage>,
    /// Keep one handle for every Save: the ring stays loaded between Saves.
    keep: bool,
}

impl Arm {
    fn new(keep: bool) -> Self {
        let metadata = Arc::new(MemoryMetadata::default());
        metadata.state.lock().unwrap().policy = StoragePolicy::default();
        Self {
            metadata,
            handle: None,
            keep,
        }
    }

    /// Runs one Save of `objects` and returns its delta outcomes, or the
    /// error it failed with, plus how many times it read the ring table.
    fn save(
        &mut self,
        objects: Vec<FinalizedObject>,
        fresh: bool,
        fail: bool,
    ) -> (Result<DeltaCounters, String>, usize) {
        if self.handle.is_none() || (fresh && !self.keep) {
            self.handle = Some(Storage::new(self.metadata.clone()).unwrap());
        }
        if fail {
            self.metadata.state.lock().unwrap().fail_register = Some(PersistenceError::Malformed);
        }
        self.metadata.calls.lock().unwrap().clear();
        let save = self.handle.as_ref().unwrap().begin_save().unwrap();
        for object in objects {
            save.accept(object).unwrap();
        }
        let outcome = save
            .finish()
            .map(|outcome| outcome.delta)
            .map_err(|error| format!("{error:?}"));
        let reads = self
            .metadata
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(call, _)| *call == "signatures")
            .count();
        (outcome, reads)
    }
}

/// The packs, in id order, and the locators with each pack id replaced by
/// that pack's rank. A kept handle keeps its unused pack ids for its next
/// Save while a fresh handle reserves new ones (#768), so the two arms
/// number the same packs differently; their order, bytes and every locator's
/// position in them must still agree.
type Ranked = (
    Vec<(layerfs_storage::location::PackDomain, Vec<u8>)>,
    Vec<(layerfs_storage::location::ObjectLocation, usize)>,
);
fn ranked(state: &metadata::MetadataState) -> Ranked {
    let rank: std::collections::BTreeMap<i64, usize> = state
        .packs
        .keys()
        .enumerate()
        .map(|(rank, id)| (*id, rank))
        .collect();
    let packs = state
        .packs
        .values()
        .map(|pack| {
            assert_eq!(pack.info.length, pack.body.len());
            (pack.info.domain, pack.body.as_ref().clone())
        })
        .collect();
    let objects = state
        .objects
        .values()
        .map(|row| {
            // Every field but the pack id, which is compared by rank.
            let row = layerfs_storage::location::ObjectLocation { pack_id: 0, ..*row };
            (row, rank[&state.objects[&row.object_id].pack_id])
        })
        .collect();
    (packs, objects)
}

fn same_store(lazy: &Arm, loaded: &Arm, step: &str) {
    let (lazy, loaded) = (
        lazy.metadata.state.lock().unwrap(),
        loaded.metadata.state.lock().unwrap(),
    );
    let (lazy_packs, lazy_objects) = ranked(&lazy);
    let (loaded_packs, loaded_objects) = ranked(&loaded);
    assert_eq!(lazy_packs, loaded_packs, "{step}: packs");
    assert_eq!(lazy_objects, loaded_objects, "{step}: locators");
    assert_eq!(lazy.signatures, loaded.signatures, "{step}: ring rows");
}

/// Runs one Save in both arms and checks they agree.
fn both(
    lazy: &mut Arm,
    loaded: &mut Arm,
    objects: impl Fn() -> Vec<FinalizedObject>,
    fresh: bool,
    fail: bool,
    step: &str,
) -> (Result<DeltaCounters, String>, usize) {
    let (got, reads) = lazy.save(objects(), fresh, fail);
    let (want, _) = loaded.save(objects(), fresh, fail);
    assert_eq!(got, want, "{step}: delta outcomes");
    same_store(lazy, loaded, step);
    (got, reads)
}

#[test]
fn a_lazily_loaded_ring_selects_what_a_loaded_ring_selects() {
    let mut lazy = Arm::new(false);
    let mut loaded = Arm::new(true);
    // 1. A FULL new-file Save: three files nothing names as a base.
    let (outcome, reads) = both(
        &mut lazy,
        &mut loaded,
        || (1..=3).map(|seed| object(&file(seed), None)).collect(),
        true,
        false,
        "full",
    );
    let outcome = outcome.unwrap();
    assert_eq!((outcome.no_candidate, outcome.prefix_selected), (3, 0));
    assert_eq!(reads, 1, "the first insert loads the ring");
    assert_eq!(lazy.metadata.state.lock().unwrap().signatures.len(), 3);

    // 2. A near copy of file 1 that declares file 1: the ring is not used.
    let (outcome, reads) = both(
        &mut lazy,
        &mut loaded,
        || vec![object(&near(1), Some(id(&file(1))))],
        true,
        false,
        "declared",
    );
    assert_eq!(outcome.unwrap().prefix_selected, 1);
    assert_eq!(reads, 0, "a Save that never uses the ring reads no table");

    // 3. A near copy of file 2 that declares nothing: the ring proposes file 2.
    let (outcome, reads) = both(
        &mut lazy,
        &mut loaded,
        || vec![object(&near(2), None)],
        true,
        false,
        "ring",
    );
    let outcome = outcome.unwrap();
    assert_eq!((outcome.no_candidate, outcome.prefix_selected), (0, 1));
    assert_eq!(reads, 1);

    // 4. A new file 4 enters the ring, then publication fails: nothing of it
    // is stored and the ring is invalidated.
    let rows = lazy.metadata.state.lock().unwrap().signatures.clone();
    let (outcome, _) = both(
        &mut lazy,
        &mut loaded,
        || vec![object(&file(4), None)],
        true,
        true,
        "failed",
    );
    assert!(outcome.is_err());
    assert_eq!(lazy.metadata.state.lock().unwrap().signatures, rows);

    // 5. On the same handle: a near copy of the failed file 4 finds nothing,
    // and a near copy of file 3 finds file 3 through the reloaded ring.
    let (outcome, reads) = both(
        &mut lazy,
        &mut loaded,
        || vec![object(&near(4), None), object(&near(3), None)],
        false,
        false,
        "after failure",
    );
    let outcome = outcome.unwrap();
    assert_eq!((outcome.no_candidate, outcome.prefix_selected), (1, 1));
    assert_eq!(reads, 1, "the invalidated ring is read again once");
    let state = lazy.metadata.state.lock().unwrap();
    assert!(!state.objects.contains_key(&id(&file(4))));
    assert!(state.objects.contains_key(&id(&near(4))));
}
