//! Captured spans of one final file, in order, to the edit script of its base.
//!
//! One forward pass with constant state. A span is *retained* when its bytes
//! are bytes of the file's own committed base at or after the base cursor:
//! inherited bytes, or a reference to that base. Every other span is new
//! bytes. New bytes wait until the next retained span, which closes them and
//! the base bytes it skipped into one edit. Edits are therefore separated by
//! retained bytes and the base cursor only moves forward, as the edit engine
//! requires, and each edit starts at its own offset in the final file.
use super::scan::Origin;
use layerfs_content::{ContentError, ContentResult, Edit};

pub(super) struct Script {
    /// Content root of the file's committed base; none for a new file.
    root: Option<[u8; 32]>,
    base_size: u64,
    /// Next unconsumed offset of the committed base.
    base: u64,
    /// Next offset of the final file.
    at: u64,
    /// New bytes that end at `at` and belong to no edit yet.
    pending: u64,
}
impl Script {
    pub fn new(root: Option<[u8; 32]>, base_size: u64) -> Self {
        Self {
            root,
            base_size,
            base: 0,
            at: 0,
            pending: 0,
        }
    }
    /// The next `length` bytes of the final file. A reference to another
    /// root, and the part of a reference or of inherited bytes that lies
    /// behind the base cursor, cannot be retained: they are new bytes, which
    /// the construction source reads from committed content.
    pub fn span(
        &mut self,
        length: u64,
        origin: Origin,
        emit: &mut dyn FnMut(Edit) -> ContentResult<()>,
    ) -> ContentResult<()> {
        let source = match origin {
            Origin::Inherited => Some(self.at),
            Origin::Reference { root, source } if Some(root) == self.root => Some(source),
            Origin::Reference { .. } | Origin::Local => None,
        };
        let end = self
            .at
            .checked_add(length)
            .ok_or(ContentError::LengthOverflow)?;
        let Some(source) = source else {
            (self.at, self.pending) = (end, self.pending + length);
            return Ok(());
        };
        let limit = source
            .checked_add(length)
            .filter(|limit| *limit <= self.base_size)
            .ok_or(ContentError::InvalidRecord(
                "captured reference beyond its source",
            ))?;
        let behind = self.base.saturating_sub(source).min(length);
        self.pending += behind;
        if behind < length {
            let start = self.at + behind - self.pending;
            let skipped = source + behind - self.base;
            if self.pending != 0 || skipped != 0 {
                emit(Edit::new(start, start + skipped, self.pending))?;
                self.pending = 0;
            }
            self.base = limit;
        }
        self.at = end;
        Ok(())
    }
    /// Closes the new bytes as an overwrite of as many base bytes: the edit
    /// of a file without pieces, whose every byte keeps its offset.
    pub fn overwrite(
        &mut self,
        emit: &mut dyn FnMut(Edit) -> ContentResult<()>,
    ) -> ContentResult<()> {
        if self.pending != 0 {
            let start = self.at - self.pending;
            if self.base != start || self.at > self.base_size {
                return Err(ContentError::InvalidRecord("captured overwrite script"));
            }
            emit(Edit::overwrite(start, self.at))?;
            (self.base, self.pending) = (self.at, 0);
        }
        Ok(())
    }
    /// The end of the final file. Bytes of it the pass did not visit are new
    /// bytes; base bytes not consumed are removed.
    pub fn finish(
        mut self,
        final_size: u64,
        emit: &mut dyn FnMut(Edit) -> ContentResult<()>,
    ) -> ContentResult<()> {
        let rest = final_size
            .checked_sub(self.at)
            .ok_or(ContentError::InvalidRecord("captured script length"))?;
        self.pending += rest;
        if self.pending != 0 || self.base < self.base_size {
            let start = final_size - self.pending;
            emit(Edit::new(
                start,
                start + (self.base_size - self.base),
                self.pending,
            ))?;
        }
        Ok(())
    }
}
