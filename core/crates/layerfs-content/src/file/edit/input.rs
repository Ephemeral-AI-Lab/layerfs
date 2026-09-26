//! Checked ordered edit stream, its bounded replacement source and applicability.
//!
//! Edits are expressed in **current-result coordinates**: each edit addresses the
//! result of every earlier edit in the same stream. The stream is validated once,
//! before any work: an inverted range, an overlap, a coordinate beyond the current
//! result or a final length that does not accumulate is rejected. Segments are
//! never reordered or merged - CDC segmentation and exact roots depend on the edit
//! boundaries the caller declared.
//!
//! One applicability rule is declared and enforced: an edit may address the
//! original base content and the gaps between earlier edits, but it may not reach
//! back into bytes an **earlier edit in the same stream introduced**. Such a range
//! is rejected with `InvalidEdit { what: "range inside an earlier replacement" }`
//! rather than silently reordered or merged. The rule is what makes one streaming
//! pass possible: bytes already emitted to the bounded consumer can never change.
//!
//! The sequence is reached only through [`EditSequence`] and its replacement bytes
//! only through [`EditSource`]. There is no resident-first convenience sequence:
//! a stream that must be replayed from a bounded spool supplies its own
//! implementation, so no type here fixes the run count into resident memory.

use std::io::Read;

use crate::error::{ContentError, ContentResult};

/// One replacement range in current-result coordinates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Edit {
    start: u64,
    end: u64,
    replacement_len: u64,
}

impl Edit {
    /// Builds an edit replacing `start..end` with `replacement_len` bytes.
    pub const fn new(start: u64, end: u64, replacement_len: u64) -> Self {
        Self {
            start,
            end,
            replacement_len,
        }
    }

    /// Builds an overwrite of `start..end` by the same number of bytes.
    pub const fn overwrite(start: u64, end: u64) -> Self {
        Self::new(start, end, end - start)
    }

    /// Builds an insertion at `at`.
    pub const fn insert(at: u64, replacement_len: u64) -> Self {
        Self::new(at, at, replacement_len)
    }

    /// Builds a deletion of `start..end`.
    pub const fn delete(start: u64, end: u64) -> Self {
        Self::new(start, end, 0)
    }

    /// First replaced offset in current-result coordinates.
    pub const fn start(self) -> u64 {
        self.start
    }

    /// Exclusive end of the replaced range in current-result coordinates.
    pub const fn end(self) -> u64 {
        self.end
    }

    /// Declared replacement length.
    pub const fn replacement_len(self) -> u64 {
        self.replacement_len
    }

    /// Bytes this edit removes.
    pub const fn removed_len(self) -> u64 {
        self.end - self.start
    }

    /// Length of the result after this edit, given the length before it.
    pub fn apply_len(self, before: u64) -> ContentResult<u64> {
        match before
            .checked_sub(self.removed_len())
            .and_then(|kept| kept.checked_add(self.replacement_len))
        {
            Some(after) => Ok(after),
            None => Err(ContentError::LengthOverflow),
        }
    }
}

/// Validated replayable final-change sequence. Implementations may keep fixed
/// records on disk; construction never needs the sequence resident as a vector.
pub trait EditSequence {
    /// Length of the authenticated base file.
    fn base_len(&self) -> u64;
    /// Length after all validated replacements.
    fn final_len(&self) -> u64;
    /// Number of final replacement runs.
    fn len(&self) -> usize;
    /// Reads one validated run by index from bounded backing.
    fn edit_at(&self, index: usize) -> ContentResult<Edit>;
    /// Whether the sequence retains the base unchanged.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Bounded byte source for replacement ranges.
///
/// Implementations are ordinary callers, not product hooks: an in-memory buffer,
/// a file or a caller-owned producer all satisfy it. `read_at` must fill from
/// `offset` and return the number of bytes written; a short read means the source
/// ended, and a declared total that does not match the bytes actually available
/// is reported as a failure by the operation.
pub trait EditSource {
    /// Declared total length of the replacement for `index`.
    fn replacement_len(&self, index: usize) -> u64;

    /// Reads replacement `index` from `offset` into `buffer`.
    fn read_at(&self, index: usize, offset: u64, buffer: &mut [u8]) -> ContentResult<usize>;
}

/// `Read` adapter over one declared replacement range.
pub struct ReplacementReader<'a> {
    source: &'a dyn EditSource,
    index: usize,
    position: u64,
    length: u64,
}

impl<'a> ReplacementReader<'a> {
    /// Reader over `0..length` of replacement `index`.
    pub fn new(source: &'a dyn EditSource, index: usize, length: u64) -> Self {
        Self {
            source,
            index,
            position: 0,
            length,
        }
    }

    /// Bytes not yet read.
    pub const fn remaining(&self) -> u64 {
        self.length - self.position
    }
}

impl Read for ReplacementReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        let remaining = self.remaining();
        if remaining == 0 {
            return Ok(0);
        }
        let want = usize::try_from(remaining)
            .unwrap_or(usize::MAX)
            .min(buffer.len());
        let read = self
            .source
            .read_at(self.index, self.position, &mut buffer[..want])
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::UnexpectedEof))?;
        if read == 0 || read > want {
            return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
        }
        self.position += read as u64;
        Ok(read)
    }
}

/// One planned piece of the result, in base and result order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Segment {
    /// Base bytes that survive into the result.
    Retain {
        /// Base-coordinate range.
        base: (u64, u64),
    },
    /// A declared replacement standing in for a base range.
    Replace {
        /// Replacement index.
        index: usize,
        /// Base-coordinate range the replacement stands in for.
        base: (u64, u64),
        /// Declared replacement length.
        len: u64,
    },
}

/// Lazy cursor that maps current-result coordinates onto base coordinates.
///
/// The plan is a function of the stream, not a copy of it: nothing here holds a
/// second edit list, and the base cursor only ever moves forward, so an arbitrary
/// number of edits is applied in one pass. A nonempty retained run before an edit
/// is yielded as its own segment, so a caller never has to reconstruct the gaps
/// itself.
pub struct Plan<'a> {
    stream: &'a dyn EditSequence,
    index: usize,
    base_cursor: u64,
    result_cursor: u64,
    pending_replace: Option<(usize, u64, u64, u64)>,
}

impl<'a> Plan<'a> {
    /// Cursor over the beginning of `stream`.
    pub fn new(stream: &'a dyn EditSequence) -> Self {
        Self {
            stream,
            index: 0,
            base_cursor: 0,
            result_cursor: 0,
            pending_replace: None,
        }
    }

    /// Next piece of the result, or `None` after the trailing retained run.
    pub fn advance(&mut self) -> ContentResult<Option<Segment>> {
        if let Some((index, start, end, len)) = self.pending_replace.take() {
            let edit = self.stream.edit_at(index)?;
            self.base_cursor = end;
            self.result_cursor = edit
                .start()
                .checked_add(edit.replacement_len())
                .ok_or(ContentError::LengthOverflow)?;
            self.index = index + 1;
            return Ok(Some(Segment::Replace {
                index,
                base: (start, end),
                len,
            }));
        }
        if self.index >= self.stream.len() {
            if self.base_cursor < self.stream.base_len() {
                let segment = Segment::Retain {
                    base: (self.base_cursor, self.stream.base_len()),
                };
                self.base_cursor = self.stream.base_len();
                return Ok(Some(segment));
            }
            return Ok(None);
        }
        let edit = self.stream.edit_at(self.index)?;
        let retained =
            edit.start()
                .checked_sub(self.result_cursor)
                .ok_or(ContentError::InvalidEdit {
                    what: "coordinate before current result",
                })?;
        let retain_end = self
            .base_cursor
            .checked_add(retained)
            .ok_or(ContentError::LengthOverflow)?;
        let removal_end = retain_end
            .checked_add(edit.removed_len())
            .ok_or(ContentError::LengthOverflow)?;
        if removal_end > self.stream.base_len() {
            return Err(ContentError::InvalidEdit {
                what: "range beyond base length",
            });
        }
        if retain_end > self.base_cursor {
            self.pending_replace =
                Some((self.index, retain_end, removal_end, edit.replacement_len()));
            let segment = Segment::Retain {
                base: (self.base_cursor, retain_end),
            };
            self.base_cursor = retain_end;
            self.result_cursor = edit.start();
            return Ok(Some(segment));
        }
        self.index += 1;
        self.base_cursor = removal_end;
        self.result_cursor = edit
            .start()
            .checked_add(edit.replacement_len())
            .ok_or(ContentError::LengthOverflow)?;
        Ok(Some(Segment::Replace {
            index: self.index - 1,
            base: (retain_end, removal_end),
            len: edit.replacement_len(),
        }))
    }

    /// Base position reached after `advance` returned `None`.
    pub const fn base_end(&self) -> u64 {
        self.base_cursor
    }
}
