//! One combined initial reservation and explicit bounded ordinal refills.
use super::state::State;
use crate::{
    error::{StorageError, StorageResult},
    port::Reserve,
};

impl State<'_> {
    pub(super) fn reserve_initial(&mut self) -> StorageResult<()> {
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
