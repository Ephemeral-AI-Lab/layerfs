//! Batched object reads and the retained-pack visibility ceiling.
//!
//! One read captures its permitted pack ceiling once and applies it to every
//! acquired location; the ceiling never moves while the read is in progress. Pack
//! bodies are read once per pack per wave, and every reconstructed object is
//! authenticated against the identity that was requested.

use std::collections::BTreeMap;

use rusqlite::Connection;

use layerfs_content::ObjectId;

use crate::encoding::DecompressionWorkspace;
use crate::error::{StorageError, StorageResult};
use crate::sqlite::lookup;

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
}

/// Reads every requested object in demand order under one ceiling.
pub fn read_objects(
    connection: &Connection,
    ids: &[ObjectId],
    ceiling: i64,
    workspace: &mut DecompressionWorkspace,
) -> StorageResult<(Vec<Vec<u8>>, ReadCounters)> {
    let locations = lookup::locations(connection, ids, i64::MAX)?;
    let mut counters = ReadCounters {
        ceiling,
        pages: ids.len().div_ceil(crate::policy::LOOKUP_PAGE_IDS) as u64,
        ..ReadCounters::default()
    };
    let mut by_id: BTreeMap<ObjectId, lookup::ObjectLocation> = BTreeMap::new();
    for location in locations {
        if location.pack_id > ceiling {
            return Err(StorageError::VisibilityCeiling {
                pack_id: location.pack_id,
                ceiling,
            });
        }
        by_id.insert(location.object_id, location);
    }
    let mut packs: BTreeMap<i64, Vec<u8>> = BTreeMap::new();
    let mut values = Vec::with_capacity(ids.len());
    for id in ids {
        let location = by_id
            .get(id)
            .copied()
            .ok_or(StorageError::ObjectMissing(*id))?;
        if let std::collections::btree_map::Entry::Vacant(slot) = packs.entry(location.pack_id) {
            let bytes = lookup::pack_bytes(connection, location.pack_id)?;
            counters.packs_read += 1;
            slot.insert(bytes);
        }
        let pack = packs
            .get(&location.pack_id)
            .ok_or(StorageError::Integrity("pack cache"))?;
        let canonical = crate::encoding::decode_canonical(
            pack,
            location.group_number,
            location.record_number,
            location.canonical_length,
            workspace,
        )?;
        if ObjectId::for_bytes(&canonical) != *id {
            return Err(StorageError::Integrity("read identity"));
        }
        counters.objects += 1;
        values.push(canonical);
    }
    Ok((values, counters))
}
