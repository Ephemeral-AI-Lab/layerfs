//! Iterative dependency-chain reconstruction.
//!
//! A DELTA record is decoded only after its whole chain has been read, checked and
//! authenticated, root first. The walk is iterative: no recursion depth depends on
//! the stored chain length, the number of steps is bounded by the role's accepted
//! depth, and the canonical, encoded and record work of one chain are charged
//! against fixed budgets that do not grow with the cutoff or the depth. Legacy sources enforce allocation chronology. The
//! first-wins path permits forward locators and explicitly refuses repeated
//! identities in the chain, because concurrent winners can have higher pack ids.

use crate::source::Source;

use layerfs_content::ObjectId;

use crate::encoding::codec::DecompressionWorkspace;
use crate::encoding::decode::{decode_selected_group, GroupCache};
use crate::encoding::full::raw_payload;
use crate::encoding::pool::PoolReader;
use crate::error::{StorageError, StorageResult};
use crate::location::ObjectLocation;
use crate::pack::layout::PackLane;
use crate::policy::StorageCapacities;

/// Work performed while reconstructing dependency chains.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ChainCounters {
    /// Objects read, including requested objects.
    pub objects: u64,
    /// Chain steps taken, one per dependency edge.
    pub edges: u64,
    /// Digits of packed record bytes read.
    pub encoded_bytes: u64,
    /// Canonical bytes reconstructed.
    pub canonical_bytes: u64,
    /// Longest chain reconstructed.
    pub max_depth: u64,
    /// Ordinary-lane group bodies decompressed while rebuilding these objects.
    ///
    /// One per record that had to decompress its group, so `k` records sharing a
    /// group charge `k` before the decoded-group cache (`P2-4`) and `1` after. It
    /// counts group bodies, not record frames: every other lane decodes a
    /// per-record frame and charges nothing here.
    pub group_decodes: u64,
    /// Pooled metadata reconstruction work, separate from ordinary-lane counts.
    pub pooled: crate::encoding::pool::PoolReadCounters,
}

/// The two stored-body caches one reading caller owns.
///
/// Both bounds are the ones they always were; what this type makes explicit is
/// their **lifetime**, because a cache whose lifetime is one leaf cannot serve the
/// next leaf even when the same bytes are demanded again.
///
/// * `packs` - ordinary-lane pack bodies, bounded by
///   [`crate::policy::DEPENDENCY_PACK_CACHE_BYTES`]. The read wave owns it, so a
///   pack can be reused while retained; capacity eviction can require rereads.
/// * `pool` - the pooled metadata reader: its pack bodies (same bound) and its
///   decoded value groups ([`crate::policy::POOLED_VALUE_CACHE_BYTES`]). The
///   **caller's operation** owns it. A pooled reader built per resolved inode leaf
///   is discarded with the leaf it just served, so a pack a later leaf of the same
///   operation needs is copied from SQLite again; the repetition is measured, and
///   the scope is the treatment.
///
/// **Invalidation contract.** A pack body at or below the ceiling a read was
/// authorized under is immutable: a save writes only packs it created above the
/// baseline publication watermark and publishes them by advancing that watermark,
/// so a committed pack is never rewritten. A reader may therefore retain a body
/// across the waves of one operation, and every pooled consult checks the
/// location's pack against the current ceiling **before** the cache answers
/// (`Resolver::decode_at`, `PoolReader::load_group`). A **writing** owner must
/// instead release the reader's pack cache on every pack write, which
/// `cas::placement::MutationOwner::write_pack` does; decoded values are never
/// released, because an ordinal's value is written once and never moves.
pub struct BodyCaches<'a> {
    /// Ordinary-lane pack bodies this reading caller already read.
    pub packs: &'a mut crate::encoding::PackCache,
    /// Pooled metadata reader this reading caller reconstructs pooled leaves through.
    pub pool: &'a mut PoolReader,
}

/// One read wave's chain resolver: shared body caches, ceiling and counters.
pub struct Resolver<'a> {
    connection: &'a dyn Source,
    ceiling: i64,
    capacities: &'a StorageCapacities,
    caches: BodyCaches<'a>,
    /// Decoded ordinary-lane group bodies this **wave** already materialised.
    ///
    /// Shared across the resolvers a wave builds - one per requested object - so
    /// `k` records of one group cost one decompression instead of `k`. The wave
    /// owns it, so its bound and its lifetime are the wave's, and a fresh wave
    /// starts empty.
    groups: &'a mut GroupCache,
    workspace: &'a mut DecompressionWorkspace,
    counters: &'a mut ChainCounters,
    packs_read: u64,
}

impl<'a> Resolver<'a> {
    /// Builds a resolver over one reading caller's caches.
    pub fn new(
        connection: &'a dyn Source,
        ceiling: i64,
        capacities: &'a StorageCapacities,
        caches: BodyCaches<'a>,
        groups: &'a mut GroupCache,
        workspace: &'a mut DecompressionWorkspace,
        counters: &'a mut ChainCounters,
    ) -> Self {
        Self {
            connection,
            ceiling,
            capacities,
            caches,
            groups,
            workspace,
            counters,
            packs_read: 0,
        }
    }

    /// Pack bodies this resolver fetched from storage.
    ///
    /// Counted where the body is fetched, not inferred from the cache's length:
    /// the cache selectively evicts bodies when it reaches its byte bound, so a
    /// length difference would be wrong exactly when the bound binds.
    pub const fn packs_read(&self) -> u64 {
        self.packs_read
    }

    /// Reconstructs and authenticates one stored canonical object.
    ///
    /// The identity is returned with the bytes: it is the hash this resolution
    /// already computed and checked against the locator, so a caller comparing
    /// identities does not have to hash the same bytes again (P2-6).
    pub fn resolve(&mut self, id: ObjectId) -> StorageResult<(Vec<u8>, ObjectId)> {
        let root = self
            .connection
            .location(id, self.ceiling)?
            .ok_or(StorageError::ObjectMissing(id))?;
        self.resolve_at(root)
    }

    /// Reconstructs one object as a **dependency** of a larger operation.
    ///
    /// The dependency bytes are charged to the chain budget including the object
    /// itself, so a selection can refuse to build a chain that a later read could
    /// not reconstruct.
    pub fn resolve_dependency(&mut self, id: ObjectId) -> StorageResult<(Vec<u8>, ObjectId)> {
        let root = self
            .connection
            .location(id, self.ceiling)?
            .ok_or(StorageError::ObjectMissing(id))?;
        self.resolve_charged(root, true)
    }

    /// Reconstructs one object whose location the caller has already resolved.
    ///
    /// One resolution owns one chain-work allowance: the canonical, encoded and
    /// decoded budgets are charged per chain and restart here, so a wave that
    /// reads many independent objects cannot spend a sibling's allowance and a
    /// single deep chain cannot hide behind a shallow neighbour.
    pub fn resolve_at(&mut self, root: ObjectLocation) -> StorageResult<(Vec<u8>, ObjectId)> {
        self.resolve_charged(root, false)
    }

    fn resolve_charged(
        &mut self,
        root: ObjectLocation,
        charge_self: bool,
    ) -> StorageResult<(Vec<u8>, ObjectId)> {
        *self.counters = ChainCounters::default();
        self.packs_read = 0;
        if root.pack_id > self.ceiling {
            return Err(StorageError::VisibilityCeiling {
                pack_id: root.pack_id,
                ceiling: self.ceiling,
            });
        }
        let id = root.object_id;
        if root.role == layerfs_content::ObjectRole::InodeLeaf {
            // A pooled leaf owns its whole chain: the physical body is rebuilt
            // from pooled COPY/INSERT instructions and the ordinals are resolved
            // through authenticated value groups.
            //
            // The reader is the **caller's**, not one built here: a reader per leaf
            // is discarded with the leaf it served, so the next leaf re-copies every
            // pack this one read. Its counters are cumulative over that wider
            // lifetime, so this chain reports the difference it made.
            let before = self.caches.pool.counters();
            let canonical = self.caches.pool.leaf_canonical_with_groups(
                self.connection,
                self.capacities,
                self.ceiling,
                self.workspace,
                root,
                Some(self.groups),
            )?;
            if ObjectId::for_bytes(&canonical) != id {
                return Err(StorageError::Integrity("pooled leaf identity"));
            }
            self.counters.objects = self.counters.objects.saturating_add(1);
            self.counters.canonical_bytes = self
                .counters
                .canonical_bytes
                .saturating_add(canonical.len() as u64);
            self.counters.pooled = self.caches.pool.counters().since(before);
            return Ok((canonical, id));
        }
        let role_depth = self.capacities.delta_depth_for_role(root.role);
        let mut chain: Vec<ObjectLocation> = Vec::with_capacity(usize::from(role_depth) + 1);
        let mut current = root;
        loop {
            if chain.iter().any(|row| row.object_id == current.object_id) {
                return Err(StorageError::Integrity("dependency cycle"));
            }
            chain.push(current);
            let Some(base) = self.base_of(&current)? else {
                break;
            };
            if chain.len() > usize::from(role_depth) {
                return Err(StorageError::Integrity("dependency chain depth"));
            }
            let location = self
                .connection
                .location(base, self.ceiling)?
                .ok_or(StorageError::ObjectMissing(base))?;
            if location.role != current.role {
                return Err(StorageError::Integrity("dependency role"));
            }
            if self.connection.ordered_dependencies()
                && locator_key(&location) >= locator_key(&current)
            {
                return Err(StorageError::Integrity("dependency chronology"));
            }
            current = location;
        }
        let depth = (chain.len() - 1) as u64;
        self.counters.edges = depth;
        self.counters.max_depth = depth;
        let mut canonical: Option<(Vec<u8>, ObjectId)> = None;
        for (position, location) in chain.iter().rev().enumerate() {
            // Every object except the requested one is a dependency and is charged
            // to the chain budget; the requested object is bounded by the canonical
            // object limit, not by a dependency budget.
            let charged = charge_self || position + 1 != chain.len();
            let decoded = {
                let base_bytes = match &canonical {
                    Some((bytes, _)) => Some(raw_payload(bytes, location.role)?),
                    None => None,
                };
                self.decode_at(location, base_bytes, charged)?
            };
            // The one hash of these bytes: it authenticates the record against
            // the locator that named it, and it is handed back to the caller
            // instead of being recomputed over the same bytes.
            let verified = ObjectId::for_bytes(&decoded);
            if verified != location.object_id {
                return Err(StorageError::Integrity("dependency identity"));
            }
            canonical = Some((decoded, verified));
        }
        canonical.ok_or(StorageError::ObjectMissing(id))
    }

    /// The direct base identity one stored locator names, or `None` when FULL.
    ///
    /// The pack body is read through this resolver's own cache, which the decode
    /// pass below then reuses: a chain is walked and decoded against one copy of
    /// each body, not two.
    fn base_of(&mut self, location: &ObjectLocation) -> StorageResult<Option<ObjectId>> {
        if location.pack_id > self.ceiling {
            return Err(StorageError::VisibilityCeiling {
                pack_id: location.pack_id,
                ceiling: self.ceiling,
            });
        }
        // The body is charged here, where it is fetched, and not inferred later
        // from the cache: the walk now reads it, so a decode that follows is a
        // cache hit and would otherwise report a read that never happened.
        let (_, fetched) = group_of(self.caches.packs, self.connection, location)?;
        if fetched {
            self.packs_read = self.packs_read.saturating_add(1);
        }
        stored_base(
            self.connection,
            self.caches.packs,
            self.groups,
            self.workspace,
            &mut self.counters.group_decodes,
            location,
        )
    }

    fn decode_at(
        &mut self,
        location: &ObjectLocation,
        base: Option<&[u8]>,
        charged: bool,
    ) -> StorageResult<Vec<u8>> {
        self.counters.objects = self.counters.objects.saturating_add(1);
        if charged {
            if location.canonical_length as u64 > self.capacities.chain_canonical_limit {
                return Err(StorageError::Integrity("dependency canonical work"));
            }
            let canonical = self
                .counters
                .canonical_bytes
                .saturating_add(location.canonical_length as u64);
            if canonical > self.capacities.chain_canonical_limit {
                return Err(StorageError::Integrity("dependency canonical work"));
            }
            self.counters.canonical_bytes = canonical;
        }
        // Visibility BEFORE any cache consult: a group whose pack is above this
        // wave's ceiling is refused here, so a body this wave decoded earlier can
        // never answer for a location the ceiling hides. The pooled reader states
        // the same order for its own cache, and the ordinary path must not be the
        // softer door.
        if location.pack_id > self.ceiling {
            return Err(StorageError::VisibilityCeiling {
                pack_id: location.pack_id,
                ceiling: self.ceiling,
            });
        }
        let (selected, fetched) = group_of(self.caches.packs, self.connection, location)?;
        if fetched {
            self.packs_read = self.packs_read.saturating_add(1);
        }
        let record_bytes = selected_record_width(selected, location)?;
        if charged {
            let encoded = self.counters.encoded_bytes.saturating_add(record_bytes);
            if encoded > self.capacities.chain_encoded_limit {
                return Err(StorageError::Integrity("dependency encoded work"));
            }
            self.counters.encoded_bytes = encoded;
        }
        decode_selected_group(
            selected,
            location,
            self.capacities,
            base,
            self.workspace,
            self.groups,
            &mut self.counters.group_decodes,
        )
    }
}

/// A chain walk's record reader over the caller's own pack cache.
///
/// Owner: the operation that walks. Bound: the caller's pack cache is bounded by
/// [`crate::policy::DEPENDENCY_PACK_CACHE_BYTES`] and the decoded-group cache by
/// [`crate::policy::DECODED_GROUP_CACHE_BYTES`]. Live multiplicity: one per
/// walking caller. Lifetime: the caller's. Release: both caches release
/// themselves at owner drop; body pressure evicts selectively and decoded-group
/// pressure keeps its existing clear-all policy.
pub struct ChainBases<'a> {
    packs: &'a mut crate::encoding::PackCache,
    groups: GroupCache,
    group_decodes: u64,
}

impl<'a> ChainBases<'a> {
    /// A walk over `packs`, with its own bounded decoded-group cache.
    pub fn new(packs: &'a mut crate::encoding::PackCache) -> Self {
        Self {
            packs,
            groups: GroupCache::new(),
            group_decodes: 0,
        }
    }

    /// Ordinary-lane group bodies this walk decompressed.
    ///
    /// A walk that is not a read wave has no wave counter to charge, so it keeps
    /// its own: the work is still reported, just by the caller that asked for it.
    pub const fn group_decodes(&self) -> u64 {
        self.group_decodes
    }

    /// The direct base identity `location` names, or `None` when it is FULL.
    pub fn base_of(
        &mut self,
        connection: &dyn Source,
        workspace: &mut DecompressionWorkspace,
        location: &ObjectLocation,
    ) -> StorageResult<Option<ObjectId>> {
        stored_base(
            connection,
            self.packs,
            &mut self.groups,
            workspace,
            &mut self.group_decodes,
            location,
        )
    }
}

/// The direct base identity one stored locator names, read from its own record.
///
/// There is no base column: this is the single reader of a dependency edge. The
/// pack body is fetched through `packs`, so a walk followed by a decode of the
/// same chain pays for each body once. An element whose record cannot be parsed is
/// an integrity failure, never a chain that silently ends.
pub fn stored_base(
    connection: &dyn Source,
    packs: &mut crate::encoding::PackCache,
    groups: &mut GroupCache,
    workspace: &mut DecompressionWorkspace,
    group_decodes: &mut u64,
    location: &ObjectLocation,
) -> StorageResult<Option<ObjectId>> {
    let (selected, _) = group_of(packs, connection, location)?;
    let crate::encoding::GroupSlice {
        header,
        view,
        bytes: selected,
    } = selected;
    match header.lane {
        PackLane::Ordinary => {
            let body: &[u8] = match view.codec {
                crate::pack::layout::GroupCodec::Raw => selected,
                crate::pack::layout::GroupCodec::Zstandard => {
                    match groups.get(location.pack_id, location.group_number) {
                        Some(cached) => cached,
                        None => {
                            let decompressed =
                                workspace.decompress_group(selected, view.decoded_length)?;
                            *group_decodes = group_decodes.saturating_add(1);
                            groups.insert(location.pack_id, location.group_number, decompressed)?;
                            groups
                                .get(location.pack_id, location.group_number)
                                .ok_or(StorageError::Integrity("decoded group cache"))?
                        }
                    }
                }
            };
            if body.len() != view.decoded_length {
                return Err(StorageError::Integrity("group body length"));
            }
            let record = crate::encoding::decode::framed_record(body, location.record_number)?;
            crate::encoding::delta::record::stored_base(
                PackLane::Ordinary,
                record,
                location.canonical_length,
                location.role,
            )
        }
        PackLane::WholeFile => {
            let record = crate::encoding::decode::framed_record(selected, location.record_number)?;
            crate::encoding::delta::record::stored_base(
                PackLane::WholeFile,
                record,
                location.canonical_length,
                location.role,
            )
        }
        PackLane::Native => {
            let record = crate::encoding::decode::framed_record(selected, location.record_number)?;
            crate::encoding::delta::record::stored_base(
                PackLane::Native,
                record,
                location.canonical_length,
                location.role,
            )
        }
        PackLane::Singleton => {
            let record = crate::encoding::decode::framed_record(selected, 0)?;
            crate::encoding::delta::record::stored_base(
                PackLane::Singleton,
                record,
                location.canonical_length,
                location.role,
            )
        }
        PackLane::PooledMetadata => Err(StorageError::Integrity(
            "pooled metadata locator has no object record",
        )),
    }
}

/// Adds one resolved chain's work to an operation's totals.
///
/// A resolver reports the chain it just resolved; an operation that resolves
/// several reports their sum, so a counter named for the save is not quietly the
/// last chain's.
pub fn accumulate(total: &mut ChainCounters, chain: ChainCounters) {
    total.pooled.accumulate(chain.pooled);
    total.objects = total.objects.saturating_add(chain.objects);
    total.edges = total.edges.saturating_add(chain.edges);
    total.encoded_bytes = total.encoded_bytes.saturating_add(chain.encoded_bytes);
    total.canonical_bytes = total.canonical_bytes.saturating_add(chain.canonical_bytes);
    total.max_depth = total.max_depth.max(chain.max_depth);
    total.group_decodes = total.group_decodes.saturating_add(chain.group_decodes);
}

/// Borrows one complete encoded group through the operation's shared cache.
///
/// The cache is bounded by [`DEPENDENCY_PACK_CACHE_BYTES`]: when the next body
/// would exceed the bound, only needed encoded-body/unit victims are evicted.
/// Decoded groups/values keep their separate existing 512 KiB policies.
fn group_of<'b>(
    packs: &'b mut crate::encoding::PackCache,
    connection: &dyn Source,
    location: &ObjectLocation,
) -> StorageResult<(crate::encoding::GroupSlice<'b>, bool)> {
    let (fetched, _) =
        packs.acquire_groups(connection, location.pack_id, &[location.group_number])?;
    let selected = packs
        .group(connection, location.pack_id, location.group_number)?
        .ok_or(StorageError::Integrity("encoded group cache"))?;
    Ok((selected, fetched))
}

/// Strict ordering key of one locator.
pub fn locator_key(location: &ObjectLocation) -> (i64, usize, usize) {
    (
        location.pack_id,
        location.group_number,
        location.record_number,
    )
}

/// Stored width of the record a locator names, charged to the chain budget.
///
/// A compressed group is charged its full decoded body: the decoded bytes are
/// what reconstruction actually materializes, and the compressed extent is
/// already bounded by the pack.
pub fn record_width(pack: &[u8], location: &ObjectLocation) -> StorageResult<u64> {
    let header = crate::pack::layout::parse_header(pack)?;
    let view = crate::pack::layout::group_view(pack, header, location.group_number)?;
    let body = pack
        .get(view.start..view.end)
        .ok_or(StorageError::Integrity("group body range"))?;
    selected_record_width(
        crate::encoding::GroupSlice {
            header,
            view,
            bytes: body,
        },
        location,
    )
}
fn selected_record_width(
    selected: crate::encoding::GroupSlice<'_>,
    location: &ObjectLocation,
) -> StorageResult<u64> {
    match selected.view.codec {
        crate::pack::layout::GroupCodec::Zstandard => Ok(selected.view.decoded_length as u64),
        crate::pack::layout::GroupCodec::Raw => Ok(crate::encoding::decode::framed_record(
            selected.bytes,
            location.record_number,
        )?
        .len() as u64),
    }
}
