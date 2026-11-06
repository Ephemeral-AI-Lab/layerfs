//! Pooled reconstruction: pooled leaf bodies and their stored inode values.
//!
//! Reading a pooled leaf is two bounded steps: rebuild its physical body from the
//! delta chain, then resolve the body's ordinals to their stored values with one
//! lookup. What authenticates both is the last step: the rebuilt canonical leaf,
//! which contains every value, must have the identity that was requested. A
//! wrong chain record or a wrong value row can therefore only fail the read; it
//! can never be returned. The chain charges a work allowance so a long history
//! cannot hide behind a shallow read.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use crate::source::Source;

use layerfs_content::inode_leaf::{
    decode_pooled_body, rebuild_leaf, INODE_VALUE_BYTES, MAXIMUM_LEAF_ROWS,
};
use layerfs_content::ObjectId;

use crate::encoding::codec::DecompressionWorkspace;
use crate::encoding::pool::{delta, leaf, PoolReadCounters};
use crate::encoding::GroupCache;
use crate::error::{StorageError, StorageResult};
use crate::location::ObjectLocation;
use crate::pack::layout::{GroupCodec, PackLane};
use crate::policy::{StorageCapacities, METADATA_RECORD_LIMIT, POOLED_VALUE_CACHE_BYTES};

/// Existing owner caches borrowed together for immutable ordinary groups.
pub(crate) struct PooledCaches<'a> {
    pub(crate) encoded: &'a mut crate::encoding::PackCache,
    pub(crate) decoded: &'a mut GroupCache,
}

/// One wave's pooled reader: pack and stored-value caches plus work counters.
#[derive(Debug, Default)]
pub struct PoolReader {
    packs: crate::encoding::PackCache,
    /// Stored values by ordinal. A value is written once under its ordinal and
    /// never changes, so an entry stays valid for the reader's lifetime.
    values: BTreeMap<u32, [u8; INODE_VALUE_BYTES]>,
    chain_encoded: u64,
    chain_canonical: u64,
    /// Chains rebuilt ahead by [`Self::prepare_leaves`], by root, whose
    /// ordinals all have their values in `values`; taken by the leaf's own
    /// reconstruction. At most one window ([`PREPARED_LEAVES`]).
    prepared: BTreeMap<ObjectId, Rebuilt>,
    pub(super) counters: PoolReadCounters,
}

/// One pooled chain rebuilt to physical bodies, root last, with the work it
/// charged.
#[derive(Debug)]
struct Rebuilt {
    chain: Vec<ObjectLocation>,
    bodies: Vec<Vec<u8>>,
    root: Option<ObjectId>,
    canonical_work: u64,
    encoded_work: u64,
}

/// Stored values the reader retains at most.
const VALUE_ROOM: usize = POOLED_VALUE_CACHE_BYTES / INODE_VALUE_BYTES;

/// Leaves one [`PoolReader::prepare_leaves`] window rebuilds ahead at most.
pub(crate) const PREPARED_LEAVES: usize = 128;

/// Physical body bytes one window keeps at most (each chain is itself
/// bounded by its canonical work allowance, 64 KiB).
const PREPARED_BYTES: usize = 1024 * 1024;

impl PoolReader {
    /// Empty reader.
    pub fn new() -> Self {
        Self::default()
    }

    /// Actual work accumulated over this reader's lifetime.
    pub fn counters(&self) -> PoolReadCounters {
        self.counters
    }

    /// Stored value bytes currently retained.
    pub fn retained_bytes(&self) -> usize {
        self.values.len() * INODE_VALUE_BYTES
    }

    /// Encoded bytes of the chain the last reconstruction read.
    ///
    /// This is the reader's own charge, so a producer that has to decide whether a
    /// dependent still fits the chain budget can use the same number the read of
    /// that dependent will charge instead of a worst-case per-record estimate.
    pub fn chain_encoded_bytes(&self) -> u64 {
        self.chain_encoded
    }

    /// The stored value of every one of these distinct ordinals, in the order
    /// asked: retained values first, then one lookup for all the rest. The
    /// retained set is bounded; when new values would cross the bound it is
    /// dropped whole before they are kept.
    fn stored_values(
        &mut self,
        connection: &dyn Source,
        ordinals: &[u32],
    ) -> StorageResult<Vec<[u8; INODE_VALUE_BYTES]>> {
        let missing: Vec<u32> = ordinals
            .iter()
            .copied()
            .filter(|ordinal| !self.values.contains_key(ordinal))
            .collect();
        self.counters.value_cache_hits = self
            .counters
            .value_cache_hits
            .saturating_add((ordinals.len() - missing.len()) as u64);
        let mut fetched = BTreeMap::new();
        if !missing.is_empty() {
            let found = connection.inode_values(&missing)?;
            if found.len() != missing.len() {
                return Err(StorageError::Integrity("metadata value count"));
            }
            self.counters.value_lookups = self.counters.value_lookups.saturating_add(1);
            self.counters.value_rows = self.counters.value_rows.saturating_add(found.len() as u64);
            fetched.extend(missing.iter().copied().zip(found));
        }
        let values = ordinals
            .iter()
            .map(|ordinal| {
                self.values
                    .get(ordinal)
                    .or_else(|| fetched.get(ordinal))
                    .copied()
                    .ok_or(StorageError::Integrity("metadata ordinal missing"))
            })
            .collect::<StorageResult<Vec<_>>>()?;
        if (self.values.len() + fetched.len()) * INODE_VALUE_BYTES > POOLED_VALUE_CACHE_BYTES {
            self.counters.value_cache_evictions = self
                .counters
                .value_cache_evictions
                .saturating_add(self.values.len() as u64);
            self.values.clear();
        }
        if fetched.len() * INODE_VALUE_BYTES <= POOLED_VALUE_CACHE_BYTES {
            self.values.extend(fetched);
        }
        Ok(values)
    }

    /// Walks one pooled leaf's chain and rebuilds its physical bodies, root
    /// last, charging the chain's work allowance.
    fn rebuild(
        &mut self,
        connection: &dyn Source,
        capacities: &StorageCapacities,
        ceiling: i64,
        workspace: &mut DecompressionWorkspace,
        root: ObjectLocation,
        mut caches: Option<PooledCaches<'_>>,
    ) -> StorageResult<Rebuilt> {
        let mut chain: Vec<ObjectLocation> =
            Vec::with_capacity(usize::from(capacities.metadata_delta_max_depth) + 1);
        let mut current = root;
        loop {
            if current.pack_id > ceiling {
                return Err(StorageError::VisibilityCeiling {
                    pack_id: current.pack_id,
                    ceiling,
                });
            }
            if chain.iter().any(|row| row.object_id == current.object_id) {
                return Err(StorageError::Integrity("pooled dependency cycle"));
            }
            chain.push(current);
            let record = self.record(connection, workspace, &current, caches.as_mut())?;
            let Some(base) = pooled_base(&record)? else {
                break;
            };
            if chain.len() > usize::from(capacities.metadata_delta_max_depth) {
                return Err(StorageError::Integrity("pooled chain depth"));
            }
            let location = connection
                .base_location(base, ceiling)?
                .ok_or(StorageError::ObjectMissing(base))?;
            if location.role != root.role {
                return Err(StorageError::Integrity("pooled chain role"));
            }
            if connection.ordered_dependencies()
                && (
                    location.pack_id,
                    location.group_number,
                    location.record_number,
                ) >= (current.pack_id, current.group_number, current.record_number)
            {
                return Err(StorageError::Integrity("pooled chain chronology"));
            }
            self.counters.chain_edges = self.counters.chain_edges.saturating_add(1);
            current = location;
        }
        let mut bodies: Vec<Vec<u8>> = Vec::with_capacity(chain.len());
        // Identity of the element decoded immediately before this one, which is
        // this element's base whenever the record says it has one. The record's
        // own base identity is checked against it, so the edge the walk followed
        // and the edge the record states must be the same edge.
        let mut decoded_id: Option<ObjectId> = None;
        let mut canonical_work = 0_u64;
        let mut encoded_work = 0_u64;
        for location in chain.iter().rev() {
            let record = self.record(connection, workspace, location, caches.as_mut())?;
            canonical_work = canonical_work.saturating_add(location.canonical_length as u64);
            encoded_work = encoded_work.saturating_add(record.len() as u64);
            if canonical_work > capacities.metadata_chain_canonical_limit
                || encoded_work > capacities.metadata_chain_encoded_limit
            {
                return Err(StorageError::Integrity("pooled chain work"));
            }
            let body = match leaf::parse(&record)? {
                leaf::PooledRecord::Full(stored) => stored.to_vec(),
                leaf::PooledRecord::Delta {
                    base,
                    output_length,
                    count,
                    instructions,
                } => {
                    let base_body = bodies
                        .last()
                        .ok_or(StorageError::Integrity("pooled delta without base"))?;
                    let base_id = decoded_id.ok_or(StorageError::Integrity("pooled delta base"))?;
                    if base != base_id {
                        return Err(StorageError::Integrity("pooled delta base identity"));
                    }
                    if output_length != leaf::physical_length(location.canonical_length)? {
                        return Err(StorageError::Integrity("pooled delta length"));
                    }
                    delta::apply(base_body, instructions, count, output_length)?
                }
            };
            bodies.push(body);
            decoded_id = Some(location.object_id);
        }
        Ok(Rebuilt {
            chain,
            bodies,
            root: decoded_id,
            canonical_work,
            encoded_work,
        })
    }

    /// Drops every chain rebuilt ahead (a demand that failed may leave some).
    pub(crate) fn forget_prepared(&mut self) {
        self.prepared.clear();
    }

    /// Whether [`Self::prepare_leaves`] rebuilt `id` and it is not read yet.
    pub(crate) fn is_prepared(&self, id: &ObjectId) -> bool {
        self.prepared.contains_key(id)
    }

    /// Rebuilds ahead the pooled leaves among `roots`, in order, from the first
    /// not yet prepared, and looks up the stored values all of them need in
    /// one lookup (#95): a demand of many leaves then costs one value lookup
    /// per window instead of one per leaf. A window ends before the leaf
    /// whose values would pass what the reader retains ([`VALUE_ROOM`]) or
    /// whose bodies would pass [`PREPARED_BYTES`], or at [`PREPARED_LEAVES`]
    /// leaves; every one of its values is kept until
    /// those leaves are read. Every leaf in the window is one the caller's
    /// demand reads, so a failure here is that demand's failure, returned
    /// once and never attempted again. Nothing here is authenticated: each
    /// leaf's reconstruction still checks its canonical identity.
    pub(crate) fn prepare_leaves(
        &mut self,
        connection: &dyn Source,
        capacities: &StorageCapacities,
        ceiling: i64,
        workspace: &mut DecompressionWorkspace,
        roots: &[ObjectLocation],
        mut caches: Option<PooledCaches<'_>>,
    ) -> StorageResult<()> {
        self.prepared.clear();
        let mut wanted = BTreeSet::new();
        let mut bytes = 0usize;
        for root in roots {
            if self.prepared.len() == PREPARED_LEAVES {
                break;
            }
            if root.role != layerfs_content::ObjectRole::InodeLeaf
                || self.prepared.contains_key(&root.object_id)
            {
                continue;
            }
            let caches = caches.as_mut().map(|caches| PooledCaches {
                encoded: &mut *caches.encoded,
                decoded: &mut *caches.decoded,
            });
            let rebuilt =
                self.rebuild(connection, capacities, ceiling, workspace, *root, caches)?;
            let ordinals = body_ordinals(&rebuilt.bodies)?;
            let more: Vec<u32> = ordinals.difference(&wanted).copied().collect();
            let size: usize = rebuilt.bodies.iter().map(Vec::len).sum();
            if !self.prepared.is_empty()
                && (wanted.len() + more.len() > VALUE_ROOM || bytes + size > PREPARED_BYTES)
            {
                break;
            }
            wanted.extend(more);
            bytes += size;
            self.prepared.insert(root.object_id, rebuilt);
        }
        if self.prepared.len() < 2 {
            // One leaf looks up its own values, as before.
            return Ok(());
        }
        let missing: Vec<u32> = wanted
            .iter()
            .copied()
            .filter(|ordinal| !self.values.contains_key(ordinal))
            .collect();
        if missing.is_empty() {
            return Ok(());
        }
        if self.values.len() + missing.len() > VALUE_ROOM {
            // Dropped whole, as `stored_values` does, then the window's values
            // are fetched together.
            self.counters.value_cache_evictions = self
                .counters
                .value_cache_evictions
                .saturating_add(self.values.len() as u64);
            self.values.clear();
        }
        let missing: Vec<u32> = wanted
            .iter()
            .copied()
            .filter(|ordinal| !self.values.contains_key(ordinal))
            .collect();
        let found = connection.inode_values(&missing)?;
        if found.len() != missing.len() {
            return Err(StorageError::Integrity("metadata value count"));
        }
        self.counters.value_lookups = self.counters.value_lookups.saturating_add(1);
        self.counters.value_rows = self.counters.value_rows.saturating_add(found.len() as u64);
        self.values.extend(missing.into_iter().zip(found));
        Ok(())
    }

    /// The physical body and the authenticated canonical bytes of one pooled
    /// locator, for a Save that reuses the ordinals of the leaf's values.
    pub(crate) fn leaf_parts_with_caches(
        &mut self,
        connection: &dyn Source,
        capacities: &StorageCapacities,
        ceiling: i64,
        workspace: &mut DecompressionWorkspace,
        root: ObjectLocation,
        caches: Option<PooledCaches<'_>>,
    ) -> StorageResult<(Vec<u8>, Vec<u8>)> {
        self.reconstruct_leaf(connection, capacities, ceiling, workspace, root, caches)
    }

    fn reconstruct_leaf(
        &mut self,
        connection: &dyn Source,
        capacities: &StorageCapacities,
        ceiling: i64,
        workspace: &mut DecompressionWorkspace,
        root: ObjectLocation,
        caches: Option<PooledCaches<'_>>,
    ) -> StorageResult<(Vec<u8>, Vec<u8>)> {
        if root.role != layerfs_content::ObjectRole::InodeLeaf {
            return Err(StorageError::Integrity("pooled record role"));
        }
        self.chain_encoded = 0;
        self.chain_canonical = 0;
        let rebuilt = match self.prepared.remove(&root.object_id) {
            Some(rebuilt) if rebuilt.chain.first() == Some(&root) => rebuilt,
            _ => self.rebuild(connection, capacities, ceiling, workspace, root, caches)?,
        };
        self.chain_canonical = rebuilt.canonical_work;
        self.chain_encoded = rebuilt.encoded_work;
        let Rebuilt {
            chain,
            mut bodies,
            root: decoded_id,
            ..
        } = rebuilt;
        // One lookup resolves the ordinals of every body of the chain. Each
        // element is then rebuilt to canonical bytes and checked against its
        // own identity, the root last: a valid dependent cannot hide a wrong
        // base, and a wrong stored value can only fail the read.
        let ordinals: Vec<u32> = body_ordinals(&bodies)?.into_iter().collect();
        let values: BTreeMap<u32, [u8; INODE_VALUE_BYTES]> = ordinals
            .iter()
            .copied()
            .zip(self.stored_values(connection, &ordinals)?)
            .collect();
        let mut canonical = None;
        for (location, body) in chain.iter().rev().zip(&bodies) {
            canonical = Some(canonical_from_body(*location, body, &values)?);
        }
        if decoded_id != Some(root.object_id) {
            return Err(StorageError::Integrity("pooled canonical root"));
        }
        Ok((
            bodies
                .pop()
                .ok_or(StorageError::Integrity("pooled chain empty"))?,
            canonical.ok_or(StorageError::Integrity("pooled canonical root"))?,
        ))
    }

    /// The direct base identity one stored pooled locator names.
    ///
    /// `None` when the leaf is stored in full. The writer's depth walk reads a
    /// chain's edges without decoding it, and does so through the same pack cache
    /// the acquisition that follows uses, so an edge costs no second fetch.
    pub fn stored_base(
        &mut self,
        connection: &dyn Source,
        workspace: &mut DecompressionWorkspace,
        location: &ObjectLocation,
    ) -> StorageResult<Option<ObjectId>> {
        self.stored_base_with_caches(connection, workspace, location, None)
    }
    pub(crate) fn stored_base_with_caches(
        &mut self,
        connection: &dyn Source,
        workspace: &mut DecompressionWorkspace,
        location: &ObjectLocation,
        mut caches: Option<PooledCaches<'_>>,
    ) -> StorageResult<Option<ObjectId>> {
        let record = self.record(connection, workspace, location, caches.as_mut())?;
        pooled_base(&record)
    }

    pub(super) fn record(
        &mut self,
        connection: &dyn Source,
        workspace: &mut DecompressionWorkspace,
        location: &ObjectLocation,
        caches: Option<&mut PooledCaches<'_>>,
    ) -> StorageResult<Vec<u8>> {
        self.counters.physical_record_calls = self.counters.physical_record_calls.saturating_add(1);
        if location.canonical_length > crate::policy::INODE_LEAF_LIMIT {
            return Err(StorageError::Integrity("inode leaf length"));
        }
        let mut work = PoolReadCounters::default();
        let result = (|| {
            let (packs, groups) = match caches {
                Some(caches) => (&mut *caches.encoded, Some(&mut *caches.decoded)),
                None => (&mut self.packs, None),
            };
            let (fetched, bytes) =
                packs.acquire_groups(connection, location.pack_id, &[location.group_number])?;
            if fetched {
                self.counters.pack_fetches = self.counters.pack_fetches.saturating_add(1);
                self.counters.pack_bytes = self.counters.pack_bytes.saturating_add(bytes as u64);
            }
            let selected = packs
                .group(connection, location.pack_id, location.group_number)?
                .ok_or(StorageError::Integrity("encoded pooled group cache"))?;
            let crate::encoding::GroupSlice {
                header,
                view,
                bytes: selected,
            } = selected;
            if header.lane != PackLane::Ordinary {
                return Err(StorageError::Integrity("pooled leaf lane"));
            }
            let body: Cow<'_, [u8]> = match view.codec {
                GroupCodec::Raw => Cow::Owned(selected.to_vec()),
                GroupCodec::Zstandard => match groups {
                    Some(groups) => {
                        if groups
                            .get(location.pack_id, location.group_number)
                            .is_some()
                        {
                            work.physical_group_cache_hits = 1;
                        } else {
                            let decoded =
                                workspace.decompress_group(selected, view.decoded_length)?;
                            work.physical_group_decodes = 1;
                            work.physical_group_decoded_bytes = decoded.len() as u64;
                            groups.insert(location.pack_id, location.group_number, decoded)?;
                        }
                        Cow::Borrowed(
                            groups
                                .get(location.pack_id, location.group_number)
                                .ok_or(StorageError::Integrity("decoded group cache"))?,
                        )
                    }
                    None => {
                        let decoded = workspace.decompress_group(selected, view.decoded_length)?;
                        work.physical_group_decodes = 1;
                        work.physical_group_decoded_bytes = decoded.len() as u64;
                        Cow::Owned(decoded)
                    }
                },
            };
            if body.len() != view.decoded_length {
                return Err(StorageError::Integrity("group body length"));
            }
            let record = crate::encoding::decode::framed_record(&body, location.record_number)?;
            if record.len() > METADATA_RECORD_LIMIT {
                return Err(StorageError::Integrity("pooled record limit"));
            }
            Ok(record.to_vec())
        })();
        self.counters.accumulate(work);
        result
    }

    /// Shares the owner's immutable ordinary encoded and decoded groups.
    /// Mutable pooled-value input stays reader-local and is invalidated after writes.
    pub(crate) fn leaf_canonical_with_caches(
        &mut self,
        connection: &dyn Source,
        capacities: &StorageCapacities,
        ceiling: i64,
        workspace: &mut DecompressionWorkspace,
        root: ObjectLocation,
        caches: Option<PooledCaches<'_>>,
    ) -> StorageResult<Vec<u8>> {
        if caches.is_some() && root.pack_id > ceiling {
            return Err(StorageError::VisibilityCeiling {
                pack_id: root.pack_id,
                ceiling,
            });
        }
        self.counters.leaf_requests = self.counters.leaf_requests.saturating_add(1);
        self.reconstruct_leaf(connection, capacities, ceiling, workspace, root, caches)
            .map(|(_, canonical)| canonical)
    }
}
/// Canonical bytes of one pooled body over already fetched values, checked
/// against the identity its locator states.
fn canonical_from_body(
    root: ObjectLocation,
    body: &[u8],
    stored: &BTreeMap<u32, [u8; INODE_VALUE_BYTES]>,
) -> StorageResult<Vec<u8>> {
    let (prefix, rows) = decode_pooled_body(body)?;
    let values = rows
        .iter()
        .map(|row| {
            stored
                .get(&row.ordinal)
                .copied()
                .ok_or(StorageError::Integrity("metadata ordinal missing"))
        })
        .collect::<StorageResult<Vec<_>>>()?;
    let canonical = rebuild_leaf(&prefix, &rows, &values)?;
    if canonical.len() != root.canonical_length {
        return Err(StorageError::Integrity("pooled canonical length"));
    }
    if ObjectId::for_bytes(&canonical) != root.object_id {
        return Err(StorageError::Integrity("pooled canonical identity"));
    }
    Ok(canonical)
}
/// The direct base identity one pooled leaf record names, or `None` when FULL.
///
/// A pooled leaf keeps its base in the record (`pool::leaf::PooledRecord::Delta`),
/// which is the same fact the ordinary lanes keep in theirs, so one reader serves
/// both: an unparsable record is an integrity failure, not a chain that ends.
fn pooled_base(record: &[u8]) -> StorageResult<Option<ObjectId>> {
    Ok(match leaf::parse(record)? {
        leaf::PooledRecord::Full(_) => None,
        leaf::PooledRecord::Delta { base, .. } => Some(base),
    })
}

/// Every ordinal the pooled bodies name.
fn body_ordinals(bodies: &[Vec<u8>]) -> StorageResult<BTreeSet<u32>> {
    let mut ordinals = BTreeSet::new();
    for body in bodies {
        let (_, rows) = decode_pooled_body(body)?;
        if rows.len() > MAXIMUM_LEAF_ROWS {
            return Err(StorageError::Integrity("pooled row count"));
        }
        ordinals.extend(rows.iter().map(|row| row.ordinal));
    }
    Ok(ordinals)
}
