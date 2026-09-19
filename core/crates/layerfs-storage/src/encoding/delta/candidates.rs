//! Persisted, bounded, content-keyed index of admitted FULL winners.
//!
//! The index proposes a delta base by **what an object contains**, not by the
//! name it was stored under, so it reaches a predecessor the caller never
//! declared. That is what makes it a second candidate *source* rather than a
//! fallback: the selector consults it only when the caller's declared list
//! acquired nothing (`encoding/delta/select.rs`), and a failed acquisition of a
//! candidate that *was* acquired stays a hard error.
//!
//! Two properties are declared here, and both are measured in the W2 evidence
//! (`docs/roadmap/0.1/0.1.7/evidence/stage-6-history-188d-20260920T000000Z/`):
//!
//! * **Bound.** A fixed power-of-two slot array, a fixed power-of-two reference
//!   table and an eight-entry signature per object. It never grows with the
//!   number of objects admitted, and [`INDEX_BYTES`] is the whole of it.
//! * **Lifetime.** The index belongs to the `Store`, not to one save, and its
//!   rows live in `content_signatures`. A save appends the entries it admitted
//!   inside the transaction that publishes them; `Store::open` reads the table
//!   back. An entry naming an object the Store does not hold is harmless - the
//!   selector probes every proposed candidate against `objects` and refuses an
//!   absent one.
//!
//! **The ring is a bound, never a growth curve, and it is not the binding one.**
//! An object enters only when no candidate was found for it, so the population
//! of the index is self-limiting: the retained-history lane fills it to 7,614 of
//! 8,192 slots and never further, at which point the ring size stops mattering
//! altogether. The measured coverage curve is flat above 8,192 slots.

use rusqlite::Connection;

use layerfs_content::ObjectId;

use crate::error::{StorageError, StorageResult};

/// Live bytes the index may hold.
///
/// **W2 (#188d).** Declared, not derived: 8,192 slots of 68 bytes (a 32-byte
/// identity, a 32-byte folded signature and the `Option` discriminant) plus
/// 65,536 reference entries of 2 bytes is 688,128 B, and the const assertion
/// below proves the whole structure - including the handle itself - fits here.
///
/// **The slot count is the measured knee, not a guess.** The retained-history
/// lane's 44,148 whole-file objects fill the ring to **7,614** entries and stop:
/// coverage is 36,544 objects at 8,192 slots, at 16,384, at 32,768 and unbounded
/// alike, because an object only enters when it found no candidate. The previous
/// value of 32,768 slots held 24,576 slots nothing could ever use.
///
/// **The signature is folded to 32 bits per hash, measured at zero cost.**
/// Coverage is byte-identical to the 64-bit form on all 44,148 objects (36,544
/// either way), and it halves both this bound and the persisted row. A hash
/// collision is a false *proposal*, never a wrong answer: the selector reads the
/// proposed base and keeps whichever representation is smaller, so a bad
/// candidate loses the comparison and stores FULL by policy.
pub const INDEX_BYTES: usize = 704 * 1024;
/// Slot count: the measured coverage knee (see [`INDEX_BYTES`]).
///
/// Bounded by `NO_ENTRY` (`u16`), which is what makes 32,768 the largest
/// expressible power of two; the measured knee is two doublings below it.
const SLOTS: usize = 8_192;
/// Reference-table entries.
///
/// Two per slot would be 16,384; the measured recall curve is still climbing
/// there (29,706 covered objects against 36,544) and has flattened by 65,536
/// (+492 for the last doubling, +184 for the next). 65,536 is the knee, and it
/// is the value this index already carried.
const REFERENCES: usize = 65_536;
const NO_ENTRY: u16 = u16::MAX;
const WINDOW: usize = 16;
const EMPTY: u64 = u64::MAX;
/// Unused-signature marker of the persisted width.
const EMPTY32: u32 = u32::MAX;
/// Persisted width of one signature: eight folded hashes.
const SIGNATURE_BYTES: usize = 32;

#[derive(Clone, Copy)]
struct Entry {
    id: ObjectId,
    signature: [u32; 8],
}

/// Fixed-size, content-keyed, persisted candidate index for one Store.
pub struct Candidates {
    slots: Box<[Option<Entry>]>,
    references: Box<[u16]>,
    /// Slot the next insertion overwrites.
    next: usize,
    /// Insertions accepted so far. The slot of insertion `v` is `(v - 1) % SLOTS`,
    /// which is what lets the persisted rows carry their own order in one
    /// integer and the ring position be recomputed instead of stored.
    stamp: u64,
    /// Highest `stamp` already written to `content_signatures`.
    flushed: u64,
    /// True when the slots do not reflect the persisted table.
    needs_load: bool,
}

/// Reported as its occupancy, never as its slots.
///
/// The ring is 8,192 entries and a derived `Debug` would print every one of them,
/// which is a diagnostic that hides what it is asked to show.
impl std::fmt::Debug for Candidates {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Candidates")
            .field("entries", &self.entries())
            .field("slots", &SLOTS)
            .field("stamp", &self.stamp)
            .field("flushed", &self.flushed)
            .field("needs_load", &self.needs_load)
            .finish()
    }
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
/// index only proposes candidates and the selected bytes decide the outcome.
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

/// The persisted width of one signature: the low 32 bits of each hash.
///
/// The low half and not the high one: the eight retained hashes are the eight
/// *smallest* of a file's rolling hashes, so their high bits are mostly zero and
/// only the low half carries the entropy. `EMPTY` folds onto `EMPTY32`, so an
/// unused entry stays unused.
fn fold(signature: &[u64; 8]) -> [u32; 8] {
    let mut folded = [EMPTY32; 8];
    for (position, hash) in signature.iter().enumerate() {
        folded[position] = *hash as u32;
    }
    folded
}

/// The slot insertion number `stamp` occupies. One-based, so `stamp` 0 means
/// "no insertion yet" and every persisted row carries a positive stamp.
fn slot_of(stamp: u64) -> usize {
    ((stamp - 1) % SLOTS as u64) as usize
}

fn encode(signature: &[u32; 8]) -> [u8; SIGNATURE_BYTES] {
    let mut raw = [0_u8; SIGNATURE_BYTES];
    for (position, hash) in signature.iter().enumerate() {
        raw[position * 4..position * 4 + 4].copy_from_slice(&hash.to_le_bytes());
    }
    raw
}

fn decode(raw: &[u8]) -> StorageResult<[u32; 8]> {
    if raw.len() != SIGNATURE_BYTES {
        return Err(StorageError::Integrity("content index signature width"));
    }
    let mut signature = [EMPTY32; 8];
    for (position, hash) in signature.iter_mut().enumerate() {
        let mut word = [0_u8; 4];
        word.copy_from_slice(&raw[position * 4..position * 4 + 4]);
        *hash = u32::from_le_bytes(word);
    }
    Ok(signature)
}

impl Candidates {
    /// Empty fixed-size index, not yet synchronized with the persisted table.
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
            stamp: 0,
            flushed: 0,
            needs_load: true,
        })
    }

    /// Empty index filled from the persisted table.
    ///
    /// The read is bounded by the table, which is bounded by [`SLOTS`]: at most
    /// 8,192 rows, and a table that holds more is refused rather than truncated.
    pub fn load(connection: &Connection) -> StorageResult<Self> {
        let mut index = Self::new()?;
        index.read(connection)?;
        Ok(index)
    }

    /// Bytes the fixed index holds.
    pub fn live_bytes(&self) -> usize {
        SLOTS * std::mem::size_of::<Option<Entry>>() + REFERENCES * std::mem::size_of::<u16>()
    }

    /// Occupied slots.
    pub fn entries(&self) -> usize {
        self.slots.iter().filter(|slot| slot.is_some()).count()
    }

    /// True when the slots must be read back before the next save may use them.
    pub fn needs_load(&self) -> bool {
        self.needs_load
    }

    /// Drops every entry and re-reads the persisted table on the next use.
    ///
    /// Called when a save fails: the table is authoritative and the slots are
    /// disposable derivation, so invalidation is a reset, never a repair. An
    /// entry this save admitted for an object it never published is dropped here
    /// rather than carried into the next save's flush.
    pub fn invalidate(&mut self) {
        self.clear();
        self.needs_load = true;
    }

    /// Re-reads the persisted table into this index.
    pub fn reload(&mut self, connection: &Connection) -> StorageResult<()> {
        self.clear();
        self.read(connection)
    }

    fn clear(&mut self) {
        self.slots.fill(None);
        self.references.fill(NO_ENTRY);
        self.next = 0;
        self.stamp = 0;
        self.flushed = 0;
    }

    /// Reads the persisted rows into the ring, oldest first.
    ///
    /// The reference table is rebuilt **in stamp order** rather than in slot
    /// order, so a reference slot two entries collide on keeps the same winner a
    /// live insertion sequence would have left there.
    fn read(&mut self, connection: &Connection) -> StorageResult<()> {
        let mut statement = connection
            .prepare("SELECT stamp, object_id, signature FROM content_signatures ORDER BY stamp")?;
        let mut rows = statement.query([])?;
        let mut retained = 0_usize;
        while let Some(row) = rows.next()? {
            if retained >= SLOTS {
                return Err(StorageError::Integrity("content index over slot bound"));
            }
            let stamp = u64::try_from(row.get::<_, i64>(0)?)
                .map_err(|_| StorageError::Integrity("content index stamp range"))?;
            // Stamps are one-based and the query is ordered, so a row that does
            // not advance the running maximum is a table this index did not write.
            if stamp <= self.stamp {
                return Err(StorageError::Integrity("content index stamp order"));
            }
            let id = ObjectId::from_bytes(&row.get::<_, Vec<u8>>(1)?)?;
            let signature = decode(&row.get::<_, Vec<u8>>(2)?)?;
            self.slots[slot_of(stamp)] = Some(Entry { id, signature });
            for hash in signature.iter().copied().filter(|hash| *hash != EMPTY32) {
                self.references[hash as usize & (REFERENCES - 1)] = slot_of(stamp) as u16;
            }
            self.stamp = stamp;
            retained += 1;
        }
        self.next = (self.stamp % SLOTS as u64) as usize;
        self.flushed = self.stamp;
        self.needs_load = false;
        Ok(())
    }

    /// Records one admitted FULL winner.
    pub fn insert(&mut self, id: ObjectId, signature: [u64; 8]) {
        if signature[0] == EMPTY {
            return;
        }
        let folded = fold(&signature);
        let slot = self.next;
        if let Some(old) = self.slots[slot] {
            for hash in old
                .signature
                .iter()
                .copied()
                .filter(|hash| *hash != EMPTY32)
            {
                let reference = &mut self.references[hash as usize & (REFERENCES - 1)];
                if *reference == slot as u16 {
                    *reference = NO_ENTRY;
                }
            }
        }
        self.slots[slot] = Some(Entry {
            id,
            signature: folded,
        });
        for hash in folded.iter().copied().filter(|hash| *hash != EMPTY32) {
            self.references[hash as usize & (REFERENCES - 1)] = slot as u16;
        }
        self.next = (slot + 1) & (SLOTS - 1);
        self.stamp += 1;
    }

    /// Writes every entry admitted since the last flush, and nothing else.
    ///
    /// The rows are keyed by ring slot, so a table row **is** a slot and the ring
    /// bound is enforced by the primary key rather than by a delete pass: no
    /// eviction statement ever runs. The walk covers the insertion numbers
    /// `(flushed, stamp]`, at most [`SLOTS`] of them, so a save that admitted more
    /// objects than the ring can hold writes the ring it actually has.
    ///
    /// The caller owns the transaction: this is a write and it belongs in the
    /// transaction that publishes the objects the entries name.
    pub fn flush(&mut self, connection: &Connection) -> StorageResult<usize> {
        if self.stamp == self.flushed {
            return Ok(0);
        }
        let start = self
            .flushed
            .saturating_add(1)
            .max(self.stamp.saturating_sub(SLOTS as u64 - 1));
        let mut statement = connection.prepare_cached(
            "INSERT OR REPLACE INTO content_signatures (slot, stamp, object_id, signature) \
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        let mut written = 0_usize;
        for value in start..=self.stamp {
            let slot = slot_of(value);
            let entry =
                self.slots[slot].ok_or(StorageError::Integrity("content index flush slot"))?;
            let id = entry.id.to_bytes();
            statement.execute(rusqlite::params![
                slot as i64,
                value as i64,
                &id[..],
                &encode(&entry.signature)[..],
            ])?;
            written += 1;
        }
        self.flushed = self.stamp;
        Ok(written)
    }

    /// Best cached candidate for `signature`, excluding `target_id`.
    ///
    /// The winner is the entry sharing at least two signature hashes with the
    /// highest overlap; ties break on the smaller identity, so the choice is
    /// deterministic and independent of insertion order.
    pub fn find(&self, target_id: ObjectId, signature: &[u64; 8]) -> Option<ObjectId> {
        let query = fold(signature);
        let mut best: Option<(usize, ObjectId)> = None;
        for hash in query.iter().copied().filter(|hash| *hash != EMPTY32) {
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
            let overlap = query
                .iter()
                .filter(|hash| **hash != EMPTY32 && entry.signature.contains(hash))
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
