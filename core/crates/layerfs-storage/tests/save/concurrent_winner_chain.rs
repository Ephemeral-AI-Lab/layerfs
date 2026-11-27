//! F-036: a Save that loses a first-wins registration to a concurrent writer
//! must resolve the winner's whole dependency chain, including a base this
//! Save's wave located as absent before the other writer published it.
#[path = "../support/memory_metadata.rs"]
mod metadata;
#[path = "../support/mod.rs"]
mod support;
use layerfs_content::{AdvisoryPredecessors, FinalizedObject, ObjectRole};
use layerfs_storage::{port::*, Storage};
use metadata::MemoryMetadata;
use std::sync::{Arc, Mutex};

/// Forwards to the shared engine; before each publication through it, runs the
/// next step of the other writer to completion, as a second daemon would.
struct Interleaved {
    inner: Arc<MemoryMetadata>,
    before_publish: Mutex<Vec<Box<dyn FnOnce() + Send>>>,
}
impl PackPersistence for Interleaved {
    fn policy(&self) -> Result<layerfs_storage::policy::StoragePolicy, PersistenceError> {
        self.inner.policy()
    }
    fn locate(
        &self,
        ids: &[layerfs_content::ObjectId],
        out: &mut Vec<layerfs_storage::location::LocatedObject>,
    ) -> Result<(), PersistenceError> {
        self.inner.locate(ids, out)
    }
    fn read_packs(
        &self,
        ids: &[i64],
        out: &mut Vec<PersistedPack>,
    ) -> Result<(), PersistenceError> {
        self.inner.read_packs(ids, out)
    }
    fn read_pack_selection(
        &self,
        id: i64,
        plan: &mut dyn PackReadPlan,
    ) -> Result<PersistedPackRead, PersistenceError> {
        self.inner.read_pack_selection(id, plan)
    }
    fn read_scoped_pack(
        &self,
        id: i64,
        plan: &mut dyn PackReadPlan,
    ) -> Result<AcquiredPackRead, PersistenceError> {
        self.inner.read_scoped_pack(id, plan)
    }
    fn inode_values(
        &self,
        ordinals: &[u32],
        out: &mut Vec<layerfs_storage::location::InodeValueRow>,
    ) -> Result<(), PersistenceError> {
        self.inner.inode_values(ordinals, out)
    }
    fn signatures(
        &self,
        out: &mut Vec<layerfs_storage::location::SignatureRow>,
    ) -> Result<(), PersistenceError> {
        self.inner.signatures(out)
    }
    fn reserve(&self, request: Reserve) -> Result<Reserved, PersistenceError> {
        self.inner.reserve(request)
    }
    fn publish(&self, batch: &Publication) -> Result<Published, PersistenceError> {
        let next = {
            let mut steps = self.before_publish.lock().unwrap();
            (!steps.is_empty()).then(|| steps.remove(0))
        };
        if let Some(other) = next {
            other();
        }
        self.inner.publish(batch)
    }
}

fn whole(raw: &[u8]) -> FinalizedObject {
    FinalizedObject::new(ObjectRole::WholeFile, support::assembled_small_object(raw)).unwrap()
}

fn edited(raw: &[u8], at: usize, tag: &[u8; 8]) -> Vec<u8> {
    let mut out = raw.to_vec();
    out[at..at + 8].copy_from_slice(tag);
    out
}

fn hinted(object: FinalizedObject, base: &FinalizedObject) -> FinalizedObject {
    object.with_predecessors(AdvisoryPredecessors::explicit(base.id()).unwrap())
}

/// The S4 interleaving, made deterministic. This Save (b) and another writer
/// (a) hold the same new content. b's first forced registration loses P to a;
/// re-reading the winner clears b's negative locator set. b then probes Z as a
/// delta base and records it absent. Before b's next registration, a publishes
/// Z and O, O stored as PREFIX on Z. b then admits O: the fresh locator finds
/// a's O, and its chain names Z, which b still holds as absent.
#[test]
fn a_concurrent_winner_chain_resolves_a_base_this_wave_located_absent() {
    let first = support::noise(1024);
    let second: Vec<u8> = support::noise(2048)[1024..].to_vec();
    let third = support::patterned(1024);
    let p = whole(&first);
    // Q names P but shares none of its content, so the similarity ring
    // proposes nothing for it: its declared base alone forces the
    // registration (a ring proposal whose trial wins is held to the wave's
    // end instead, #810).
    let unrelated: Vec<u8> = support::noise(3072)[2048..].to_vec();
    let q = hinted(whole(&unrelated), &p);
    let z = whole(&second);
    let r = hinted(whole(&third), &z);
    let s = hinted(whole(&edited(&first, 600, b"F036-s01")), &q);
    let o = whole(&edited(&second, 300, b"F036-o01"));

    let shared = Arc::new(MemoryMetadata::default());
    let other_prefix = Arc::new(Mutex::new(0_u64));
    // Each step is its own Save on its own handle, like one Commit of daemon a.
    let save_one = |metadata: Arc<MemoryMetadata>, object: FinalizedObject| {
        let store = Storage::new(metadata).unwrap();
        let save = store.begin_save().unwrap();
        save.accept(object).unwrap();
        save.finish().unwrap()
    };
    let (step_one, p_other) = (shared.clone(), p.clone());
    let (step_two, z_other, o_other) = (shared.clone(), z.clone(), hinted(o.clone(), &z));
    let recorded = other_prefix.clone();
    let steps: Vec<Box<dyn FnOnce() + Send>> = vec![
        Box::new(move || {
            save_one(step_one, p_other);
        }),
        Box::new(move || {
            save_one(step_two.clone(), z_other);
            *recorded.lock().unwrap() = save_one(step_two, o_other).prefix_records;
        }),
    ];
    let storage = Storage::new(Arc::new(Interleaved {
        inner: shared.clone(),
        before_publish: Mutex::new(steps),
    }))
    .unwrap();

    // One wave: P, Q (hint P), R (hint Z), S (hint Q), O, Z.
    let save = storage.begin_save().unwrap();
    for object in [&p, &q, &r, &s, &o, &z] {
        save.accept(object.clone()).unwrap();
    }
    let outcome = save
        .finish()
        .unwrap_or_else(|error| panic!("Save over a winner's chain to Z {}: {error:?}", z.id()));
    assert_eq!(
        *other_prefix.lock().unwrap(),
        1,
        "fixture: the other writer must store O as PREFIX on Z"
    );
    assert!(outcome.reused >= 3, "P, O and Z are reuses: {outcome:?}");

    let reader = Storage::new(shared).unwrap();
    let ids: Vec<_> = [&p, &q, &r, &s, &o, &z].iter().map(|o| o.id()).collect();
    let read = reader.reader().unwrap().read_objects(&ids).unwrap();
    let expected: Vec<_> = [&p, &q, &r, &s, &o, &z]
        .iter()
        .map(|o| o.canonical().to_vec())
        .collect();
    assert_eq!(read, expected);
}
