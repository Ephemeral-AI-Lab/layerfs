//! Pooled reconstruction: value groups and pooled leaf bodies.
//!
//! Reading a pooled leaf is two bounded steps: rebuild its physical body from the
//! delta chain, then resolve every ordinal through its value group. Both steps
//! authenticate what they read - the group body against its catalogue digest and
//! the rebuilt leaf against the identity that was requested - and both charge a
//! per-chain work allowance so a long history cannot hide behind a shallow read.

use std::collections::BTreeMap;

use rusqlite::Connection;

use layerfs_content::inode_leaf::{
    decode_pooled_body, rebuild_leaf, INODE_VALUE_BYTES, MAXIMUM_LEAF_ROWS,
};

use crate::encoding::codec::DecompressionWorkspace;
use crate::encoding::pool::{delta, leaf, value_group};
use crate::error::{StorageError, StorageResult};
use crate::pack::layout::{group_view, parse_header, GroupCodec, PackLane};
use crate::policy::{StorageCapacities, METADATA_DECODED_WORK_LIMIT, METADATA_RECORD_LIMIT};
use crate::sqlite::lookup::{self, ObjectLocation};
use crate::sqlite::pool;

/// Largest decoded value bytes retained across one wave.
const VALUE_CACHE_BYTES: usize = 512 * 1024;

/// One wave's pooled reader: pack and decoded-value caches plus work counters.
#[derive(Debug, Default)]
pub struct PoolReader {
    packs: BTreeMap<i64, Vec<u8>>,
    groups: BTreeMap<u32, Vec<[u8; INODE_VALUE_BYTES]>>,
    retained_bytes: usize,
    decoded_work: u64,
    chain_encoded: u64,
    chain_canonical: u64,
}

impl PoolReader {
    /// Empty reader.
    pub fn new() -> Self {
        Self::default()
    }

    /// Restarts the per-chain decoded-work allowance.
    ///
    /// The decoded-value cache survives: it is bounded independently, and dropping
    /// it would make a sibling target re-read groups it already paid for.
    pub fn begin_chain(&mut self) {
        self.decoded_work = 0;
    }

    /// Decoded value bytes currently retained.
    pub fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    /// Work charged by the current chain.
    pub fn decoded_work(&self) -> u64 {
        self.decoded_work
    }

    /// Encoded bytes of the chain the last reconstruction read.
    ///
    /// This is the reader's own charge, so a producer that has to decide whether a
    /// dependent still fits the chain budget can use the same number the read of
    /// that dependent will charge instead of a worst-case per-record estimate.
    pub fn chain_encoded_bytes(&self) -> u64 {
        self.chain_encoded
    }

    /// Every value of one authenticated group, in ordinal order.
    pub fn group_values(
        &mut self,
        connection: &Connection,
        capacities: &StorageCapacities,
        ceiling: i64,
        workspace: &mut DecompressionWorkspace,
        row: &pool::ValueGroupRow,
    ) -> StorageResult<Vec<[u8; INODE_VALUE_BYTES]>> {
        self.load_group(connection, capacities, ceiling, workspace, row)?;
        self.groups
            .get(&row.first_ordinal)
            .cloned()
            .ok_or(StorageError::Integrity("metadata group cache"))
    }

    /// One value of one authenticated group, copied out of the wave's cache.
    ///
    /// A caller that resolves the ordinals of one leaf needs a single value per
    /// row; this accessor decodes the covering group once and copies seventy-three
    /// bytes, instead of materialising the whole group per row.
    pub fn group_value(
        &mut self,
        connection: &Connection,
        capacities: &StorageCapacities,
        ceiling: i64,
        workspace: &mut DecompressionWorkspace,
        row: &pool::ValueGroupRow,
        ordinal: u32,
    ) -> StorageResult<[u8; INODE_VALUE_BYTES]> {
        if ordinal < row.first_ordinal
            || u64::from(ordinal) >= u64::from(row.first_ordinal) + row.count as u64
        {
            return Err(StorageError::Integrity("metadata ordinal range"));
        }
        self.load_group(connection, capacities, ceiling, workspace, row)?;
        self.groups
            .get(&row.first_ordinal)
            .and_then(|values| values.get((ordinal - row.first_ordinal) as usize))
            .copied()
            .ok_or(StorageError::Integrity("metadata ordinal range"))
    }

    /// Decodes one group into the wave's cache unless it is already there.
    fn load_group(
        &mut self,
        connection: &Connection,
        capacities: &StorageCapacities,
        ceiling: i64,
        workspace: &mut DecompressionWorkspace,
        row: &pool::ValueGroupRow,
    ) -> StorageResult<()> {
        let _ = capacities;
        // The ceiling is decided before the cache is consulted: a group retained
        // by an earlier read of the same wave is still only readable when its own
        // pack is at or below the wave's captured ceiling.
        if row.pack_id > ceiling {
            return Err(StorageError::VisibilityCeiling {
                pack_id: row.pack_id,
                ceiling,
            });
        }
        if self.groups.contains_key(&row.first_ordinal) {
            return Ok(());
        }
        let body = self.group_body(connection, workspace, row)?;
        self.decoded_work = self
            .decoded_work
            .saturating_add(body.len() as u64 + row.count as u64 * INODE_VALUE_BYTES as u64);
        if self.decoded_work > METADATA_DECODED_WORK_LIMIT {
            return Err(StorageError::Integrity("metadata decoded work"));
        }
        value_group::authenticate(&body, row.digest)?;
        let canonical = value_group::decode(&body, row.count)?;
        let mut values = Vec::with_capacity(canonical.len());
        for value in canonical {
            values.push(layerfs_content::inode_leaf::decode_pooled_value(&value)?);
        }
        let charged = values.len() * INODE_VALUE_BYTES;
        if self.retained_bytes + charged > VALUE_CACHE_BYTES {
            self.groups.clear();
            self.retained_bytes = 0;
        }
        self.groups.insert(row.first_ordinal, values);
        self.retained_bytes += charged;
        Ok(())
    }

    fn group_body(
        &mut self,
        connection: &Connection,
        workspace: &mut DecompressionWorkspace,
        row: &pool::ValueGroupRow,
    ) -> StorageResult<Vec<u8>> {
        let pack = self.pack(connection, row.pack_id)?;
        let header = parse_header(pack)?;
        if header.lane != PackLane::PooledMetadata {
            return Err(StorageError::Integrity("value group lane"));
        }
        let view = group_view(pack, header, row.group_number)?;
        let selected = pack
            .get(view.start..view.end)
            .ok_or(StorageError::Integrity("group body range"))?;
        let body = match view.codec {
            GroupCodec::Raw => selected.to_vec(),
            GroupCodec::Zstandard => workspace.decompress_group(selected, view.decoded_length)?,
        };
        if body.len() != view.decoded_length {
            return Err(StorageError::Integrity("value group body length"));
        }
        Ok(body)
    }

    /// Reads one pack body through this wave's cache, bounded as the dependency
    /// cache is: the cache is released wholesale when the next body would cross
    /// the declared bound, so a wave's retained pack bytes are a constant rather
    /// than a function of how many packs it reads.
    fn pack(&mut self, connection: &Connection, pack_id: i64) -> StorageResult<&[u8]> {
        if !self.packs.contains_key(&pack_id) {
            let bytes = lookup::pack_bytes(connection, pack_id)?;
            let retained: usize = self.packs.values().map(Vec::len).sum();
            if retained.saturating_add(bytes.len()) > crate::policy::DEPENDENCY_PACK_CACHE_BYTES {
                self.packs.clear();
            }
            self.packs.insert(pack_id, bytes);
        }
        self.packs
            .get(&pack_id)
            .map(Vec::as_slice)
            .ok_or(StorageError::Integrity("pack cache"))
    }

    /// Rebuilds the physical body of one pooled leaf from its delta chain.
    pub fn leaf_body(
        &mut self,
        connection: &Connection,
        capacities: &StorageCapacities,
        ceiling: i64,
        workspace: &mut DecompressionWorkspace,
        root: ObjectLocation,
    ) -> StorageResult<Vec<u8>> {
        if root.role != layerfs_content::ObjectRole::InodeLeaf {
            return Err(StorageError::Integrity("pooled record role"));
        }
        self.chain_encoded = 0;
        self.chain_canonical = 0;
        let mut chain: Vec<ObjectLocation> =
            Vec::with_capacity(usize::from(capacities.metadata_delta_max_depth) + 1);
        let mut current = root;
        loop {
            chain.push(current);
            let Some(base) = current.base_object_id else {
                break;
            };
            if chain.len() > usize::from(capacities.metadata_delta_max_depth) {
                return Err(StorageError::Integrity("pooled chain depth"));
            }
            let location = lookup::location(connection, base, ceiling)?
                .ok_or(StorageError::ObjectMissing(base))?;
            if location.role != root.role {
                return Err(StorageError::Integrity("pooled chain role"));
            }
            if (
                location.pack_id,
                location.group_number,
                location.record_number,
            ) >= (current.pack_id, current.group_number, current.record_number)
            {
                return Err(StorageError::Integrity("pooled chain chronology"));
            }
            current = location;
        }
        let mut body: Option<Vec<u8>> = None;
        let mut canonical_work = 0_u64;
        let mut encoded_work = 0_u64;
        for location in chain.iter().rev() {
            let record = self.record(connection, workspace, location)?;
            canonical_work = canonical_work.saturating_add(location.canonical_length as u64);
            encoded_work = encoded_work.saturating_add(record.len() as u64);
            if canonical_work > capacities.metadata_chain_canonical_limit
                || encoded_work > capacities.metadata_chain_encoded_limit
            {
                return Err(StorageError::Integrity("pooled chain work"));
            }
            self.chain_canonical = canonical_work;
            self.chain_encoded = encoded_work;
            body = Some(match leaf::parse(&record)? {
                leaf::PooledRecord::Full(stored) => stored.to_vec(),
                leaf::PooledRecord::Delta {
                    base,
                    output_length,
                    count,
                    instructions,
                } => {
                    let base_body = body
                        .as_ref()
                        .ok_or(StorageError::Integrity("pooled delta without base"))?;
                    let expected_base = base;
                    let base_id = location
                        .base_object_id
                        .ok_or(StorageError::Integrity("pooled delta base"))?;
                    if expected_base != base_id {
                        return Err(StorageError::Integrity("pooled delta base identity"));
                    }
                    if output_length != leaf::physical_length(location.canonical_length)? {
                        return Err(StorageError::Integrity("pooled delta length"));
                    }
                    delta::apply(base_body, instructions, count, output_length)?
                }
            });
        }
        body.ok_or(StorageError::Integrity("pooled chain empty"))
    }

    fn record(
        &mut self,
        connection: &Connection,
        workspace: &mut DecompressionWorkspace,
        location: &ObjectLocation,
    ) -> StorageResult<Vec<u8>> {
        if location.canonical_length > crate::policy::INODE_LEAF_LIMIT {
            return Err(StorageError::Integrity("inode leaf length"));
        }
        let pack = self.pack(connection, location.pack_id)?;
        let header = parse_header(pack)?;
        if header.lane != PackLane::Ordinary {
            return Err(StorageError::Integrity("pooled leaf lane"));
        }
        let view = group_view(pack, header, location.group_number)?;
        let selected = pack
            .get(view.start..view.end)
            .ok_or(StorageError::Integrity("group body range"))?;
        let body = match view.codec {
            GroupCodec::Raw => selected.to_vec(),
            GroupCodec::Zstandard => workspace.decompress_group(selected, view.decoded_length)?,
        };
        if body.len() != view.decoded_length {
            return Err(StorageError::Integrity("group body length"));
        }
        let record = crate::encoding::decode::framed_record(&body, location.record_number)?;
        if record.len() > METADATA_RECORD_LIMIT {
            return Err(StorageError::Integrity("pooled record limit"));
        }
        Ok(record.to_vec())
    }

    /// Rebuilds the canonical leaf of one pooled locator.
    pub fn leaf_canonical(
        &mut self,
        connection: &Connection,
        capacities: &StorageCapacities,
        ceiling: i64,
        workspace: &mut DecompressionWorkspace,
        root: ObjectLocation,
    ) -> StorageResult<Vec<u8>> {
        self.begin_chain();
        let body = self.leaf_body(connection, capacities, ceiling, workspace, root)?;
        let (prefix, rows) = decode_pooled_body(&body)?;
        if rows.len() > MAXIMUM_LEAF_ROWS {
            return Err(StorageError::Integrity("pooled row count"));
        }
        let mut values = Vec::with_capacity(rows.len());
        // The rows of one leaf are in ordinal order, so the group covering a row is
        // almost always the group that covered the row before it: the catalogue is
        // queried when the covering group changes, not once per row.
        let mut covering: Option<pool::ValueGroupRow> = None;
        for row in &rows {
            let group = match covering {
                Some(group)
                    if row.ordinal >= group.first_ordinal
                        && u64::from(row.ordinal)
                            < u64::from(group.first_ordinal) + group.count as u64 =>
                {
                    group
                }
                _ => {
                    let group = pool::group_for(connection, row.ordinal)?
                        .ok_or(StorageError::Integrity("metadata ordinal missing"))?;
                    covering = Some(group);
                    group
                }
            };
            values.push(self.group_value(
                connection,
                capacities,
                ceiling,
                workspace,
                &group,
                row.ordinal,
            )?);
        }
        let canonical = rebuild_leaf(&prefix, &rows, &values)?;
        if canonical.len() != root.canonical_length {
            return Err(StorageError::Integrity("pooled canonical length"));
        }
        Ok(canonical)
    }
}
