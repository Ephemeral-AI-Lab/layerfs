//! Byte forms of the merge's spilled entries: decisions and their paths.

use crate::compare::tree::EntryFacts;
use crate::error::{ContentError, ContentResult};
use crate::object::inode_leaf::{decode_inode_value, encode_inode_value, INODE_VALUE_BYTES};

const ENTRY_BYTES: usize = 8 + INODE_VALUE_BYTES;

fn malformed() -> ContentError {
    ContentError::InvalidRecord("merge spill entry")
}

/// Reads bytes from the front of a slice.
pub(super) struct Cursor<'b>(pub(super) &'b [u8]);

impl<'b> Cursor<'b> {
    pub(super) fn take(&mut self, count: usize) -> ContentResult<&'b [u8]> {
        if self.0.len() < count {
            return Err(malformed());
        }
        let (head, rest) = self.0.split_at(count);
        self.0 = rest;
        Ok(head)
    }

    pub(super) fn byte(&mut self) -> ContentResult<u8> {
        Ok(self.take(1)?[0])
    }

    pub(super) fn number(&mut self) -> ContentResult<u64> {
        Ok(u64::from_be_bytes(
            self.take(8)?.try_into().map_err(|_| malformed())?,
        ))
    }

    /// A path written by [`put_text`].
    pub(super) fn text(&mut self) -> ContentResult<String> {
        let length = u16::from_be_bytes(self.take(2)?.try_into().map_err(|_| malformed())?);
        String::from_utf8(self.take(length as usize)?.to_vec()).map_err(|_| malformed())
    }

    pub(super) fn entry(&mut self) -> ContentResult<EntryFacts> {
        let serial = self.number()?;
        let value = decode_inode_value(self.take(INODE_VALUE_BYTES)?)?;
        Ok(EntryFacts { serial, value })
    }

    pub(super) fn end(&self) -> ContentResult<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(malformed())
        }
    }
}

pub(super) fn put_text(out: &mut Vec<u8>, text: &str) -> ContentResult<()> {
    let length = u16::try_from(text.len()).map_err(|_| malformed())?;
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(text.as_bytes());
    Ok(())
}

pub(super) fn put_entry(out: &mut Vec<u8>, entry: &EntryFacts) {
    out.reserve(ENTRY_BYTES);
    out.extend_from_slice(&entry.serial.to_be_bytes());
    out.extend_from_slice(&encode_inode_value(entry.value));
}
