//! In-memory `EditSequence`/`EditSource` fixtures for one-shot edit drives.
//!
//! C1 publishes the two traits and no resident-first sequence, because a caller
//! that must replay a bounded spool supplies its own implementation. Every edit
//! row in this harness declares one final run it already holds in memory, so the
//! harness is that caller for its own rows. This file is harness code, not
//! product code: it holds no product rule the product does not already state.

use layerfs_content::{ContentError, ContentResult, Edit, EditSequence, EditSource};

/// One validated ordered stream held as fixed records.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Edits {
    edits: Vec<Edit>,
    base_len: u64,
    final_len: u64,
}

impl Edits {
    /// Validates `edits` against `base_len` and accumulates the final length.
    pub fn new(base_len: u64, edits: Vec<Edit>) -> ContentResult<Self> {
        let mut length = base_len;
        let mut previous_end = 0_u64;
        for edit in &edits {
            if edit.start() > edit.end() {
                return Err(ContentError::InvalidEdit {
                    what: "inverted range",
                });
            }
            if edit.end() > length {
                return Err(ContentError::InvalidEdit {
                    what: "range beyond current result",
                });
            }
            if edit.start() < previous_end {
                return Err(ContentError::InvalidEdit {
                    what: "range inside an earlier replacement",
                });
            }
            length = edit.apply_len(length)?;
            previous_end = edit
                .start()
                .checked_add(edit.replacement_len())
                .ok_or(ContentError::LengthOverflow)?;
        }
        Ok(Self {
            edits,
            base_len,
            final_len: length,
        })
    }

    /// Result length the declared edits accumulate to.
    pub const fn final_len(&self) -> u64 {
        self.final_len
    }

    /// The edits in order.
    pub fn edits(&self) -> &[Edit] {
        &self.edits
    }
}

impl EditSequence for Edits {
    fn base_len(&self) -> u64 {
        self.base_len
    }
    fn final_len(&self) -> u64 {
        self.final_len
    }
    fn len(&self) -> usize {
        self.edits.len()
    }
    fn edit_at(&self, index: usize) -> ContentResult<Edit> {
        self.edits
            .get(index)
            .copied()
            .ok_or(ContentError::InvalidEdit { what: "edit index" })
    }
}

/// In-memory [`EditSource`] for rows whose replacement bytes are already held.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Parts {
    parts: Vec<Vec<u8>>,
}

impl Parts {
    /// Empty source.
    pub fn new() -> Self {
        Self { parts: Vec::new() }
    }

    /// Appends one replacement and returns its index.
    pub fn push(&mut self, bytes: Vec<u8>) -> usize {
        self.parts.push(bytes);
        self.parts.len() - 1
    }
}

impl EditSource for Parts {
    fn replacement_len(&self, index: usize) -> u64 {
        self.parts.get(index).map_or(0, |part| part.len() as u64)
    }

    fn read_at(&self, index: usize, offset: u64, buffer: &mut [u8]) -> ContentResult<usize> {
        let part = self.parts.get(index).ok_or(ContentError::InvalidEdit {
            what: "replacement index",
        })?;
        let start = usize::try_from(offset).map_err(|_| ContentError::LengthOverflow)?;
        if start > part.len() {
            return Err(ContentError::InvalidEdit {
                what: "replacement offset",
            });
        }
        let available = &part[start..];
        let count = available.len().min(buffer.len());
        buffer[..count].copy_from_slice(&available[..count]);
        Ok(count)
    }
}
