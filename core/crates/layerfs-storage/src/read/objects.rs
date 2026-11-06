//! One bounded canonical demand using the existing authenticated resolver.

use super::{prefetch, Fetch};
use crate::{
    encoding::{
        codec::DecompressionWorkspace,
        delta::read::{BodyCaches, ChainCounters, Resolver},
        pool::PoolReader,
        GroupCache,
    },
    error::{StorageError, StorageResult},
    policy::{StorageCapacities, READ_CANONICAL_BYTES_LIMIT},
    source::Source,
};
use layerfs_content::ObjectId;
use std::{
    collections::BTreeSet,
    ops::{Deref, DerefMut},
    sync::{Mutex, PoisonError},
};

/// Decode regions of finished reads, kept for the next read (F-043).
///
/// A freed 1 MiB decode region stays in the glibc arena of the thread that
/// freed it, like the encode regions (`save/state.rs`): about 80 MB in the
/// SDK runner. Only plain regions are kept, never a zstd context, and at most
/// one per available CPU; a region past that is freed. Releasing them when
/// the Store closes is a follow-up once `store/handle.rs` is free.
static IDLE_DECODE: Mutex<Vec<Vec<u64>>> = Mutex::new(Vec::new());

/// A decode workspace that takes an idle region when there is one (so its
/// region is already resident) and returns its region when dropped.
pub(super) struct Decoder(Option<DecompressionWorkspace>);
impl Decoder {
    pub(super) fn new() -> StorageResult<Self> {
        let idle = IDLE_DECODE
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop();
        Ok(Self(Some(match idle {
            Some(region) => DecompressionWorkspace::in_region(region)?,
            None => DecompressionWorkspace::new()?,
        })))
    }
}
impl Deref for Decoder {
    type Target = DecompressionWorkspace;
    fn deref(&self) -> &DecompressionWorkspace {
        self.0.as_ref().expect("decoder until drop")
    }
}
impl DerefMut for Decoder {
    fn deref_mut(&mut self) -> &mut DecompressionWorkspace {
        self.0.as_mut().expect("decoder until drop")
    }
}
impl Drop for Decoder {
    fn drop(&mut self) {
        let Some(region) = self.0.take().and_then(DecompressionWorkspace::into_region) else {
            return;
        };
        let bound = std::thread::available_parallelism().map_or(1, usize::from);
        let mut idle = IDLE_DECODE.lock().unwrap_or_else(PoisonError::into_inner);
        if idle.len() < bound {
            idle.push(region);
        }
    }
}

/// One owner's bounded pack, group and pooled-value caches. The decode
/// arena is the caller's, passed to each read.
pub(crate) struct ReadState {
    groups: GroupCache,
    pool: PoolReader,
    prefetch_pool: crate::encoding::pool::PoolReadCounters,
    packs: crate::encoding::PackCache,
}
impl ReadState {
    pub(crate) fn new() -> Self {
        Self {
            groups: GroupCache::new(),
            pool: PoolReader::new(),
            prefetch_pool: crate::encoding::pool::PoolReadCounters::default(),
            packs: crate::encoding::PackCache::new(),
        }
    }
    /// Body, decoded-group and pooled-value bytes kept now; the decode arena
    /// is apart. The read path passes its pool reader the shared pack cache,
    /// so the pool's own pack cache stays empty here.
    pub(crate) fn retained_bytes(&self) -> usize {
        self.packs.retained_bytes() + self.groups.retained_bytes() + self.pool.retained_bytes()
    }
    pub(crate) fn pooled_read_counters(&self) -> crate::encoding::pool::PoolReadCounters {
        let mut work = self.pool.counters();
        work.accumulate(self.prefetch_pool);
        work
    }
    pub(crate) fn read(
        &mut self,
        workspace: &mut DecompressionWorkspace,
        source: &Fetch,
        capacities: &StorageCapacities,
        ids: &[ObjectId],
    ) -> StorageResult<Vec<Vec<u8>>> {
        if ids.len() > capacities.read_objects {
            return Err(StorageError::CapacityExceeded {
                what: "storage.read_objects",
                limit: capacities.read_objects as u64,
                actual: ids.len() as u64,
            });
        }
        source.begin_demand();
        source.locate(ids)?;
        let roots = ids
            .iter()
            .map(|id| {
                source
                    .location(*id, i64::MAX)?
                    .ok_or(StorageError::ObjectMissing(*id))
            })
            .collect::<StorageResult<Vec<_>>>()?;
        let bytes = roots.iter().try_fold(0usize, |total, row| {
            total
                .checked_add(row.canonical_length)
                .ok_or(StorageError::Integrity("read byte accounting"))
        })?;
        if bytes > READ_CANONICAL_BYTES_LIMIT {
            return Err(StorageError::CapacityExceeded {
                what: "storage.read_canonical_bytes",
                limit: READ_CANONICAL_BYTES_LIMIT as u64,
                actual: bytes as u64,
            });
        }
        let located: Vec<i64> = roots.iter().map(|row| row.pack_id).collect();
        source.whole_packs(&located, &mut self.packs)?;
        let prefetch =
            prefetch::chains(source, &roots, &mut self.packs, workspace, &mut self.groups)?;
        self.prefetch_pool.accumulate(prefetch);
        let mut counters = ChainCounters::default();
        // Output slots preserve demand order, including repeated identities.
        // Only the bounded root worklist is scheduled by physical location.
        let mut order: Vec<_> = (0..roots.len()).collect();
        order.sort_unstable_by_key(|position| {
            let row = roots[*position];
            (row.pack_id, row.group_number, row.record_number)
        });
        let mut out = vec![Vec::new(); ids.len()];
        let mut from = 0;
        while from < order.len() {
            let id = roots[order[from]].pack_id;
            let end = from
                + order[from..]
                    .iter()
                    .take_while(|position| roots[**position].pack_id == id)
                    .count();
            let needed: Vec<_> = order[from..end]
                .iter()
                .map(|position| roots[*position].group_number)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            self.packs.acquire_groups(source, id, &needed)?;
            for &position in &order[from..end] {
                let root = roots[position];
                let (canonical, _) = Resolver::new(
                    source,
                    i64::MAX,
                    capacities,
                    BodyCaches {
                        packs: &mut self.packs,
                        pool: &mut self.pool,
                    },
                    &mut self.groups,
                    workspace,
                    &mut counters,
                )
                .resolve_at(root)?;
                out[position] = canonical;
            }
            from = end;
        }
        Ok(out)
    }
}
