//! Bytes one mark addresses by position and may not hold whole: what it
//! knows about every object by rank, and the identity that ends each block
//! of locators.
//!
//! Both are arrays as long as the Store has objects. A [`Paged`] array keeps
//! a fixed number of [`PAGE`]-byte pages in memory and the rest in an
//! unnamed file of the mark's sort space ([`Space`]): a page is read when it
//! is asked for and written back, if it changed, when another page takes its
//! place. A page lives in the one slot its number selects, so an array that
//! fits the slots never touches the file, a pass in ascending order reads
//! and writes each page once, and nothing is searched or aged.
//!
//! A page that was never written reads as zeros, in the file (a hole) or
//! past its end.
use super::super::spill::sort::{io_error, Space};
use layerfs_history::error::{HistoryError, HistoryResult};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::Arc;

/// Bytes of one page.
pub(crate) const PAGE: usize = 4096;

/// One page held in memory.
struct Slot {
    page: u64,
    bytes: Box<[u8; PAGE]>,
    /// Whether the bytes differ from the file's.
    changed: bool,
}

/// An array of bytes, zero until written, held a fixed number of pages at a
/// time. See the module comment.
pub(crate) struct Paged {
    space: Arc<Space>,
    file: Option<File>,
    /// The file's length: no page at or past it was ever written.
    stored: u64,
    /// `slots[page % slots.len()]` is the only place page `page` is held.
    slots: Vec<Option<Slot>>,
    /// How often a page was asked for.
    asked: u64,
}

impl Paged {
    /// An array that holds at most about `bytes` of its pages in memory
    /// under [`super::spill::BUDGET_BYTES`], that share of the space's own
    /// budget, and at least one page.
    pub(crate) fn new(space: &Arc<Space>, bytes: usize) -> Self {
        let mut slots = Vec::new();
        slots.resize_with((space.scaled(bytes) / PAGE).max(1), || None);
        Self {
            space: space.clone(),
            file: None,
            stored: 0,
            slots,
            asked: 0,
        }
    }

    /// The slot holding `page`, after making it the page its slot holds.
    fn hold(&mut self, page: u64) -> HistoryResult<&mut Slot> {
        self.asked += 1;
        let at = (page % self.slots.len() as u64) as usize;
        if self.slots[at]
            .as_ref()
            .is_some_and(|slot| slot.page == page)
        {
            return self.slots[at]
                .as_mut()
                .ok_or(HistoryError::Integrity("mark page"));
        }
        self.space.missed();
        // The page that was here goes back to the file if it changed; its
        // memory is the new page's.
        let mut slot = match self.slots[at].take() {
            Some(slot) => {
                if slot.changed {
                    self.store(&slot)?;
                }
                slot
            }
            None => Slot {
                page,
                bytes: Box::new([0; PAGE]),
                changed: false,
            },
        };
        slot.page = page;
        slot.changed = false;
        let start = page
            .checked_mul(PAGE as u64)
            .ok_or(HistoryError::Integrity("mark page"))?;
        if start < self.stored {
            let Some(mut file) = self.file.as_ref() else {
                return Err(HistoryError::Integrity("mark page file"));
            };
            file.seek(SeekFrom::Start(start)).map_err(io_error)?;
            file.read_exact(&mut slot.bytes[..]).map_err(io_error)?;
            self.space.paged_in();
        } else {
            slot.bytes.fill(0);
        }
        Ok(self.slots[at].insert(slot))
    }

    /// Writes one page to the file.
    fn store(&mut self, slot: &Slot) -> HistoryResult<()> {
        if self.file.is_none() {
            self.file = Some(self.space.file()?);
        }
        let Some(mut file) = self.file.as_ref() else {
            return Err(HistoryError::Integrity("mark page file"));
        };
        let start = slot.page * PAGE as u64;
        file.seek(SeekFrom::Start(start)).map_err(io_error)?;
        file.write_all(&slot.bytes[..]).map_err(io_error)?;
        self.space.paged_out();
        let end = start + PAGE as u64;
        if end > self.stored {
            self.space.grow(end - self.stored);
            self.stored = end;
        }
        Ok(())
    }

    /// The byte at `at`.
    pub(crate) fn byte(&mut self, at: u64) -> HistoryResult<u8> {
        let slot = self.hold(at / PAGE as u64)?;
        Ok(slot.bytes[(at % PAGE as u64) as usize])
    }

    /// Sets the bits of `bits` in the byte at `at` and returns the byte as
    /// it was.
    pub(crate) fn raise(&mut self, at: u64, bits: u8) -> HistoryResult<u8> {
        let slot = self.hold(at / PAGE as u64)?;
        let byte = &mut slot.bytes[(at % PAGE as u64) as usize];
        let before = *byte;
        if before | bits != before {
            *byte = before | bits;
            slot.changed = true;
        }
        Ok(before)
    }

    /// Copies the bytes from `at` on into `out`.
    pub(crate) fn read(&mut self, mut at: u64, mut out: &mut [u8]) -> HistoryResult<()> {
        while !out.is_empty() {
            let from = (at % PAGE as u64) as usize;
            let length = out.len().min(PAGE - from);
            let slot = self.hold(at / PAGE as u64)?;
            out[..length].copy_from_slice(&slot.bytes[from..from + length]);
            out = &mut out[length..];
            at += length as u64;
        }
        Ok(())
    }

    /// Writes `bytes` from `at` on.
    pub(crate) fn write(&mut self, mut at: u64, mut bytes: &[u8]) -> HistoryResult<()> {
        while !bytes.is_empty() {
            let from = (at % PAGE as u64) as usize;
            let length = bytes.len().min(PAGE - from);
            let slot = self.hold(at / PAGE as u64)?;
            slot.bytes[from..from + length].copy_from_slice(&bytes[..length]);
            slot.changed = true;
            bytes = &bytes[length..];
            at += length as u64;
        }
        Ok(())
    }

    /// The bytes of the page holding `at`, from `at` to the page's end.
    pub(crate) fn rest_of_page(&mut self, at: u64) -> HistoryResult<&[u8]> {
        let slot = self.hold(at / PAGE as u64)?;
        Ok(&slot.bytes[(at % PAGE as u64) as usize..])
    }
}

impl Drop for Paged {
    fn drop(&mut self) {
        self.space.shrink(self.stored);
        self.space.asked(self.asked);
    }
}
