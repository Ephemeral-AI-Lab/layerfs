//! The Store's own locator index as a mark reads it: in bounded pieces, with
//! one bit per object for what the walk has reached.
//!
//! A mark must answer "where is this identity stored" and "was it reached"
//! for every object of the Store, and may hold neither the locators nor the
//! identities. The engine hands out its locators in one fixed order
//! ([`crate::backend::Mechanics::locations_after`]), so an object's position
//! in that order, its rank, is a dense number: the reached set is a bitmap
//! over ranks. One pass at the start keeps the last identity of every
//! [`BLOCK`] locators. Finding an identity is then a search of those kept
//! identities for its block, one bounded read of that block, and a search
//! inside it; identities asked in order read each block once.
//!
//! What stays in memory for the whole mark: 32 bytes per [`BLOCK`] objects
//! and three bits per object. Everything else is one window of blocks.
use super::storage;
use crate::backend::{Mechanics, Ranked};
use layerfs_content::ObjectId;
use layerfs_history::error::{HistoryError, HistoryResult};
use layerfs_storage::{location::ObjectLocation, StorageError};
use std::cmp::Ordering;

/// Locators per block: one kept identity, and at most this many rows read to
/// find one identity.
pub(super) const BLOCK: usize = 512;
/// Blocks one read takes when it continues the read before it.
const AHEAD_BLOCKS: usize = 16;

/// The order of two identities by their bytes.
pub(super) fn by_bytes(left: &ObjectId, right: &ObjectId) -> Ordering {
    left.cmp(right)
}

/// The order of two identities by their first eight bytes as a signed
/// big-endian number, then by their bytes.
pub(super) fn by_signed_prefix(left: &ObjectId, right: &ObjectId) -> Ordering {
    let prefix = |id: &ObjectId| {
        let mut head = [0u8; 8];
        head.copy_from_slice(&id.as_bytes()[..8]);
        i64::from_be_bytes(head)
    };
    prefix(left).cmp(&prefix(right)).then(left.cmp(right))
}

/// One bit per rank.
#[derive(Default)]
pub(in crate::history) struct Bits(Vec<u64>);

impl Bits {
    pub(super) fn new(count: u64) -> Self {
        Self(vec![0; count.div_ceil(64) as usize])
    }

    pub(super) fn get(&self, rank: u64) -> bool {
        self.0
            .get((rank / 64) as usize)
            .is_some_and(|word| word & (1 << (rank % 64)) != 0)
    }

    /// Sets the bit and says whether it was clear.
    pub(super) fn set(&mut self, rank: u64) -> bool {
        let Some(word) = self.0.get_mut((rank / 64) as usize) else {
            return false;
        };
        let bit = 1 << (rank % 64);
        let clear = *word & bit == 0;
        *word |= bit;
        clear
    }

    /// The set ranks in `from..to`, ascending.
    pub(super) fn ones(&self, from: u64, to: u64) -> impl Iterator<Item = u64> + '_ {
        (from..to).filter(|rank| self.get(*rank))
    }
}

/// What the walk knows about each object, by rank.
pub(super) struct Marks {
    /// Reached: queued for a wave, or read in the visit of its pack.
    pub(super) reached: Bits,
    /// Reached as a root or through a logical reference.
    pub(super) logical: Bits,
    /// Reached as a direct delta base.
    pub(super) bases: Bits,
}

/// The engine's locators, read a window of blocks at a time.
pub(super) struct Locators {
    order: fn(&ObjectId, &ObjectId) -> Ordering,
    /// `after[b]` is the last identity of block `b`: the key block `b + 1`
    /// is read after. Only full blocks have one.
    after: Vec<ObjectId>,
    /// How many locators the Store holds.
    count: u64,
    /// Ranks below this have a negative signed prefix: in byte order they
    /// follow every rank at or above it. Zero when the order is by bytes.
    wrap: u64,
    /// The rank of the first held row, and the rows held.
    first: u64,
    held: Vec<ObjectLocation>,
}

impl Locators {
    /// Reads every locator once, in order, hands each with its rank to
    /// `each` and keeps the block boundaries.
    pub(super) fn load(
        mechanics: &dyn Mechanics,
        mut each: impl FnMut(u64, &ObjectLocation) -> HistoryResult<()>,
    ) -> HistoryResult<Self> {
        let signed = mechanics.locators_by_signed_prefix();
        let mut table = Self {
            order: if signed { by_signed_prefix } else { by_bytes },
            after: Vec::new(),
            count: 0,
            wrap: 0,
            first: 0,
            held: Vec::new(),
        };
        let mut last: Option<ObjectId> = None;
        loop {
            let mut rows = Vec::with_capacity(BLOCK * AHEAD_BLOCKS);
            mechanics
                .locations_after(last.as_ref(), BLOCK * AHEAD_BLOCKS, &mut rows)
                .map_err(StorageError::from)
                .map_err(storage)?;
            for row in &rows {
                if last.is_some_and(|last| (table.order)(&last, &row.object_id) != Ordering::Less) {
                    return Err(HistoryError::Integrity("mark object locations"));
                }
                if signed && row.object_id.as_bytes()[0] >= 0x80 {
                    table.wrap = table.count + 1;
                }
                each(table.count, row)?;
                table.count += 1;
                if table.count % BLOCK as u64 == 0 {
                    table.after.push(row.object_id);
                }
                last = Some(row.object_id);
            }
            if rows.len() < BLOCK * AHEAD_BLOCKS {
                break;
            }
        }
        Ok(table)
    }

    /// How many locators the Store holds.
    pub(super) fn count(&self) -> u64 {
        self.count
    }

    /// The two rank ranges that together list every object in byte order.
    pub(super) fn byte_order(&self) -> [(u64, u64); 2] {
        [(self.wrap, self.count), (0, self.wrap)]
    }

    /// Holds the block `block`, and the blocks after it when this read
    /// continues the one before.
    fn hold(&mut self, mechanics: &dyn Mechanics, block: u64) -> HistoryResult<()> {
        let start = block * BLOCK as u64;
        let end = self.first + self.held.len() as u64;
        if start >= self.first && start < end {
            return Ok(());
        }
        let blocks = if start == end && !self.held.is_empty() {
            AHEAD_BLOCKS
        } else {
            1
        };
        let after = match block {
            0 => None,
            block => Some(
                self.after
                    .get(block as usize - 1)
                    .ok_or(HistoryError::Integrity("mark object locations"))?,
            ),
        };
        let want = ((blocks * BLOCK) as u64).min(self.count.saturating_sub(start)) as usize;
        self.held.clear();
        self.first = start;
        mechanics
            .locations_after(after, want, &mut self.held)
            .map_err(StorageError::from)
            .map_err(storage)?;
        // The same transaction reads the same rows: anything else means the
        // ranks no longer name the objects they named.
        let ordered = self
            .held
            .windows(2)
            .all(|pair| (self.order)(&pair[0].object_id, &pair[1].object_id) == Ordering::Less);
        if self.held.len() != want || !ordered {
            return Err(HistoryError::Integrity("mark object locations"));
        }
        Ok(())
    }

    /// The rank and locator of `id`, or `None` when the Store locates no
    /// such object.
    ///
    /// `sparse` says the identities asked for are few beside the locators:
    /// most blocks hold none of them. One whose block is not held is then
    /// asked of the engine alone ([`Mechanics::location_after`]), where the
    /// engine can: its row and the count of rows before it in its block,
    /// instead of the block's rows.
    pub(super) fn find(
        &mut self,
        mechanics: &dyn Mechanics,
        id: &ObjectId,
        sparse: bool,
    ) -> HistoryResult<Option<(u64, ObjectLocation)>> {
        let order = self.order;
        let block = self
            .after
            .partition_point(|last| order(last, id) == Ordering::Less) as u64;
        let start = block * BLOCK as u64;
        if start >= self.count {
            return Ok(None);
        }
        if sparse && !(start >= self.first && start < self.first + self.held.len() as u64) {
            let after = match block {
                0 => None,
                block => self.after.get(block as usize - 1),
            };
            match mechanics
                .location_after(after, id)
                .map_err(StorageError::from)
                .map_err(storage)?
            {
                Some(Ranked::Absent) => return Ok(None),
                Some(Ranked::Found(before, location)) => {
                    // The same transaction counted the same rows the block
                    // boundaries were taken from.
                    if before >= BLOCK as u64 || start + before >= self.count {
                        return Err(HistoryError::Integrity("mark object locations"));
                    }
                    return Ok(Some((start + before, location)));
                }
                None => {}
            }
        }
        self.hold(mechanics, block)?;
        let from = (block * BLOCK as u64 - self.first) as usize;
        let rows = &self.held[from..(from + BLOCK).min(self.held.len())];
        Ok(rows
            .binary_search_by(|row| order(&row.object_id, id))
            .ok()
            .map(|index| (block * BLOCK as u64 + index as u64, rows[index])))
    }

    /// The locator at `rank`.
    pub(super) fn at(
        &mut self,
        mechanics: &dyn Mechanics,
        rank: u64,
    ) -> HistoryResult<ObjectLocation> {
        self.hold(mechanics, rank / BLOCK as u64)?;
        let block = rank / BLOCK as u64 * BLOCK as u64;
        self.held
            .get((block - self.first) as usize + (rank % BLOCK as u64) as usize)
            .copied()
            .ok_or(HistoryError::Integrity("mark object locations"))
    }

    /// Every locator whose rank is set in `bits`, with its rank, in
    /// ascending byte order of identity.
    pub(super) fn each_set(
        &mut self,
        mechanics: &dyn Mechanics,
        bits: &Bits,
        mut each: impl FnMut(u64, &ObjectLocation) -> HistoryResult<()>,
    ) -> HistoryResult<()> {
        for (from, to) in self.byte_order() {
            let mut block = from / BLOCK as u64;
            while block * (BLOCK as u64) < to {
                let start = block * BLOCK as u64;
                let stop = (start + BLOCK as u64).min(to);
                if bits.ones(start.max(from), stop).next().is_some() {
                    self.hold(mechanics, block)?;
                    for rank in bits.ones(start.max(from), stop) {
                        let row = self
                            .held
                            .get((rank - self.first) as usize)
                            .ok_or(HistoryError::Integrity("mark object locations"))?;
                        each(rank, row)?;
                    }
                }
                block += 1;
            }
        }
        Ok(())
    }
}
