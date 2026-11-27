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
//! Neither the kept identities (32 bytes per [`BLOCK`] objects) nor the bits
//! (two an object) are held whole: both are arrays of the mark's sort space
//! ([`super::paged`]), a fixed number of pages in memory and the rest in an
//! unnamed file. The locate pass of a wave asks for ranks in ascending
//! order, so it reads each page once. Everything else is one window of
//! blocks.
use super::super::spill::sort::Space;
use super::paged::Paged;
use super::{super::spill::records, objects::storage};
use crate::backend::{Mechanics, Ranked};
use layerfs_content::ObjectId;
use layerfs_history::error::{HistoryError, HistoryResult};
use layerfs_storage::{location::ObjectLocation, StorageError};
use std::cmp::Ordering;
use std::sync::Arc;

/// Locators per block: one kept identity, and at most this many rows read to
/// find one identity.
pub(super) const BLOCK: usize = 512;
/// Blocks one read takes when it continues the read before it.
const AHEAD_BLOCKS: usize = 16;

/// The order of two identities by their bytes.
pub(crate) fn by_bytes(left: &ObjectId, right: &ObjectId) -> Ordering {
    left.cmp(right)
}

/// The order of two identities by their first eight bytes as a signed
/// big-endian number, then by their bytes.
pub(crate) fn by_signed_prefix(left: &ObjectId, right: &ObjectId) -> Ordering {
    let prefix = |id: &ObjectId| {
        let mut head = [0u8; 8];
        head.copy_from_slice(&id.as_bytes()[..8]);
        i64::from_be_bytes(head)
    };
    prefix(left).cmp(&prefix(right)).then(left.cmp(right))
}

/// Bytes of kept identities and of bits a mark holds in memory, at most.
const FENCE_BYTES: usize = 4 * 1024 * 1024;
const MARK_BYTES: usize = 8 * 1024 * 1024;

/// What the walk knows about each object, by rank: two bits, whether it was
/// reached as a root or through a logical reference ([`records::LOGICAL`])
/// and whether as a direct delta base ([`records::BASE`]). An object with
/// neither was not reached.
pub(super) struct Marks {
    /// Four ranks a byte.
    kinds: Paged,
}

impl Marks {
    pub(super) fn new(space: &Arc<Space>) -> Self {
        Self {
            kinds: Paged::new(space, MARK_BYTES),
        }
    }

    /// Notes why the object at `rank` was reached, and says whether this is
    /// the first time it is.
    pub(super) fn note(&mut self, rank: u64, kind: u8) -> HistoryResult<bool> {
        let shift = (rank % 4) * 2;
        let kind = kind & (records::LOGICAL | records::BASE);
        let before = self.kinds.raise(rank / 4, kind << shift)?;
        Ok((before >> shift) & 3 == 0 && kind != 0)
    }

    /// Why the object at `rank` was reached; zero when it was not.
    pub(super) fn kinds(&mut self, rank: u64) -> HistoryResult<u8> {
        Ok((self.kinds.byte(rank / 4)? >> ((rank % 4) * 2)) & 3)
    }

    /// Hands every reached rank in `from..to` to `each` with why it was
    /// reached, ascending, until `each` says `false`. A page of unreached
    /// objects is passed over a byte at a time.
    pub(super) fn each(
        &mut self,
        from: u64,
        to: u64,
        mut each: impl FnMut(u64, u8) -> HistoryResult<bool>,
    ) -> HistoryResult<()> {
        let mut rank = from;
        while rank < to {
            let bytes = self.kinds.rest_of_page(rank / 4)?;
            // The ranks of this page that are below `to`, found here and
            // handed over once the page is no longer borrowed.
            let mut found = [(0u64, 0u8); 64];
            let mut count = 0;
            let mut at = rank;
            for byte in bytes {
                if at >= to || count + 4 > found.len() {
                    break;
                }
                if *byte != 0 {
                    for inside in at % 4..4 {
                        let kinds = (*byte >> (inside * 2)) & 3;
                        let rank = at - at % 4 + inside;
                        if kinds != 0 && rank < to {
                            found[count] = (rank, kinds);
                            count += 1;
                        }
                    }
                }
                at = at - at % 4 + 4;
            }
            for (rank, kinds) in &found[..count] {
                if !each(*rank, *kinds)? {
                    return Ok(());
                }
            }
            rank = at;
        }
        Ok(())
    }
}

/// The engine's locators, read a window of blocks at a time.
pub(super) struct Locators {
    order: fn(&ObjectId, &ObjectId) -> Ordering,
    /// The last identity of every full block, 32 bytes each: the identity
    /// of block `b` is the key block `b + 1` is read after.
    after: Paged,
    /// How many full blocks there are.
    full: u64,
    /// The block the last identity was found in: a wave asks in ascending
    /// order, so the next one is most often there or in the block after.
    near: u64,
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
        space: &Arc<Space>,
        mut each: impl FnMut(u64, &ObjectLocation) -> HistoryResult<()>,
    ) -> HistoryResult<Self> {
        let signed = mechanics.locators_by_signed_prefix();
        let mut table = Self {
            order: if signed { by_signed_prefix } else { by_bytes },
            after: Paged::new(space, FENCE_BYTES),
            full: 0,
            near: 0,
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
                    table
                        .after
                        .write(table.full * 32, row.object_id.as_bytes())?;
                    table.full += 1;
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

    /// The last identity of full block `block`.
    fn fence(&mut self, block: u64) -> HistoryResult<ObjectId> {
        if block >= self.full {
            return Err(HistoryError::Integrity("mark object locations"));
        }
        let mut bytes = [0u8; 32];
        self.after.read(block * 32, &mut bytes)?;
        ObjectId::from_bytes(&bytes).map_err(|_| HistoryError::Integrity("mark object locations"))
    }

    /// The block that holds `id` if any does: how many full blocks end
    /// before it.
    fn block_of(&mut self, id: &ObjectId) -> HistoryResult<u64> {
        let order = self.order;
        for block in [self.near, self.near + 1] {
            if block > self.full {
                break;
            }
            let after_start = block == 0 || order(&self.fence(block - 1)?, id) == Ordering::Less;
            let before_end = block == self.full || order(&self.fence(block)?, id) != Ordering::Less;
            if after_start && before_end {
                self.near = block;
                return Ok(block);
            }
        }
        let (mut low, mut high) = (0, self.full);
        while low < high {
            let middle = low + (high - low) / 2;
            if order(&self.fence(middle)?, id) == Ordering::Less {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        self.near = low;
        Ok(low)
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
            block => Some(self.fence(block - 1)?),
        };
        let want = ((blocks * BLOCK) as u64).min(self.count.saturating_sub(start)) as usize;
        self.held.clear();
        self.first = start;
        mechanics
            .locations_after(after.as_ref(), want, &mut self.held)
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
        let block = self.block_of(id)?;
        let start = block * BLOCK as u64;
        if start >= self.count {
            return Ok(None);
        }
        if sparse && !(start >= self.first && start < self.first + self.held.len() as u64) {
            let after = match block {
                0 => None,
                block => Some(self.fence(block - 1)?),
            };
            match mechanics
                .location_after(after.as_ref(), id)
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

    /// Every reached object's locator with its rank and why it was
    /// reached, in ascending byte order of identity.
    pub(super) fn each_set(
        &mut self,
        mechanics: &dyn Mechanics,
        marks: &mut Marks,
        mut each: impl FnMut(u64, &ObjectLocation, u8) -> HistoryResult<()>,
    ) -> HistoryResult<()> {
        let mut reached: Vec<(u64, u8)> = Vec::with_capacity(BLOCK);
        for (from, to) in self.byte_order() {
            let mut block = from / BLOCK as u64;
            while block * (BLOCK as u64) < to {
                let start = block * BLOCK as u64;
                let stop = (start + BLOCK as u64).min(to);
                reached.clear();
                marks.each(start.max(from), stop, |rank, kinds| {
                    reached.push((rank, kinds));
                    Ok(true)
                })?;
                if !reached.is_empty() {
                    self.hold(mechanics, block)?;
                    for (rank, kinds) in &reached {
                        let row = self
                            .held
                            .get((rank - self.first) as usize)
                            .ok_or(HistoryError::Integrity("mark object locations"))?;
                        each(*rank, row, *kinds)?;
                    }
                }
                block += 1;
            }
        }
        Ok(())
    }
}
