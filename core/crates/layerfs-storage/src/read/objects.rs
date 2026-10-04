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
use std::collections::BTreeMap;

pub(crate) struct ReadState {
    workspace: DecompressionWorkspace,
    groups: GroupCache,
    pool: PoolReader,
    packs: BTreeMap<i64, Vec<u8>>,
}
impl ReadState {
    pub(crate) fn new() -> StorageResult<Self> {
        Ok(Self {
            workspace: DecompressionWorkspace::new()?,
            groups: GroupCache::new(),
            pool: PoolReader::new(),
            packs: BTreeMap::new(),
        })
    }
    pub(crate) fn pooled_read_counters(&self) -> crate::encoding::pool::PoolReadCounters {
        self.pool.counters()
    }
    pub(crate) fn read(
        &mut self,
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
        prefetch::chains(source, &roots, &mut self.packs, &mut self.workspace)?;
        let mut counters = ChainCounters::default();
        let mut out = Vec::with_capacity(ids.len());
        for root in roots {
            let (canonical, _) = Resolver::new(
                source,
                i64::MAX,
                capacities,
                BodyCaches {
                    packs: &mut self.packs,
                    pool: &mut self.pool,
                },
                &mut self.groups,
                &mut self.workspace,
                &mut counters,
            )
            .resolve_at(root)?;
            out.push(canonical);
        }
        Ok(out)
    }
}
