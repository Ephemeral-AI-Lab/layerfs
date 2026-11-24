//! One combined initial reservation, explicit bounded refills, and the id
//! block a handle keeps between its Saves.
use super::state::State;
use crate::{
    error::{StorageError, StorageResult},
    port::Reserve,
};

/// The unused rest of a Save's reserved pack ids, and at most
/// [`crate::policy::KEPT_ORDINALS`] of its unused ordinals (finish releases
/// the tail past them), kept on the handle when the Save finished, so the
/// next Save on it reserves only when a range runs out (#768). Ids are never
/// reused: a Save that fails, refuses or ends uncertain drops the block, and
/// a dropped handle leaks its rest (at most one pack refill and
/// `KEPT_ORDINALS` ordinals). Gaps are allowed.
///
/// The block records no Store generation: the storage port does not expose
/// one, and none is needed. Neither allocator moves back: a PostgreSQL
/// compaction swaps only the pack and locator tables and abandons when
/// `store_policy` moved after its mark, a SQLite compaction copies both
/// counters, and a write on a handle whose session is fenced to an older
/// generation fails, which drops the block.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct IdBlock {
    next_pack: i64,
    pack_end: i64,
    next_ordinal: u64,
    ordinal_end: u64,
}
impl IdBlock {
    fn is_empty(&self) -> bool {
        self.next_pack >= self.pack_end && self.next_ordinal >= self.ordinal_end
    }
}

impl State<'_> {
    /// Takes the handle's kept block, or reserves both kinds at once.
    pub(super) fn reserve_initial(&mut self) -> StorageResult<()> {
        if let Some(kept) = self.storage.kept.take().filter(|b| !b.is_empty()) {
            self.next_pack = kept.next_pack;
            self.pack_end = kept.pack_end;
            self.next_ordinal = kept.next_ordinal;
            self.ordinal_end = kept.ordinal_end;
            return Ok(());
        }
        let blocks = self.storage.reservations;
        self.storage.source.note(|c| {
            c.reserve += 1;
            c.initial_reservations += 1;
            c.ordinal_reservations += 1;
        });
        let _work = self.storage.work.span(super::Stage::PackReserve);
        let allocation = self.storage.source.metadata.reserve(Reserve {
            packs: blocks.packs,
            ordinals: blocks.ordinals,
        })?;
        if allocation.first_pack_id <= 0 || allocation.first_ordinal == 0 {
            return Err(StorageError::Integrity("initial Save reservation"));
        }
        self.next_pack = allocation.first_pack_id;
        self.pack_end = self
            .next_pack
            .checked_add(blocks.packs as i64)
            .ok_or(StorageError::Integrity("pack allocation overflow"))?;
        self.ordinal_range(allocation.first_ordinal, blocks.ordinals)
    }
    /// After an acknowledged finish: the unused pack ids and the kept ordinal
    /// prefix go back to the handle for its next Save.
    pub(super) fn keep_rest(&self) {
        self.storage.kept.set(Some(IdBlock {
            next_pack: self.next_pack,
            pack_end: self.pack_end,
            next_ordinal: self.next_ordinal,
            ordinal_end: self.ordinal_end,
        }));
    }
    pub(super) fn reserve_ordinals(&mut self, block: usize) -> StorageResult<()> {
        self.storage.source.note(|c| {
            c.reserve += 1;
            c.reservation_refills += 1;
            c.ordinal_reservations += 1;
        });
        let _work = self.storage.work.span(super::Stage::OrdinalReserve);
        let allocation = self.storage.source.metadata.reserve(Reserve {
            packs: 0,
            ordinals: block,
        })?;
        self.ordinal_range(allocation.first_ordinal, block)
    }
    fn ordinal_range(&mut self, first: u32, count: usize) -> StorageResult<()> {
        if first == 0 {
            return Err(StorageError::Integrity("metadata ordinal reservation"));
        }
        self.next_ordinal = u64::from(first);
        self.ordinal_end = self
            .next_ordinal
            .checked_add(count as u64)
            .filter(|end| *end <= u64::from(u32::MAX) + 1)
            .ok_or(StorageError::Integrity("metadata ordinal maximum"))?;
        Ok(())
    }
}
