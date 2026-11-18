//! The stored length of every pack of the sealed Store, for the §6.4 decision
//! and for the windows of the pack copy and of the verification.
//!
//! The run reads the lengths once, inside the mark's transaction, where the
//! walk has just read the pages they are on; reading them again for the copy
//! would visit one scattered table page per pack a second time. They may not
//! be held as one entry per pack either, so they go to a list of bounded
//! memory ([`crate::history::spill`]): up to [`HELD_BYTES`] of them stay in
//! memory and the rest is in an unnamed file beside the Store. They arrive
//! and are read back in pack order.

use crate::history::spill::Item;
use crate::history::spill::Merge;
use crate::history::spill::Sorter;
use crate::history::spill::Space;
use crate::Sequence;
use layerfs_history::error::HistoryError;
use layerfs_history::error::HistoryResult;
use std::cmp::Ordering;
use std::sync::Arc;

/// Bytes of pack lengths held in memory: 65,536 packs.
const HELD_BYTES: usize = 1024 * 1024;

/// One pack and its stored length.
#[derive(Clone, Copy)]
struct PackLength {
    pack: i64,
    length: u64,
}

impl Item for PackLength {
    const BYTES: usize = 16;
    fn encode(&self, out: &mut [u8]) {
        out[..8].copy_from_slice(&self.pack.to_le_bytes());
        out[8..16].copy_from_slice(&self.length.to_le_bytes());
    }
    fn decode(bytes: &[u8]) -> Option<Self> {
        Some(Self {
            pack: i64::from_le_bytes(bytes[..8].try_into().ok()?),
            length: u64::from_le_bytes(bytes[8..16].try_into().ok()?),
        })
    }
}

fn by_pack(left: &PackLength, right: &PackLength) -> Ordering {
    left.pack.cmp(&right.pack)
}

/// Every pack of the sealed Store and its stored length, in pack order.
pub struct PackLengths {
    list: Sorter<PackLength>,
    total: u64,
}

impl PackLengths {
    /// An empty list whose file, when it needs one, is in `space`: the
    /// run's one space for sort files, which counts it.
    pub fn new(space: &Arc<Space>) -> Self {
        Self {
            list: Sorter::new(space, HELD_BYTES, by_pack),
            total: 0,
        }
    }

    /// Adds one pack.
    pub fn push(&mut self, pack: i64, length: u64) -> HistoryResult<()> {
        self.total = self.total.saturating_add(length);
        self.list.push(PackLength { pack, length })
    }

    /// The stored bytes of every pack.
    pub fn total(&self) -> u64 {
        self.total
    }

    /// Every pack and its length, ascending.
    pub fn packs(&mut self) -> HistoryResult<Packs<'_>> {
        Ok(Packs(self.list.sorted()?))
    }

    /// The stored bytes of the `retained` packs. A retained pack the Store
    /// does not hold is a mark that disagrees with it.
    pub fn stored(&mut self, retained: &Sequence) -> HistoryResult<u64> {
        let mut packs = self.packs()?;
        let mut stored = 0u64;
        retained.each(&mut |wanted| loop {
            match packs.next().transpose()? {
                Some((pack, _)) if u64::try_from(pack).is_ok_and(|pack| pack < wanted) => {}
                Some((pack, length)) if u64::try_from(pack) == Ok(wanted) => {
                    stored = stored.saturating_add(length);
                    return Ok(());
                }
                _ => return Err(HistoryError::Integrity("mark pack")),
            }
        })?;
        Ok(stored)
    }
}

/// The packs of a [`PackLengths`], ascending.
pub struct Packs<'a>(Merge<'a, PackLength>);

impl Iterator for Packs<'_> {
    type Item = HistoryResult<(i64, u64)>;
    fn next(&mut self) -> Option<Self::Item> {
        self.0
            .next()
            .map(|next| next.map(|next| (next.pack, next.length)))
            .transpose()
    }
}
