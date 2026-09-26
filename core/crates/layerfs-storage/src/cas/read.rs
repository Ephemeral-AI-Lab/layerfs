//! Batched object reads and the retained-pack visibility ceiling.
//!
//! One read captures its permitted pack ceiling once and applies it to every
//! acquired location, including every dependency read; the ceiling never moves
//! while the read is in progress. Pack bodies are read once per pack per wave,
//! dependencies are reconstructed iteratively under fixed work budgets, and every
//! reconstructed object - base or requested - is authenticated against the
//! identity it was stored under.

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::Connection;

use layerfs_content::ObjectId;

use crate::encoding::codec::DecompressionWorkspace;
use crate::encoding::delta::read::{BodyCaches, ChainCounters, Resolver};
use crate::encoding::pool::PoolReader;
use crate::error::{StorageError, StorageResult};
use crate::policy::StorageCapacities;
use crate::sqlite::{connection, lookup, schema};

/// Work performed by one bounded read wave.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReadCounters {
    /// Objects returned.
    pub objects: u64,
    /// Pack bodies read.
    pub packs_read: u64,
    /// Membership/locator pages issued.
    pub pages: u64,
    /// Ceiling applied to every acquired location.
    pub ceiling: i64,
    /// Dependency edges followed.
    pub edges: u64,
    /// Longest dependency chain reconstructed.
    pub max_depth: u64,
    /// Canonical bytes reconstructed, including dependencies.
    pub canonical_bytes: u64,
    /// Ordinary-lane group bodies decompressed by this wave.
    ///
    /// The wave's decode work, distinct from `packs_read` (one per pack per wave)
    /// and from `objects`: the defect this counter exists to price is one group
    /// decompression per **record** inside a pack that is read once.
    pub group_decodes: u64,
    /// Pooled metadata reconstruction work, separate from ordinary-lane counts.
    pub pooled: crate::encoding::pool::PoolReadCounters,
}

/// Reads every requested object in demand order under one ceiling.
///
/// `pool` is the pooled metadata reader this read reconstructs pooled leaves
/// through. It is the **caller's**, so its lifetime is the caller's reuse scope:
/// the ordinary-lane pack cache below is one wave's, and a pooled reader built
/// inside this function would be one leaf's. Both bounds are unchanged
/// ([`crate::policy::DEPENDENCY_PACK_CACHE_BYTES`],
/// [`crate::policy::POOLED_VALUE_CACHE_BYTES`]).
pub fn read_objects(
    connection: &Connection,
    ids: &[ObjectId],
    ceiling: i64,
    capacities: &StorageCapacities,
    workspace: &mut DecompressionWorkspace,
    groups: &mut crate::encoding::GroupCache,
    pool: &mut PoolReader,
) -> StorageResult<(Vec<Vec<u8>>, ReadCounters)> {
    // Locators are collected above the ceiling on purpose: a record that exists
    // but is not yet published must be reported as a visibility refusal, never
    // mistaken for a missing object. Dependency reads inside the resolver do use
    // the captured ceiling.
    let locations = lookup::candidates(connection, ids, i64::MAX)?;
    let mut counters = ReadCounters {
        ceiling,
        pages: ids.len().div_ceil(crate::policy::LOOKUP_PAGE_IDS) as u64,
        ..ReadCounters::default()
    };
    let mut by_id: BTreeMap<ObjectId, lookup::ObjectLocation> = BTreeMap::new();
    let mut hidden = std::collections::BTreeSet::new();
    for (location, _, eligible) in locations {
        if !eligible {
            hidden.insert(location.object_id);
            continue;
        }
        if location.pack_id > ceiling {
            return Err(StorageError::VisibilityCeiling {
                pack_id: location.pack_id,
                ceiling,
            });
        }
        by_id.entry(location.object_id).or_insert(location);
    }
    let bytes = ids.iter().try_fold(0usize, |total, id| {
        let location = by_id.get(id).ok_or_else(|| {
            if hidden.contains(id) {
                StorageError::Unpublished(*id)
            } else {
                StorageError::ObjectMissing(*id)
            }
        })?;
        total
            .checked_add(location.canonical_length)
            .ok_or(StorageError::Integrity("read byte accounting"))
    })?;
    check_read_bytes(bytes)?;
    let mut packs: BTreeMap<i64, Vec<u8>> = BTreeMap::new();
    let mut chain = ChainCounters::default();
    let mut totals = ChainCounters::default();
    let mut values = Vec::with_capacity(ids.len());
    for id in ids {
        let location = by_id
            .get(id)
            .copied()
            .ok_or(StorageError::ObjectMissing(*id))?;
        let ((canonical, verified), packs_fetched) = {
            let mut resolver = Resolver::new(
                connection,
                ceiling,
                capacities,
                BodyCaches {
                    packs: &mut packs,
                    pool: &mut *pool,
                },
                groups,
                workspace,
                &mut chain,
            );
            let resolved = resolver.resolve_at(location)?;
            (resolved, resolver.packs_read())
        };
        totals.pooled.accumulate(chain.pooled);
        totals.objects = totals.objects.saturating_add(chain.objects);
        totals.edges = totals.edges.saturating_add(chain.edges);
        totals.encoded_bytes = totals.encoded_bytes.saturating_add(chain.encoded_bytes);
        totals.canonical_bytes = totals.canonical_bytes.saturating_add(chain.canonical_bytes);
        totals.max_depth = totals.max_depth.max(chain.max_depth);
        totals.group_decodes = totals.group_decodes.saturating_add(chain.group_decodes);
        counters.packs_read += packs_fetched;
        // The resolver already hashed these bytes to authenticate them against
        // the locator, and returned that identity (P2-6). Comparing identities
        // is the same check without a second pass over the object.
        if verified != *id {
            return Err(StorageError::Integrity("read identity"));
        }
        counters.objects += 1;
        values.push(canonical);
    }
    counters.edges = totals.edges;
    counters.max_depth = totals.max_depth;
    counters.canonical_bytes = totals.canonical_bytes;
    counters.group_decodes = totals.group_decodes;
    counters.pooled = totals.pooled;
    Ok((values, counters))
}

/// Refuses a demand larger than the declared read ceiling.
///
/// The ceiling is a caller-declared resource, not a trigger: a wave that exceeds
/// it fails before it reads anything. It is checked here as well as at the Store's
/// own entry point because the pooled session is a second way into
/// [`read_objects`], and a bound that only one of two doors enforces is not a
/// bound.
pub(crate) fn check_read_demand(ids: &[ObjectId], limit: usize) -> StorageResult<()> {
    if ids.len() > limit {
        return Err(StorageError::CapacityExceeded {
            what: "storage.read_objects",
            limit: limit as u64,
            actual: ids.len() as u64,
        });
    }
    Ok(())
}

pub(crate) fn check_read_bytes(bytes: usize) -> StorageResult<()> {
    if bytes > crate::policy::READ_CANONICAL_BYTES_LIMIT {
        return Err(StorageError::CapacityExceeded {
            what: "storage.read_canonical_bytes",
            limit: crate::policy::READ_CANONICAL_BYTES_LIMIT as u64,
            actual: bytes as u64,
        });
    }
    Ok(())
}

/// One operation's pooled read session.
///
/// A wave pays two fixed costs before it reads anything: a connection (with the
/// declared pragma profile) and a decode arena. Both belong to the **operation**
/// rather than to the wave, so a session opens them once and every wave of that
/// operation reuses them. What is deliberately **not** pooled is the visibility
/// scope: publication and the pack range ceiling are re-read for every wave, and a
/// save that completed between two waves is visible to the second one. Pooling it
/// would turn one operation's later waves into a snapshot of its first.
pub struct ReadSession {
    connection: Connection,
    arbitration: std::sync::Arc<std::sync::Mutex<()>>,
    workspace: DecompressionWorkspace,
    /// Decoded ordinary-lane group bodies this operation already materialised.
    ///
    /// The cache belongs to the **operation**, not to one wave: a group read by
    /// one wave is served to the next without decompressing it again, which is
    /// what makes `k` records of one group cost one decompression across an
    /// operation. It is bounded by [`DECODED_GROUP_CACHE_BYTES`] and released
    /// wholesale when the bound is crossed, the pooled value cache's discipline.
    ///
    /// Its one hazard is stated where it is handled: the ceiling is **not**
    /// pooled (it is re-read per wave), so a body cached under an older, higher
    /// ceiling must never answer an ineligible location. Publication-aware
    /// lookup and the resolver's pack range check run before consulting the cache.
    groups: crate::encoding::GroupCache,
    /// The operation's pooled metadata reader: pack bodies and decoded value groups.
    ///
    /// It belongs to the **operation**, exactly as the decoded-group cache above
    /// does and for the same reason. The pooled reader used to be built once per
    /// resolved inode leaf, so a pack a later leaf of the same operation demanded
    /// was copied out of SQLite again - measured as 176x the whole pack space over
    /// one stride10 run. Nothing about its bounds changes here: the pack cache is
    /// still [`crate::policy::DEPENDENCY_PACK_CACHE_BYTES`] and the decoded value
    /// cache still [`crate::policy::POOLED_VALUE_CACHE_BYTES`], both released
    /// wholesale when the next body would cross them.
    ///
    /// **Why this lifetime is sound.** A pack body at or below the ceiling a wave
    /// was authorized under is immutable - a save creates its packs above the
    /// baseline publication watermark and publishes them only by advancing that
    /// watermark - so a retained body cannot go stale for a reader that never
    /// writes. The ceiling itself is *not* pooled: it is re-read for every wave,
    /// and every pooled consult checks the location's pack against the current
    /// ceiling before the cache is consulted, so a body cached under an older,
    /// higher ceiling can never answer a location this wave must not see. A
    /// **writing** owner keeps its own reader instead and releases cached pack
    /// bodies after each write (`cas::placement::MutationOwner::write_pack`):
    /// its pooled lane may append to the one open pack.
    pool: PoolReader,
}

impl ReadSession {
    /// Opens one session over `path`: a connection with the declared profile and
    /// an empty decode arena, which materialises on the first decompression.
    pub fn open(path: &Path) -> StorageResult<Self> {
        Ok(Self {
            connection: connection::open(path, false)?,
            arbitration: crate::sqlite::ownership::arbitration(path)?,
            workspace: DecompressionWorkspace::new()?,
            groups: crate::encoding::GroupCache::new(),
            pool: PoolReader::new(),
        })
    }

    /// Reads one wave under a ceiling captured for this wave alone.
    pub fn read(
        &mut self,
        ids: &[ObjectId],
        capacities: &StorageCapacities,
    ) -> StorageResult<(Vec<Vec<u8>>, ReadCounters)> {
        check_read_demand(ids, capacities.read_objects)?;
        let _guard = crate::sqlite::ownership::lock(&self.arbitration)?;
        crate::sqlite::ownership::scope(
            &self.connection,
            0,
            crate::sqlite::ownership::publication(&self.connection)?,
        )?;
        let ceiling = schema::retained_pack_ceiling(&self.connection)?;
        read_objects(
            &self.connection,
            ids,
            ceiling,
            capacities,
            &mut self.workspace,
            &mut self.groups,
            &mut self.pool,
        )
    }
}
