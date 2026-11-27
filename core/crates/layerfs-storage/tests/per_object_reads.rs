//! Per-object results of one batched demand (#818 H5): each object's result
//! is the one its point read returns, healthy objects cost what the batched
//! read costs, a damaged unit fails only the objects that need it and is not
//! requested again, and a transport failure fails the whole call.
#[path = "support/chain_provider.rs"]
mod chains;
#[path = "support/memory_metadata.rs"]
mod metadata;

use chains::{copy, history, ChainProvider, Shape};
use layerfs_content::{
    inode_leaf::{
        encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeValue, LEAF_ROW_BYTES,
    },
    AdvisoryPredecessors, FinalizedObject, ObjectId, ObjectRole, PredecessorProvenance,
};
use layerfs_storage::{
    location::{InodeValueRow, LocatedChains, LocatedObject, SignatureRow},
    policy::StoragePolicy,
    port::*,
    ReadCache, Storage, StorageResult,
};
use metadata::MemoryMetadata;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

/// A port over another that refuses one pack's body as `Missing` (its
/// catalogue rows stay), fails its `fail_at`-th read call as a transport
/// failure, and logs every read call with the packs it names.
struct Faulty {
    inner: Arc<dyn PackPersistence>,
    missing: Option<i64>,
    fail_at: Option<usize>,
    log: Mutex<Vec<(&'static str, Vec<i64>)>>,
}
impl Faulty {
    fn new(inner: Arc<dyn PackPersistence>) -> Self {
        Self {
            inner,
            missing: None,
            fail_at: None,
            log: Mutex::new(Vec::new()),
        }
    }
    /// Logs one read call; refuses it when it is the chosen one or names the
    /// missing pack.
    fn call(&self, name: &'static str, packs: &[i64]) -> Result<(), PersistenceError> {
        let mut log = self.log.lock().unwrap();
        let at = log.len();
        log.push((name, packs.to_vec()));
        if self.fail_at == Some(at) {
            return Err(match at % 2 {
                0 => PersistenceError::Uncertain,
                _ => PersistenceError::BackendUnavailable,
            });
        }
        if self.missing.is_some_and(|id| packs.contains(&id)) {
            return Err(PersistenceError::Missing);
        }
        Ok(())
    }
    fn requests_of(&self, pack: i64) -> usize {
        let log = self.log.lock().unwrap();
        log.iter()
            .filter(|(_, packs)| packs.contains(&pack))
            .count()
    }
}
impl PackPersistence for Faulty {
    fn policy(&self) -> Result<StoragePolicy, PersistenceError> {
        self.inner.policy()
    }
    fn locate(
        &self,
        ids: &[ObjectId],
        out: &mut Vec<LocatedObject>,
    ) -> Result<(), PersistenceError> {
        self.call("locate", &[])?;
        self.inner.locate(ids, out)
    }
    fn locate_chains(
        &self,
        roots: &[ObjectId],
        rows: &mut Vec<LocatedObject>,
        chains: &mut LocatedChains,
    ) -> Result<(), PersistenceError> {
        self.call("locate_chains", &[])?;
        self.inner.locate_chains(roots, rows, chains)
    }
    fn read_groups(
        &self,
        reads: &[GroupRead],
        out: &mut Vec<GroupReadAnswer>,
    ) -> Result<(), PersistenceError> {
        let packs: Vec<i64> = reads.iter().map(|read| read.pack.pack_id).collect();
        self.call("read_groups", &packs)?;
        self.inner.read_groups(reads, out)
    }
    fn read_packs(
        &self,
        ids: &[i64],
        out: &mut Vec<PersistedPack>,
    ) -> Result<(), PersistenceError> {
        self.call("read_packs", ids)?;
        self.inner.read_packs(ids, out)
    }
    fn read_pack_selection(
        &self,
        id: i64,
        plan: &mut dyn PackReadPlan,
    ) -> Result<PersistedPackRead, PersistenceError> {
        self.call("read_pack_selection", &[id])?;
        self.inner.read_pack_selection(id, plan)
    }
    fn read_scoped_pack(
        &self,
        id: i64,
        plan: &mut dyn PackReadPlan,
    ) -> Result<AcquiredPackRead, PersistenceError> {
        self.call("read_scoped_pack", &[id])?;
        self.inner.read_scoped_pack(id, plan)
    }
    fn inode_values(
        &self,
        ordinals: &[u32],
        out: &mut Vec<InodeValueRow>,
    ) -> Result<(), PersistenceError> {
        self.call("inode_values", &[])?;
        self.inner.inode_values(ordinals, out)
    }
    fn signatures(&self, out: &mut Vec<SignatureRow>) -> Result<(), PersistenceError> {
        self.inner.signatures(out)
    }
    fn reserve(&self, request: Reserve) -> Result<Reserved, PersistenceError> {
        self.inner.reserve(request)
    }
    fn publish(&self, batch: &Publication) -> Result<Published, PersistenceError> {
        self.inner.publish(batch)
    }
    fn whole_pack_bytes(&self) -> usize {
        self.inner.whole_pack_bytes()
    }
    fn batched_chain_reads(&self) -> bool {
        self.inner.batched_chain_reads()
    }
}

/// How reads reach the Store: the plain memory engine (one call per unit, as
/// SQLite) or the chain provider that batches chains and groups (as
/// PostgreSQL).
#[derive(Clone, Copy, Debug)]
enum Route {
    Plain,
    Batched,
}

/// A fresh port over a copy of `memory`'s state.
fn port(memory: &MemoryMetadata, route: Route, missing: Option<i64>) -> Arc<Faulty> {
    let state = copy(memory);
    let inner: Arc<dyn PackPersistence> = match route {
        Route::Plain => state,
        Route::Batched => {
            let mut chains = ChainProvider::new(state);
            chains.batched = true;
            Arc::new(chains)
        }
    };
    let mut faulty = Faulty::new(inner);
    faulty.missing = missing;
    Arc::new(faulty)
}

fn show(result: &StorageResult<Vec<u8>>) -> String {
    match result {
        Ok(bytes) => format!("ok {}", ObjectId::for_bytes(bytes)),
        Err(error) => format!("{error:?}"),
    }
}

/// Each object read alone, through a fresh handle over a fresh copy.
fn alone(
    memory: &MemoryMetadata,
    route: Route,
    missing: Option<i64>,
    ids: &[ObjectId],
) -> Vec<String> {
    ids.iter()
        .map(|id| {
            let storage = Storage::new(port(memory, route, missing)).unwrap();
            let read = storage.reader().unwrap().read_objects(&[*id]);
            show(&read.map(|mut values| values.pop().unwrap()))
        })
        .collect()
}

/// One per-object demand through a fresh handle, and its port.
fn each(
    memory: &MemoryMetadata,
    route: Route,
    missing: Option<i64>,
    ids: &[ObjectId],
) -> (Vec<String>, Arc<Faulty>) {
    let port = port(memory, route, missing);
    let storage = Storage::new(port.clone()).unwrap();
    let each = storage.reader().unwrap().read_objects_each(ids).unwrap();
    (each.iter().map(show).collect(), port)
}

/// Every stored object, in id order: all versions of every file.
fn stored(memory: &MemoryMetadata) -> Vec<ObjectId> {
    memory
        .state
        .lock()
        .unwrap()
        .objects
        .keys()
        .copied()
        .collect()
}

fn ordinary_history() -> Arc<MemoryMetadata> {
    let shape = Shape {
        files: 5,
        versions: 5,
        size: 3_000,
        rewrite: false,
        reopen: None,
    };
    history(shape, |memory| memory as Arc<dyn PackPersistence>).0
}

/// The objects whose stored record names a base, with that base.
fn based(memory: &MemoryMetadata) -> Vec<(ObjectId, ObjectId)> {
    let state = memory.state.lock().unwrap();
    state
        .objects
        .values()
        .filter_map(|row| row.base.map(|base| (row.object_id, base)))
        .collect()
}

/// Asserts the damaged demand's results equal the point reads, some fail
/// and some are served; returns how many failed.
fn check(memory: &MemoryMetadata, route: Route, missing: Option<i64>, ids: &[ObjectId]) -> usize {
    let (batched, _) = each(memory, route, missing, ids);
    let point = alone(memory, route, missing, ids);
    for (at, (batched, point)) in batched.iter().zip(&point).enumerate() {
        assert_eq!(batched, point, "{route:?} object {at} ({})", ids[at]);
    }
    let failed = batched.iter().filter(|r| !r.starts_with("ok ")).count();
    assert!(failed > 0, "{route:?}: the damage is met");
    assert!(failed < ids.len(), "{route:?}: healthy objects are served");
    failed
}

#[test]
fn healthy_demands_cost_what_the_batched_read_costs() {
    let memory = ordinary_history();
    let ids = stored(&memory);
    for route in [Route::Plain, Route::Batched] {
        let batch_port = port(&memory, route, None);
        let storage = Storage::new(batch_port.clone()).unwrap();
        let reader = storage.reader().unwrap();
        let batch = reader.read_objects(&ids).unwrap();
        let (batch_counts, batch_pooled) = (storage.diagnostics(), reader.pooled_read_counters());
        drop(reader);

        let each_port = port(&memory, route, None);
        let storage = Storage::new(each_port.clone()).unwrap();
        let reader = storage.reader().unwrap();
        let each = reader.read_objects_each(&ids).unwrap();
        assert_eq!(storage.diagnostics(), batch_counts, "{route:?}");
        assert_eq!(reader.pooled_read_counters(), batch_pooled, "{route:?}");
        assert_eq!(
            *each_port.log.lock().unwrap(),
            *batch_port.log.lock().unwrap()
        );
        let each: Vec<Vec<u8>> = each.into_iter().map(Result::unwrap).collect();
        assert_eq!(each, batch, "{route:?}");

        // Through caller-kept caches too.
        let kept_port = port(&memory, route, None);
        let storage = Storage::new(kept_port.clone()).unwrap();
        let mut cache = ReadCache::new().unwrap();
        let kept = storage.read_each_with(&mut cache, &ids).unwrap();
        assert_eq!(storage.diagnostics(), batch_counts, "{route:?}");
        assert_eq!(
            *kept_port.log.lock().unwrap(),
            *batch_port.log.lock().unwrap()
        );
        assert_eq!(
            kept.into_iter().map(Result::unwrap).collect::<Vec<_>>(),
            batch
        );
    }
}

#[test]
fn a_missing_object_is_its_own_result() {
    let memory = ordinary_history();
    let mut ids = stored(&memory);
    // A newest version no other object names, and an id never stored.
    let named: Vec<ObjectId> = based(&memory).into_iter().map(|(_, base)| base).collect();
    let leaf = *ids.iter().rev().find(|id| !named.contains(id)).unwrap();
    memory.state.lock().unwrap().objects.remove(&leaf);
    ids.push(ObjectId::for_bytes(b"never stored"));
    for route in [Route::Plain, Route::Batched] {
        assert_eq!(check(&memory, route, None, &ids), 2, "{route:?}");
    }
}

#[test]
fn a_mismatched_object_is_its_own_result() {
    let memory = ordinary_history();
    let ids = stored(&memory);
    {
        // One object's row names another object's record: its bytes do not
        // have its identity.
        let mut state = memory.state.lock().unwrap();
        let rows: Vec<_> = state.objects.values().copied().collect();
        let (wrong, right) = (rows[0], rows[rows.len() / 2]);
        let mut moved = right;
        moved.object_id = wrong.object_id;
        state.objects.insert(wrong.object_id, moved);
    }
    for route in [Route::Plain, Route::Batched] {
        check(&memory, route, None, &ids);
    }
}

#[test]
fn an_undecodable_body_fails_only_the_objects_that_need_it() {
    let memory = ordinary_history();
    let ids = stored(&memory);
    {
        // One byte of the last pack's last group flipped.
        let mut state = memory.state.lock().unwrap();
        let pack = state.packs.values_mut().last().unwrap();
        let mut body = pack.body.as_ref().clone();
        let at = body.len() - 40;
        body[at] ^= 0x5a;
        pack.body = Arc::new(body);
    }
    check(&memory, Route::Plain, None, &ids);
}

#[test]
fn a_broken_chain_fails_only_its_dependents() {
    let memory = ordinary_history();
    let ids = stored(&memory);
    // The deepest base of one chain is gone: every object above it fails
    // with its absence, as each one's own read does.
    let (_, base) = based(&memory)
        .into_iter()
        .find(|(_, base)| memory.state.lock().unwrap().objects[base].base.is_none())
        .unwrap();
    memory.state.lock().unwrap().objects.remove(&base);
    let failed = check(&memory, Route::Plain, None, &ids);
    assert!(failed >= 2, "the base and at least one dependent");
}

#[test]
fn a_missing_pack_fails_only_its_objects_and_is_requested_once() {
    let memory = ordinary_history();
    let ids = stored(&memory);
    // A pack holding a chain's base, whose dependents live in other packs.
    let edges = based(&memory);
    let pack = {
        let state = memory.state.lock().unwrap();
        let (_, base) = edges
            .into_iter()
            .find(|(child, base)| state.objects[child].pack_id != state.objects[base].pack_id)
            .unwrap();
        state.objects[&base].pack_id
    };
    check(&memory, Route::Plain, Some(pack), &ids);
    let (_, plain) = each(&memory, Route::Plain, Some(pack), &ids);
    assert_eq!(
        plain.requests_of(pack),
        1,
        "the failed pack is not asked again"
    );
    // On the batched route the pack is one of a group read over several
    // packs, which this provider (as PostgreSQL's `read_groups`) refuses as
    // a whole call: no object can be blamed, so the demand fails, once.
    let port = port(&memory, Route::Batched, Some(pack));
    let storage = Storage::new(port.clone()).unwrap();
    let read = storage.reader().unwrap().read_objects_each(&ids);
    assert!(read.is_err());
    assert_eq!(port.requests_of(pack), 1);
}

#[test]
fn a_transport_failure_at_any_call_fails_the_demand() {
    let memory = ordinary_history();
    let ids = stored(&memory);
    for route in [Route::Plain, Route::Batched] {
        let (_, clean) = each(&memory, route, None, &ids);
        let calls = clean.log.lock().unwrap().len();
        assert!(calls > 0);
        for at in 0..calls {
            let mut faulty = Faulty::new(match route {
                Route::Plain => copy(&memory) as Arc<dyn PackPersistence>,
                Route::Batched => {
                    let mut chains = ChainProvider::new(copy(&memory));
                    chains.batched = true;
                    Arc::new(chains)
                }
            });
            faulty.fail_at = Some(at);
            let storage = Storage::new(Arc::new(faulty)).unwrap();
            let read = storage.reader().unwrap().read_objects_each(&ids);
            assert!(read.is_err(), "{route:?} call {at}: the whole demand fails");
        }
    }
}

const ROWS: u64 = 8;

/// A pooled leaf of `ROWS` rows from `first_serial`; its first `rewrites`
/// rows carry rewritten values.
fn leaf(first_serial: u64, rewrites: u64) -> FinalizedObject {
    let rows: Vec<_> = (0..ROWS)
        .map(|row| InodeLeafRow {
            serial: first_serial + row,
            value: encode_inode_value(InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 1,
                content_root: ObjectId::for_bytes(
                    &[
                        &(first_serial + row).to_be_bytes()[..],
                        &[u8::from(row < rewrites)],
                    ]
                    .concat(),
                ),
                metadata_root: layerfs_content::object::inode_leaf::InodeMetadata::Root(
                    ObjectId::for_bytes(b"metadata"),
                ),
            }),
        })
        .collect();
    let canonical = InodeLeaf {
        subtree_bytes: rows.len() as u64 * LEAF_ROW_BYTES as u64,
        rows,
    }
    .encode()
    .unwrap();
    FinalizedObject::new(ObjectRole::InodeLeaf, canonical).unwrap()
}

/// Four leaves, the first with three later versions (a pooled delta chain),
/// one Save each.
fn pooled_history() -> Arc<MemoryMetadata> {
    let memory = Arc::new(MemoryMetadata::default());
    let storage = Storage::new(memory.clone()).unwrap();
    let save = |objects: &[FinalizedObject]| {
        let save = storage.begin_save().unwrap();
        for object in objects {
            save.accept(object.clone()).unwrap();
        }
        save.finish().unwrap();
    };
    save(&[leaf(1, 0), leaf(100, 0), leaf(200, 0), leaf(300, 0)]);
    let mut last = leaf(1, 0);
    for rewrites in 1..4 {
        let mut named = AdvisoryPredecessors::new();
        named
            .push(last.id(), PredecessorProvenance::OriginalBase)
            .unwrap();
        let next = leaf(1, rewrites).with_predecessors(named);
        save(&[next.clone()]);
        last = next;
    }
    memory
}

#[test]
fn pooled_leaves_healthy_cost_and_bad_leaves() {
    let memory = pooled_history();
    let ids = stored(&memory);
    assert!(based(&memory).len() >= 2, "a pooled delta chain is stored");
    for route in [Route::Plain, Route::Batched] {
        let batch_port = port(&memory, route, None);
        let storage = Storage::new(batch_port.clone()).unwrap();
        let reader = storage.reader().unwrap();
        let batch = reader.read_objects(&ids).unwrap();
        let (counts, pooled) = (storage.diagnostics(), reader.pooled_read_counters());
        drop(reader);
        let each_port = port(&memory, route, None);
        let storage = Storage::new(each_port.clone()).unwrap();
        let reader = storage.reader().unwrap();
        let each = reader.read_objects_each(&ids).unwrap();
        assert_eq!(storage.diagnostics(), counts, "{route:?}");
        assert_eq!(reader.pooled_read_counters(), pooled, "{route:?}");
        assert!(pooled.value_lookups >= 1);
        assert_eq!(
            *each_port.log.lock().unwrap(),
            *batch_port.log.lock().unwrap()
        );
        assert_eq!(
            each.into_iter().map(Result::unwrap).collect::<Vec<_>>(),
            batch
        );
    }

    // A stored value of one leaf is gone: the leaves that hold it fail with
    // its absence in the window's one lookup, the others are served.
    let gone = copy(&memory);
    {
        let mut state = gone.state.lock().unwrap();
        let ordinal = *state.values.keys().last().unwrap();
        state.values.remove(&ordinal);
    }
    for route in [Route::Plain, Route::Batched] {
        check(&gone, route, None, &ids);
    }

    // One leaf's row names a record its group does not hold: its rebuild
    // fails, the window's other leaves stay prepared.
    let bad = copy(&memory);
    {
        let mut state = bad.state.lock().unwrap();
        let row = state
            .objects
            .values_mut()
            .find(|row| row.base.is_none() && row.record_number > 0)
            .unwrap();
        row.record_number += 1_000;
    }
    check(&bad, Route::Plain, None, &ids);
}

#[test]
fn lengths_each_match_lengths_alone() {
    let memory = ordinary_history();
    let mut ids: Vec<ObjectId> = stored(&memory)
        .into_iter()
        .filter(|id| {
            let state = memory.state.lock().unwrap();
            matches!(
                state.objects[id].role,
                ObjectRole::WholeFile | ObjectRole::FileState
            )
        })
        .collect();
    assert!(ids.len() >= 4);
    let named: Vec<ObjectId> = based(&memory).into_iter().map(|(_, base)| base).collect();
    let gone = *ids.iter().rev().find(|id| !named.contains(id)).unwrap();
    memory.state.lock().unwrap().objects.remove(&gone);
    ids.push(ObjectId::for_bytes(b"never stored"));
    let storage = Storage::new(port(&memory, Route::Plain, None)).unwrap();
    let mut cache = ReadCache::new().unwrap();
    let each = storage.lengths_each_with(&mut cache, &ids).unwrap();
    let mut by_id = BTreeMap::new();
    for (id, length) in ids.iter().zip(&each) {
        let storage = Storage::new(port(&memory, Route::Plain, None)).unwrap();
        let alone = storage.reader().unwrap().file_lengths(&[*id]);
        let alone = alone.map(|lengths| lengths[0]);
        assert_eq!(format!("{length:?}"), format!("{alone:?}"), "{id}");
        by_id.insert(*id, alone.is_ok());
    }
    assert_eq!(by_id.values().filter(|ok| !**ok).count(), 2);
}
