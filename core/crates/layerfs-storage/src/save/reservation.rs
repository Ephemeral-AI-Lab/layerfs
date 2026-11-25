//! One combined initial reservation, explicit bounded refills, and the id
//! block a handle keeps between its Saves.
use super::state::State;
use crate::{
    error::{StorageError, StorageResult},
    port::Reserve,
};

/// The unused rest of a Save's reserved pack ids, and, on a handle that keeps
/// ordinals ([`crate::Storage::keeping_ordinals`]), at most
/// [`crate::policy::KEPT_ORDINALS`] of its unused ordinals (finish releases
/// the tail past them; on any other handle it releases every unused
/// ordinal), kept on the handle when the Save finished, so the next Save on
/// it reserves only when a range runs out (#768). Ids are never reused: a
/// Save that fails, refuses or ends uncertain drops the block (except a
/// held tail whose transaction was refused definitely,
/// [`super::PreparedTail::refused`]: its unused rest is kept), and a handle
/// dropped with it leaks its rest (at most one pack refill and
/// `KEPT_ORDINALS` ordinals). The owner of a keeping handle returns its kept
/// ordinals before an orderly drop ([`crate::Storage::release_kept_ordinals`]);
/// only a crash or an unknown outcome loses them. Gaps are allowed.
///
/// The block records no Store generation: the storage port does not expose
/// one, and none is needed. Neither allocator moves back except by a
/// release that rewinds the ordinal counter only while it still ends this
/// block: a PostgreSQL compaction swaps only the pack and locator tables and
/// abandons when `store_policy` moved after its mark, a SQLite compaction
/// copies both counters, and a write on a handle whose session is fenced to
/// an older generation fails, which drops the block.
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
    /// The unused ordinals, as first ordinal and count, taken out of the
    /// block: they are no longer this block's whether or not their release
    /// is acknowledged.
    pub(crate) fn take_ordinals(&mut self) -> Option<(u32, usize)> {
        let count = self.ordinal_end.checked_sub(self.next_ordinal)?;
        let first = u32::try_from(self.next_ordinal).ok()?;
        self.next_ordinal = self.ordinal_end;
        (count != 0).then_some((first, count as usize))
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
    /// After a definite refusal of the transaction that carried this Save's
    /// held last publication (`release`: that publication's ordinal
    /// release, which was not applied either): every id from the first one
    /// the Save did not assign, up to its reserved ends. Ids the refused
    /// publication assigned stay below it and are never reused.
    pub(super) fn unused_after_refusal(&self, release: Option<(u32, usize)>) -> IdBlock {
        let ordinal_end = release.map_or(self.ordinal_end, |(first, count)| {
            u64::from(first) + count as u64
        });
        IdBlock {
            next_pack: self.next_pack,
            pack_end: self.pack_end,
            next_ordinal: self.next_ordinal,
            ordinal_end: ordinal_end.max(self.next_ordinal),
        }
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
