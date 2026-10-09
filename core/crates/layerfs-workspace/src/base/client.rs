//! Authenticated canonical acquisition and exact immutable cache keys.
use crate::cache::CanonicalCache;
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, ObjectId};
use std::sync::Arc;

const DEMAND_IDS: usize = 4096;
const DEMAND_BYTES: usize = 32 * 1024 * 1024;
const CANONICAL_BYTES: usize = 16 * 1024 * 1024;
/// Largest object a memory-only read copies. Such a read runs inside an owner
/// job; a larger object is left to the ordinary demand outside it.
const RESIDENT_BYTES: usize = 64 * 1024;

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
    /// Base directories whose child-directory count was derived by listing.
    pub directory_count_scans: u64,
    /// Child-directory counts currently remembered; at most
    /// [`crate::DIRECTORY_COUNT_CAPACITY`].
    pub directory_counts: usize,
    /// File lengths currently remembered. Each is charged to the cache
    /// allowance and counted in `charged_cache_bytes`, not in `cached_objects`.
    pub file_lengths: usize,
}
/// A runtime-backed authenticated object provider with bounded immutable caching.
/// Upstream implements its public demand-window/authority contract; allocation
/// before upstream returns remains that provider's responsibility.
pub struct CanonicalClient {
    source: Arc<dyn AuthenticatedObjects + Send + Sync>,
    cache: Arc<CanonicalCache>,
    lengths: Option<Arc<dyn crate::FileLengths + Send + Sync>>,
    /// Answers only from the cache: no upstream demand is ever made.
    resident: bool,
}
/// The upstream of a memory-only client. It is never asked.
struct NoUpstream;
impl AuthenticatedObjects for NoUpstream {
    fn read_canonical_batch(&self, _: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        Err(NOT_RESIDENT)
    }
}
const NOT_RESIDENT: ContentError = ContentError::ProviderFailure {
    what: "base object not resident",
};
impl CanonicalClient {
    /// A client over objects already in `cache`, with no provider and no
    /// length provider: it answers a file length only when the cache
    /// remembers it. A demand for anything else fails without I/O, and
    /// without counting a miss: the ordinary demand that follows counts it.
    pub fn resident(cache: Arc<CanonicalCache>) -> Self {
        Self {
            source: Arc::new(NoUpstream),
            cache,
            lengths: None,
            resident: true,
        }
    }
    /// Selects a cache allowance. A large object can bypass cache without refusal.
    pub fn new(source: Arc<dyn AuthenticatedObjects + Send + Sync>, cache_bytes: usize) -> Self {
        Self::with_cache(source, None, Arc::new(CanonicalCache::new(cache_bytes)))
    }
    /// Adds the owning trusted Store-length port without payload classification.
    /// Missing capability fails explicitly; no FileView/stat fallback is used.
    pub fn with_lengths(
        source: Arc<dyn AuthenticatedObjects + Send + Sync>,
        lengths: Arc<dyn crate::FileLengths + Send + Sync>,
        cache_bytes: usize,
    ) -> Self {
        Self::with_cache(
            source,
            Some(lengths),
            Arc::new(CanonicalCache::new(cache_bytes)),
        )
    }
    /// Uses one existing cache allowance with this operation's owning providers.
    ///
    /// Provider failure/partial-result custody remains on the supplied source;
    /// it is never placed in the shared cache. The caller must authorize the
    /// operation and restrict cache sharing to its declared authorization context.
    /// Cached immutable identity alone supplies no authority or revocation fence.
    pub fn with_cache(
        source: Arc<dyn AuthenticatedObjects + Send + Sync>,
        lengths: Option<Arc<dyn crate::FileLengths + Send + Sync>>,
        cache: Arc<CanonicalCache>,
    ) -> Self {
        Self {
            source,
            cache,
            lengths,
            resident: false,
        }
    }
    /// The length provider's own answer, asked every time: nothing is
    /// remembered and nothing remembered is used. Commit construction checks
    /// a captured file against it.
    pub(crate) fn provided_length(&self, id: ObjectId) -> crate::WorkspaceResult<u64> {
        self.lengths
            .as_ref()
            .ok_or(crate::WorkspaceError::MissingLengthProvider)?
            .file_length(id)
    }
    /// Bounded cumulative acquisition counters and logical cache charge.
    pub fn diagnostics(&self) -> ContentResult<ClientWork> {
        self.cache.diagnostics()
    }
    /// Whether this client answers from memory only.
    pub(crate) const fn is_resident(&self) -> bool {
        self.resident
    }
    fn state(&self) -> ContentResult<std::sync::MutexGuard<'_, crate::cache::State>> {
        self.cache
            .state
            .lock()
            .map_err(|_| ContentError::ProviderFailure {
                what: "base cache owner",
            })
    }
    /// The remembered child-directory count of one directory content root.
    pub(crate) fn directory_count(&self, root: ObjectId) -> ContentResult<Option<u64>> {
        Ok(self.state()?.counts.get(root))
    }
    /// Remembers one derived count; the least recently used one makes room.
    pub(crate) fn remember_directory_count(&self, root: ObjectId, count: u64) -> ContentResult<()> {
        let mut state = self.state()?;
        state.counts.insert(root, count);
        state.work.directory_count_scans = state.work.directory_count_scans.saturating_add(1);
        Ok(())
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
                .cache
                .state
                .lock()
                .map_err(|_| ContentError::ProviderFailure {
                    what: "base cache owner",
                })?;
            for (index, id) in ids.iter().enumerate() {
                if self.resident && !state.cache.within(*id, RESIDENT_BYTES) {
                    return Err(NOT_RESIDENT);
                }
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
            .cache
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
    /// The length of one regular file's content root, for its attributes.
    /// It is a pure function of that immutable root, so an answer the
    /// provider gave once is remembered in the cache's own allowance and
    /// returned from memory afterwards, with no provider demand. A
    /// memory-only client answers nothing else. An evicted answer costs one
    /// more provider demand and nothing else.
    fn file_length(&self, id: ObjectId) -> crate::WorkspaceResult<u64> {
        if let Some(length) = self.state()?.cache.length(id) {
            return Ok(length);
        }
        let length = self.provided_length(id)?;
        let mut state = self.state()?;
        let evictions = state.cache.remember_length(id, length);
        state.work.evictions = state.work.evictions.saturating_add(evictions);
        Ok(length)
    }
}
