//! Authenticated canonical acquisition and exact immutable cache keys.
use crate::cache::Cache;
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, ObjectId};
use std::sync::{Arc, Mutex};

const DEMAND_IDS: usize = 4096;
const DEMAND_BYTES: usize = 32 * 1024 * 1024;
const CANONICAL_BYTES: usize = 16 * 1024 * 1024;

/// Bounded cumulative object observations; no hidden namespace inventory.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ClientWork {
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub upstream_batches: u64,
    pub authenticated_bytes: u64,
    pub evictions: u64,
    pub charged_cache_bytes: usize,
    pub cached_objects: usize,
}
struct State {
    cache: Cache,
    work: ClientWork,
}
/// A runtime-backed authenticated object provider with bounded immutable caching.
/// Upstream implements its public demand-window/authority contract; allocation
/// before upstream returns remains that provider's responsibility.
pub struct CanonicalClient {
    source: Arc<dyn AuthenticatedObjects + Send + Sync>,
    state: Mutex<State>,
    lengths: Option<Arc<dyn crate::FileLengths + Send + Sync>>,
}
impl CanonicalClient {
    /// Selects a cache allowance. A large object can bypass cache without refusal.
    pub fn new(source: Arc<dyn AuthenticatedObjects + Send + Sync>, cache_bytes: usize) -> Self {
        Self {
            source,
            lengths: None,
            state: Mutex::new(State {
                cache: Cache::new(cache_bytes),
                work: ClientWork::default(),
            }),
        }
    }
    /// Adds the owning trusted Store-length port without payload classification.
    /// Missing capability fails explicitly; no FileView/stat fallback is used.
    pub fn with_lengths(
        source: Arc<dyn AuthenticatedObjects + Send + Sync>,
        lengths: Arc<dyn crate::FileLengths + Send + Sync>,
        cache_bytes: usize,
    ) -> Self {
        let mut client = Self::new(source, cache_bytes);
        client.lengths = Some(lengths);
        client
    }
    pub(crate) fn file_length(&self, id: ObjectId) -> crate::WorkspaceResult<u64> {
        self.lengths
            .as_ref()
            .ok_or(crate::WorkspaceError::MissingLengthProvider)?
            .file_length(id)
    }
    /// Bounded cumulative acquisition counters and logical cache charge.
    pub fn diagnostics(&self) -> ContentResult<ClientWork> {
        let state = self
            .state
            .lock()
            .map_err(|_| ContentError::ProviderFailure {
                what: "base cache owner",
            })?;
        let mut work = state.work;
        work.charged_cache_bytes = state.cache.charged();
        work.cached_objects = state.cache.entries();
        Ok(work)
    }
}
impl AuthenticatedObjects for CanonicalClient {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        if ids.len() > DEMAND_IDS {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "base demand ids",
                limit: DEMAND_IDS as u64,
                actual: ids.len() as u64,
            });
        }
        let mut values = vec![None; ids.len()];
        let mut misses = Vec::new();
        let mut cached_bytes = 0;
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| ContentError::ProviderFailure {
                    what: "base cache owner",
                })?;
            for (index, id) in ids.iter().enumerate() {
                if let Some(value) = state.cache.get(*id, DEMAND_BYTES - cached_bytes)? {
                    cached_bytes += value.len();
                    values[index] = Some(value);
                    state.work.cache_hits = state.work.cache_hits.saturating_add(1);
                } else {
                    misses.push((index, *id));
                    state.work.cache_misses = state.work.cache_misses.saturating_add(1);
                }
            }
            if !misses.is_empty() {
                state.work.upstream_batches = state.work.upstream_batches.saturating_add(1);
            }
        }
        // No cache lock spans transport, provider service or authentication.
        if !misses.is_empty() {
            let demand: Vec<_> = misses.iter().map(|(_, id)| *id).collect();
            let fetched = self.source.read_canonical_batch(&demand)?;
            if fetched.len() != misses.len() {
                return Err(ContentError::BatchCardinality {
                    requested: misses.len(),
                    returned: fetched.len(),
                });
            }
            for ((index, id), value) in misses.iter().zip(fetched) {
                if value.len() > CANONICAL_BYTES {
                    return Err(ContentError::ObjectLimitExceeded {
                        limit: CANONICAL_BYTES,
                        actual: value.len(),
                    });
                }
                if ObjectId::for_bytes(&value) != *id {
                    return Err(ContentError::IdentityMismatch);
                }
                values[*index] = Some(value);
            }
        }
        let mut result = Vec::with_capacity(ids.len());
        let mut bytes = 0_usize;
        for value in values {
            let value = value.ok_or(ContentError::BatchCardinality {
                requested: ids.len(),
                returned: result.len(),
            })?;
            bytes = bytes
                .checked_add(value.len())
                .ok_or(ContentError::LengthOverflow)?;
            if bytes > DEMAND_BYTES {
                return Err(ContentError::BoundedCapacityExceeded {
                    what: "base demand bytes",
                    limit: DEMAND_BYTES as u64,
                    actual: bytes as u64,
                });
            }
            result.push(value);
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| ContentError::ProviderFailure {
                what: "base cache owner",
            })?;
        for (index, id) in misses {
            let evictions = state.cache.insert(id, &result[index]);
            state.work.evictions = state.work.evictions.saturating_add(evictions);
            state.work.authenticated_bytes = state
                .work
                .authenticated_bytes
                .saturating_add(result[index].len() as u64);
        }
        Ok(result)
    }
}

impl crate::FileLengths for CanonicalClient {
    fn file_length(&self, id: ObjectId) -> crate::WorkspaceResult<u64> {
        CanonicalClient::file_length(self, id)
    }
}
