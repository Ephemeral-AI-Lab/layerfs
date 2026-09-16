//! Bounded admitted-FULL winner cache used when no explicit predecessor applies.
//!
//! Only representations actually stored as FULL enter this cache: a DELTA record
//! is not a useful prefix anchor. The cache is owned by one save operation, so it
//! never outlives the operation that filled it and a failed save cannot leave a
//! partly advanced cache behind. The structure is fixed-size: a power-of-two slot
//! array, a power-of-two reference table and an eight-entry signature per object,
//! with no growth with the number of admitted objects.

use layerfs_content::ObjectId;

use crate::error::{StorageError, StorageResult};

/// Live bytes the cache may hold.
pub const INDEX_BYTES: usize = 128 * 1024;
const SLOTS: usize = 1024;
const REFERENCES: usize = 8_192;
const NO_ENTRY: u16 = u16::MAX;
const WINDOW: usize = 16;
const EMPTY: u64 = u64::MAX;

#[derive(Clone, Copy)]
struct Entry {
    id: ObjectId,
    signature: [u64; 8],
}

/// Fixed-size, content-keyed prefix candidate cache for one save operation.
pub struct Candidates {
    slots: Box<[Option<Entry>]>,
    references: Box<[u16]>,
    next: usize,
}

const _: () = {
    assert!(SLOTS.is_power_of_two());
    assert!(REFERENCES.is_power_of_two() && SLOTS < NO_ENTRY as usize);
    assert!(
        SLOTS * std::mem::size_of::<Option<Entry>>()
            + REFERENCES * std::mem::size_of::<u16>()
            + std::mem::size_of::<Option<Candidates>>()
            <= INDEX_BYTES
    );
};

fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

/// Eight smallest distinct mixed rolling hashes of `raw`, without allocation.
///
/// `EMPTY` marks an unused signature slot; that one hash value is deliberately
/// omitted, which is a bounded candidate loss, never a correctness risk: the
/// cache only proposes candidates and the selected bytes decide the outcome.
pub fn signature(raw: &[u8]) -> [u64; 8] {
    let mut result = [EMPTY; 8];
    if raw.len() < WINDOW {
        return result;
    }
    let high = 257_u64.wrapping_pow((WINDOW - 1) as u32);
    let mut rolling = raw[..WINDOW].iter().fold(0_u64, |hash, byte| {
        hash.wrapping_mul(257) + u64::from(*byte)
    });
    for start in 0..=raw.len() - WINDOW {
        if start != 0 {
            rolling = rolling
                .wrapping_sub(u64::from(raw[start - 1]).wrapping_mul(high))
                .wrapping_mul(257)
                .wrapping_add(u64::from(raw[start + WINDOW - 1]));
        }
        let hash = mix(rolling);
        if hash < result[7] && !result.contains(&hash) {
            let index = result.partition_point(|value| *value < hash);
            result.copy_within(index..7, index + 1);
            result[index] = hash;
        }
    }
    result
}

impl Candidates {
    /// Empty fixed-size cache.
    pub fn new() -> StorageResult<Self> {
        let mut slots: Vec<Option<Entry>> = Vec::new();
        slots
            .try_reserve_exact(SLOTS)
            .map_err(|_| StorageError::Integrity("candidate index allocation"))?;
        slots.resize(SLOTS, None);
        let mut references: Vec<u16> = Vec::new();
        references
            .try_reserve_exact(REFERENCES)
            .map_err(|_| StorageError::Integrity("candidate index allocation"))?;
        references.resize(REFERENCES, NO_ENTRY);
        Ok(Self {
            slots: slots.into_boxed_slice(),
            references: references.into_boxed_slice(),
            next: 0,
        })
    }

    /// Bytes charged by the fixed cache.
    pub fn live_bytes(&self) -> usize {
        std::mem::size_of_val(&self.slots) + std::mem::size_of_val(&self.references)
    }

    /// Records one admitted FULL winner.
    pub fn insert(&mut self, id: ObjectId, signature: [u64; 8]) {
        if signature[0] == EMPTY {
            return;
        }
        let slot = self.next;
        if let Some(old) = self.slots[slot] {
            for hash in old.signature.iter().copied().filter(|hash| *hash != EMPTY) {
                let reference = &mut self.references[hash as usize & (REFERENCES - 1)];
                if *reference == slot as u16 {
                    *reference = NO_ENTRY;
                }
            }
        }
        self.slots[slot] = Some(Entry { id, signature });
        for hash in signature.iter().copied().filter(|hash| *hash != EMPTY) {
            self.references[hash as usize & (REFERENCES - 1)] = slot as u16;
        }
        self.next = (slot + 1) & (SLOTS - 1);
    }

    /// Best cached candidate for `signature`, excluding `target_id`.
    ///
    /// The winner is the entry sharing at least two signature hashes with the
    /// highest overlap; ties break on the smaller identity, so the choice is
    /// deterministic and independent of insertion order.
    pub fn find(&self, target_id: ObjectId, signature: &[u64; 8]) -> Option<ObjectId> {
        let mut best: Option<(usize, ObjectId)> = None;
        for hash in signature.iter().copied().filter(|hash| *hash != EMPTY) {
            let reference = self.references[hash as usize & (REFERENCES - 1)];
            if reference == NO_ENTRY {
                continue;
            }
            let Some(entry) = self.slots[reference as usize] else {
                continue;
            };
            if entry.id == target_id || !entry.signature.contains(&hash) {
                continue;
            }
            let overlap = signature
                .iter()
                .filter(|hash| **hash != EMPTY && entry.signature.contains(hash))
                .count();
            if overlap >= 2
                && best.is_none_or(|(count, id)| {
                    overlap > count || (overlap == count && entry.id < id)
                })
            {
                best = Some((overlap, entry.id));
            }
        }
        best.map(|(_, id)| id)
    }
}
