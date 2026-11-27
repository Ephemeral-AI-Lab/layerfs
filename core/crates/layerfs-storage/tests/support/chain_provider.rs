//! A provider that answers chain locates and group reads (#758) over the
//! memory engine, with switches that make its answers wrong on purpose.
#![allow(dead_code)]
use crate::metadata::MemoryMetadata;
use layerfs_content::{
    construct_bytes_with_predecessor, ConstructionPolicy, ContentResult, FinalizedConsumer,
    FinalizedObject, ObjectId, PredecessorBase,
};
use layerfs_storage::{
    location::{
        ChainMember, InodeValueRow, LocatedChain, LocatedChains, LocatedObject, PackDirectory,
        SignatureRow,
    },
    pack::layout,
    policy::{StoragePolicy, DEPENDENCY_PACK_CACHE_BYTES},
    port::*,
    Storage,
};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

type Edit = Box<dyn FnMut(&mut LocatedChains) + Send>;
type Step = Box<dyn FnOnce() + Send>;
type ClassifiedFault = Result<Vec<(i64, PersistenceError)>, PersistenceError>;

/// Chains follow the `base` the memory engine stored with each published row.
pub struct ChainProvider {
    pub inner: Arc<MemoryMetadata>,
    /// Members carry ids (a logical list) instead of `None` (a physical one).
    pub ids: bool,
    /// Directories of every pack the rows and members name.
    pub directories: bool,
    /// `read_groups` answers ranged requests with `Ranges`; else the default.
    pub ranged: bool,
    /// Rewrites each chain answer before it is returned.
    pub edit: Mutex<Option<Edit>>,
    /// Flips this byte, (pack, offset), wherever a returned range holds it.
    pub corrupt_range: Mutex<Option<(i64, usize)>>,
    /// Answers in reverse request order.
    pub reverse: bool,
    /// Every `read_groups` request.
    pub requests: Mutex<Vec<Vec<GroupRead>>>,
    /// Group pack failures in reply order, or a failure of the entire call.
    pub classified_fault: Mutex<Option<ClassifiedFault>>,
    /// Answers chain locates at all; else `locate` and no chains.
    pub chains: bool,
    /// The `batched_chain_reads` knob.
    pub batched: bool,
    /// The roots of every chain locate, in call order.
    pub asked: Mutex<Vec<Vec<ObjectId>>>,
    /// Runs before the next publication, as another writer would.
    pub before_publish: Mutex<Option<Step>>,
}
impl ChainProvider {
    pub fn new(inner: Arc<MemoryMetadata>) -> Self {
        Self {
            inner,
            ids: false,
            directories: true,
            ranged: true,
            edit: Mutex::new(None),
            corrupt_range: Mutex::new(None),
            reverse: false,
            requests: Mutex::new(Vec::new()),
            classified_fault: Mutex::new(None),
            chains: true,
            batched: false,
            asked: Mutex::new(Vec::new()),
            before_publish: Mutex::new(None),
        }
    }
    pub fn calls(&self, name: &str) -> usize {
        let calls = self.inner.calls.lock().unwrap();
        calls.iter().filter(|(call, _)| *call == name).count()
    }
}
impl PackPersistence for ChainProvider {
    fn policy(&self) -> Result<StoragePolicy, PersistenceError> {
        self.inner.policy()
    }
    fn locate(
        &self,
        ids: &[ObjectId],
        out: &mut Vec<LocatedObject>,
    ) -> Result<(), PersistenceError> {
        self.inner.locate(ids, out)
    }
    fn locate_chains(
        &self,
        roots: &[ObjectId],
        rows: &mut Vec<LocatedObject>,
        chains: &mut LocatedChains,
    ) -> Result<(), PersistenceError> {
        self.asked.lock().unwrap().push(roots.to_vec());
        self.inner.locate(roots, rows)?;
        chains.chains.clear();
        chains.directories.clear();
        if !self.chains {
            return Ok(());
        }
        let state = self.inner.state.lock().unwrap();
        let mut packs = BTreeSet::new();
        for row in rows.iter() {
            packs.insert(row.pack.pack_id);
            let mut members = Vec::new();
            let mut base = row.location.base;
            while let Some(id) = base {
                let location = state.objects[&id];
                members.push(ChainMember {
                    object_id: self.ids.then_some(id),
                    pack: state.packs[&location.pack_id].info,
                    group_number: location.group_number,
                    record_number: location.record_number,
                    canonical_length: location.canonical_length,
                });
                packs.insert(location.pack_id);
                base = location.base;
            }
            if !members.is_empty() {
                let root = row.location.object_id;
                chains.chains.push(LocatedChain { root, members });
            }
        }
        if self.directories {
            for id in packs {
                let pack = &state.packs[&id];
                let header = layout::parse_header(&pack.body).unwrap();
                let prefix = pack.body[..header.body_offset].to_vec();
                chains.directories.push(PackDirectory {
                    pack: pack.info,
                    prefix,
                });
            }
        }
        drop(state);
        if let Some(edit) = self.edit.lock().unwrap().as_mut() {
            edit(chains);
        }
        Ok(())
    }
    fn read_groups(
        &self,
        reads: &[GroupRead],
        out: &mut Vec<GroupReadAnswer>,
    ) -> Result<(), PersistenceError> {
        self.inner
            .calls
            .lock()
            .unwrap()
            .push(("read_groups", reads.len()));
        self.requests.lock().unwrap().push(reads.to_vec());
        let state = self.inner.state.lock().unwrap();
        let requested: usize = reads
            .iter()
            .map(|read| match read.ranges.is_empty() {
                true => read.pack.length,
                false => read.ranges.iter().map(|range| range.length).sum(),
            })
            .sum();
        assert!(
            requested <= DEPENDENCY_PACK_CACHE_BYTES,
            "byte cap per call"
        );
        out.clear();
        for read in reads {
            let Some(pack) = state.packs.get(&read.pack.pack_id) else {
                return Err(PersistenceError::Missing);
            };
            if read.ranges.is_empty() || !self.ranged {
                let body = pack.body.as_ref().clone();
                out.push(GroupReadAnswer::Whole(PersistedPack::authenticate(
                    pack.info, body,
                )?));
                continue;
            }
            let mut bodies: Vec<Vec<u8>> = read
                .ranges
                .iter()
                .map(|range| pack.body[range.offset..range.offset + range.length].to_vec())
                .collect();
            if let Some((pack, at)) = *self.corrupt_range.lock().unwrap() {
                for (range, body) in read.ranges.iter().zip(&mut bodies) {
                    let inside = range.offset <= at && at < range.offset + range.length;
                    if pack == read.pack.pack_id && inside {
                        body[at - range.offset] ^= 0x5a;
                    }
                }
            }
            out.push(GroupReadAnswer::Ranges(bodies));
        }
        if self.reverse {
            out.reverse();
        }
        Ok(())
    }
    fn batched_chain_reads(&self) -> bool {
        self.batched
    }
    fn read_groups_classified(
        &self,
        reads: &[GroupRead],
        out: &mut Vec<Result<GroupReadAnswer, PersistenceError>>,
    ) -> Result<(), PersistenceError> {
        let fault = self.classified_fault.lock().unwrap().take();
        let Some(fault) = fault else {
            let mut answers = Vec::new();
            self.read_groups(reads, &mut answers)?;
            out.extend(answers.into_iter().map(Ok));
            return Ok(());
        };
        self.requests.lock().unwrap().push(reads.to_vec());
        let mut faults = fault?.into_iter().peekable();
        let state = self.inner.state.lock().unwrap();
        for read in reads {
            assert!(read.ranges.is_empty(), "fixture requests whole packs");
            let answer = if faults
                .peek()
                .is_some_and(|(id, _)| *id == read.pack.pack_id)
            {
                Err(faults.next().unwrap().1)
            } else {
                let pack = &state.packs[&read.pack.pack_id];
                PersistedPack::authenticate(pack.info, pack.body.as_ref().clone())
                    .map(GroupReadAnswer::Whole)
            };
            out.push(answer);
        }
        assert!(
            faults.next().is_none(),
            "all injected faults were exercised"
        );
        Ok(())
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
        out: &mut Vec<InodeValueRow>,
    ) -> Result<(), PersistenceError> {
        self.inner.inode_values(ordinals, out)
    }
    fn signatures(&self, out: &mut Vec<SignatureRow>) -> Result<(), PersistenceError> {
        self.inner.signatures(out)
    }
    fn reserve(&self, request: Reserve) -> Result<Reserved, PersistenceError> {
        self.inner.reserve(request)
    }
    fn publish(&self, batch: &Publication) -> Result<Published, PersistenceError> {
        let step = self.before_publish.lock().unwrap().take();
        if let Some(step) = step {
            step();
        }
        self.inner.publish(batch)
    }
}

#[derive(Default)]
struct Emitted(Vec<FinalizedObject>);
impl FinalizedConsumer for Emitted {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.0.push(object);
        Ok(())
    }
}

fn noise(seed: u64, len: usize) -> Vec<u8> {
    let mut state = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    (0..len)
        .map(|_| {
            state ^= state << 7;
            state ^= state >> 9;
            state ^= state << 8;
            state as u8
        })
        .collect()
}

/// The shape of a [`history`].
#[derive(Clone, Copy)]
pub struct Shape {
    pub files: usize,
    pub versions: usize,
    pub size: usize,
    /// Each version rewritten whole instead of edited in a few bytes.
    pub rewrite: bool,
    /// The version whose Save opens a fresh handle, with cold caches.
    pub reopen: Option<usize>,
}

/// `files` small files saved over `versions` Saves through `wrap`'s port,
/// strict about hints, each version offered the last as its base. Returns
/// the Store, the newest version of each file and the writer's counts.
pub fn history(
    shape: Shape,
    wrap: impl Fn(Arc<MemoryMetadata>) -> Arc<dyn PackPersistence>,
) -> (
    Arc<MemoryMetadata>,
    Vec<ObjectId>,
    layerfs_storage::Diagnostics,
) {
    history_with(shape, wrap, |storage, objects| {
        let save = storage.begin_save().unwrap();
        for object in objects {
            save.accept(object).unwrap();
        }
        save.finish().unwrap();
    })
}

/// [`history`] whose last version's objects go to `last` instead of a Save.
pub fn history_with(
    shape: Shape,
    wrap: impl Fn(Arc<MemoryMetadata>) -> Arc<dyn PackPersistence>,
    last: impl FnOnce(&Storage, Vec<FinalizedObject>),
) -> (
    Arc<MemoryMetadata>,
    Vec<ObjectId>,
    layerfs_storage::Diagnostics,
) {
    let mut last = Some(last);
    let memory = Arc::new(MemoryMetadata::default());
    let open = || {
        let storage = Storage::new(wrap(memory.clone())).unwrap();
        storage.set_strict_hints(true);
        storage
    };
    let mut storage = open();
    let mut contents: Vec<Vec<u8>> = (0..shape.files)
        .map(|file| noise(file as u64 + 1, shape.size + file * 977))
        .collect();
    let mut roots: Vec<Option<ObjectId>> = vec![None; shape.files];
    let policy = ConstructionPolicy::frozen_default();
    for version in 0..shape.versions {
        if shape.reopen == Some(version) {
            storage = open();
        }
        for (file, bytes) in contents.iter_mut().enumerate() {
            if shape.rewrite {
                *bytes = noise((version * 1000 + file) as u64 + 7, bytes.len());
            } else {
                let at = (version * 7919 + file * 104_729) % (bytes.len() - 64);
                bytes[at..at + 64].copy_from_slice(&noise((version * 97 + file) as u64, 64));
            }
        }
        let reader = storage.reader().unwrap();
        let mut emitted = Emitted::default();
        let next: Vec<ObjectId> = contents
            .iter()
            .zip(&roots)
            .map(|(bytes, base)| {
                layerfs_telemetry::timer::Timing::disabled("test.construct", |scope| {
                    construct_bytes_with_predecessor(
                        policy,
                        &policy.capacities(),
                        bytes,
                        base.map(|root| PredecessorBase::new(&reader, root)),
                        &mut emitted,
                        scope.child("content"),
                    )
                })
                .0
                .unwrap()
                .root
            })
            .collect();
        drop(reader);
        // Marks where this version's Save starts in the call log.
        memory.calls.lock().unwrap().push(("save", version));
        if version + 1 == shape.versions {
            (last.take().unwrap())(&storage, emitted.0);
        } else {
            let save = storage.begin_save().unwrap();
            for object in emitted.0 {
                save.accept(object).unwrap();
            }
            save.finish().unwrap();
        }
        roots = next.into_iter().map(Some).collect();
    }
    let roots = roots.into_iter().map(Option::unwrap).collect();
    (memory, roots, storage.diagnostics())
}

/// Reads `ids` through a fresh handle over `port`.
pub fn read(
    port: Arc<dyn PackPersistence>,
    ids: &[ObjectId],
    strict: bool,
) -> (
    layerfs_storage::StorageResult<Vec<Vec<u8>>>,
    layerfs_storage::Diagnostics,
) {
    let storage = Storage::new(port).unwrap();
    storage.set_strict_hints(strict);
    let read = storage.reader().unwrap().read_objects(ids);
    (read, storage.diagnostics())
}

/// The depth of `id`'s stored chain.
pub fn depth(memory: &MemoryMetadata, id: ObjectId) -> usize {
    let state = memory.state.lock().unwrap();
    let mut depth = 0;
    let mut base = state.objects[&id].base;
    while let Some(next) = base {
        depth += 1;
        base = state.objects[&next].base;
    }
    depth
}

/// A fresh copy of the Store with an empty call log.
pub fn copy(memory: &MemoryMetadata) -> Arc<MemoryMetadata> {
    let state = memory.state.lock().unwrap().clone();
    let copy = MemoryMetadata::default();
    *copy.state.lock().unwrap() = state;
    Arc::new(copy)
}
