//! F-036: a Save that loses a first-wins registration to a concurrent writer
//! must resolve the winner's whole dependency chain, including a base this
//! Save's wave located as absent before the other writer published it.
#[path = "support/memory_metadata.rs"]
mod metadata;
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
    let q = hinted(whole(&edited(&first, 100, b"F036-q01")), &p);
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

/// A tentative unpublished copy of `v1` (`v1 -> v0`) in `Packer::ready` must
/// never shadow the committed first-wins copy of `v1` in `Fetch` when walking
/// a committed base chain (`v0 -> v1`), or `DepthCache::cost_of` / `Resolver`
/// observes a false-positive `Integrity("dependency cycle")`.
#[test]
fn ready_pack_entry_never_shadows_a_committed_base_into_a_cycle() {
    let base_bytes = support::noise(4096);
    let p_bytes = support::noise(4096);
    let p = whole(&p_bytes);
    let x = hinted(whole(&edited(&p_bytes, 100, b"F723-x0!")), &p);
    let v1 = whole(&base_bytes);
    let v0_plain = whole(&edited(&base_bytes, 100, b"F723-v0!"));
    let v0 = hinted(v0_plain.clone(), &v1);
    let v1_hint_v0 = hinted(v1.clone(), &v0);
    let v2 = hinted(whole(&edited(&base_bytes, 300, b"F723-v2!")), &v0);
    // Incompressible payloads large enough to seal `v1_hint_v0` out of the
    // open `WholeFile` group and flush `Packer::queued` into `Packer::ready`
    // (> PACK_LIMIT = 256 KiB) before `v2` is offered.
    let pad_noise = support::noise(6 * 60 * 1024);
    let pads: Vec<_> = (0..6)
        .map(|i| whole(&pad_noise[i * 60 * 1024..(i + 1) * 60 * 1024]))
        .collect();

    let shared = Arc::new(MemoryMetadata::default());
    let save_one = |metadata: Arc<MemoryMetadata>, object: FinalizedObject| {
        let store = Storage::new(metadata).unwrap();
        let save = store.begin_save().unwrap();
        save.accept(object).unwrap();
        save.finish().unwrap()
    };
    let (step_shared, v1_other, v0_other) = (shared.clone(), v1.clone(), v0.clone());
    let other_prefix = Arc::new(Mutex::new(0_u64));
    let recorded = other_prefix.clone();
    let steps: Vec<Box<dyn FnOnce() + Send>> = vec![Box::new(move || {
        save_one(step_shared.clone(), v1_other);
        *recorded.lock().unwrap() = save_one(step_shared, v0_other).prefix_records;
    })];
    let storage = Storage::new(Arc::new(Interleaved {
        inner: shared.clone(),
        before_publish: Mutex::new(steps),
    }))
    .unwrap();

    // Wave:
    // 1. `v0` (FULL) enters `Packer`.
    // 2. `x` (hint `v0`) triggers `register_ready()`; `before_publish` commits
    //    `v1` (FULL) and `v0` (PREFIX -> `v1`). `v0` loses first-wins and is
    //    invalidated from `Fetch::absent`, while `v1` remains marked absent.
    // 3. `v1_hint_v0` is not skipped (still in `absent`), sees `v0 -> v1` in
    //    `Fetch`, and encodes `v1 -> v0` into `Packer`.
    // 4. `pad` seals `v1 -> v0` into `Packer::ready`.
    // 5. `v2` (hint `v0`) does not flush `Packer::ready` because `v0` is already
    //    committed, and walks `v0 -> v1` via `WaveSource::base_location`.
    let save = storage.begin_save().unwrap();
    for object in [&p, &v0_plain, &x, &v1_hint_v0] {
        save.accept(object.clone()).unwrap();
    }
    for pad in &pads {
        save.accept(pad.clone()).unwrap();
    }
    save.accept(v2.clone()).unwrap();
    save.finish()
        .unwrap_or_else(|error| panic!("Save hit cycle or integrity error: {error:?}"));
    assert_eq!(
        *other_prefix.lock().unwrap(),
        1,
        "fixture: concurrent writer must store v0 as PREFIX on v1"
    );

    let reader = Storage::new(shared).unwrap();
    let ids = [v0.id(), v1.id(), v2.id()];
    let read = reader.reader().unwrap().read_objects(&ids).unwrap();
    assert_eq!(
        read,
        vec![
            v0.canonical().to_vec(),
            v1.canonical().to_vec(),
            v2.canonical().to_vec(),
        ]
    );
}

/// When a Save loses first-wins on `v0` after encoding its private copy of
/// `v0` against `v1`, and the concurrent winner already stored `v1` as a
/// `PREFIX` on `v0`, `acknowledge()`'s `Wave::resolve(v0)` walks from the
/// unpublished `v0` through committed `v1` to committed `v0`. Different
/// `(pack_id, group, record)` locators must not be rejected as a cycle (#723).
#[test]
fn lost_candidate_resolving_through_winner_chain_to_same_object_id_is_not_a_cycle() {
    let base_bytes = support::noise(4096);
    let v0 = whole(&base_bytes);
    let v1_plain = whole(&edited(&base_bytes, 100, b"F723-v1!"));
    let v1_hint_v0 = hinted(v1_plain.clone(), &v0);
    let v0_hint_v1 = hinted(v0.clone(), &v1_plain);
    let p_bytes = support::noise(4096);
    let p = whole(&p_bytes);
    let x = hinted(whole(&edited(&p_bytes, 100, b"F723-x0!")), &p);

    let shared = Arc::new(MemoryMetadata::default());
    let save_one = |metadata: Arc<MemoryMetadata>, object: FinalizedObject| {
        let store = Storage::new(metadata).unwrap();
        let save = store.begin_save().unwrap();
        save.accept(object).unwrap();
        save.finish().unwrap()
    };
    let (step_shared, v0_other, v1_other) = (shared.clone(), v0.clone(), v1_hint_v0.clone());
    let other_prefix = Arc::new(Mutex::new(0_u64));
    let recorded = other_prefix.clone();
    let steps: Vec<Box<dyn FnOnce() + Send>> = vec![Box::new(move || {
        save_one(step_shared.clone(), v0_other);
        *recorded.lock().unwrap() = save_one(step_shared, v1_other).prefix_records;
    })];
    let storage = Storage::new(Arc::new(Interleaved {
        inner: shared.clone(),
        before_publish: Mutex::new(steps),
    }))
    .unwrap();

    // 1. `p` enters `Packer`, `x` (hint `p`) triggers `register_ready()`.
    // 2. `before_publish` commits `v0` (FULL) and `v1` (PREFIX -> `v0`).
    // 3. `v1_plain` is published by `a` or `v1` is resolved via `x`? No,
    //    `v1_hint_v0`? Here `a` offers `v0_hint_v1`: `v0` is still in `absent`
    //    from the wave start, while `v1` is located via `register_ready`? No:
    //    `v1_plain` is also offered first so `register_ready` invalidates `v1`,
    //    then `v0_hint_v1` encodes `v0 -> v1` and loses `v0` at `finish()`.
    let save = storage.begin_save().unwrap();
    for object in [&p, &x, &v1_plain, &v0_hint_v1] {
        save.accept(object.clone()).unwrap();
    }
    save.finish()
        .unwrap_or_else(|error| panic!("Lost candidate resolution hit cycle: {error:?}"));
    assert_eq!(
        *other_prefix.lock().unwrap(),
        1,
        "fixture: concurrent writer must store v1 as PREFIX on v0"
    );

    let reader = Storage::new(shared).unwrap();
    let read = reader
        .reader()
        .unwrap()
        .read_objects(&[v0.id(), v1_plain.id()])
        .unwrap();
    assert_eq!(
        read,
        vec![v0.canonical().to_vec(), v1_plain.canonical().to_vec()]
    );
}
