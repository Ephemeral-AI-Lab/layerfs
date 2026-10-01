//! Exact ordinal selection validates one whole sparse16 block before returning.
use super::declaration::checkpoint_count;
use super::spool::RowSpool;
use super::spool_slots::Slot;
use crate::error::{ContentError, ContentResult};
use crate::filesystem::path::PathName;

impl RowSpool {
    pub(super) fn ordinal_binding(
        &self,
        slot: &Slot,
        ordinal: u32,
    ) -> ContentResult<(PathName, Option<u64>)> {
        let groups = 1 + checkpoint_count(u64::from(slot.records));
        let group = u64::from(ordinal / 16);
        let data = slot
            .offset
            .checked_add((groups - 1) * 8)
            .ok_or(ContentError::LengthOverflow)?;
        let mut at = data
            .checked_add(self.checked_checkpoint(slot, group, groups)?)
            .ok_or(ContentError::LengthOverflow)?;
        let end = if group + 1 < groups {
            data.checked_add(self.checked_checkpoint(slot, group + 1, groups)?)
                .ok_or(ContentError::LengthOverflow)?
        } else {
            slot.offset
                .checked_add(slot.bytes)
                .ok_or(ContentError::LengthOverflow)?
        };
        let count = (u64::from(slot.records) - group * 16).min(16);
        let mut previous: Option<PathName> = None;
        let mut selected = None;
        for offset in 0..count {
            let (name, child, next) = self.decode_binding(at, end)?;
            if previous.as_ref().is_some_and(|prior| prior >= &name) {
                return Err(ContentError::NonCanonicalOrdering);
            }
            if offset == u64::from(ordinal % 16) {
                selected = Some((name.clone(), child));
            }
            previous = Some(name);
            at = next;
        }
        if at != end {
            return Err(ContentError::InvalidRecord("directory block span"));
        }
        selected.ok_or(ContentError::InvalidRecord("binding ordinal"))
    }
}
