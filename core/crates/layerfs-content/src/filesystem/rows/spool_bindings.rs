//! Sealed header views and full-name checkpoint search with a bounded local scan.

use super::declaration::checkpoint_count;
use super::spool::{RowSpool, KIND_DIRECTORY};
use super::spool_cursor::SpoolBindings;
use super::spool_slots::Slot;
use super::{
    BindingLookup, BindingPoint, BindingRowSource, BindingRows, BindingSourceId, DirectoryHeader,
    DirectoryHeaderSource,
};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::path::PathName;

impl BindingRows for RowSpool {
    fn binding_source_id(&self) -> ContentResult<BindingSourceId> {
        self.ensure_known()?;
        Ok(self.authority.source_id())
    }
    fn binding_at(&self, point: &BindingPoint) -> ContentResult<(PathName, Option<u64>)> {
        if !self.authority.accepts_point(point) {
            return Err(ContentError::InvalidRecord("directory issuer"));
        }
        self.ensure_sealed()?;
        let index =
            usize::try_from(point.header_descriptor()).map_err(|_| ContentError::LengthOverflow)?;
        let slot = self
            .directory_slot(index)?
            .ok_or(ContentError::InvalidRecord("directory selection"))?;
        if slot.key != point.parent() || point.binding_ordinal() >= slot.records {
            return Err(ContentError::InvalidRecord("binding ordinal"));
        }
        self.ordinal_binding(&slot, point.binding_ordinal())
    }
    fn directory_headers(&self) -> ContentResult<Box<dyn DirectoryHeaderSource + '_>> {
        self.ensure_sealed()?;
        Ok(Box::new(SpoolHeaders {
            spool: self,
            at: 0,
            failure: None,
        }))
    }

    fn directory_header(&self, parent: u64) -> ContentResult<Option<DirectoryHeader>> {
        self.ensure_sealed()?;
        self.find(KIND_DIRECTORY, parent)?
            .map(|(index, slot)| self.slot_header(index, &slot))
            .transpose()
    }

    fn bindings(
        &self,
        selected: &DirectoryHeader,
    ) -> ContentResult<Box<dyn BindingRowSource + '_>> {
        self.ensure_sealed()?;
        if !self.authority.accepts(selected) {
            return Err(ContentError::InvalidRecord("directory issuer"));
        }
        let index =
            usize::try_from(selected.ordinal()).map_err(|_| ContentError::LengthOverflow)?;
        let slot = self
            .directory_slot(index)?
            .ok_or(ContentError::InvalidRecord("directory selection"))?;
        if self.slot_header(index, &slot)? != *selected {
            return Err(ContentError::InvalidRecord("directory selection"));
        }
        Ok(Box::new(SpoolBindings::new(self, *selected, slot)?))
    }

    fn binding_for(&self, parent: u64, name: &[u8]) -> ContentResult<BindingLookup> {
        self.ensure_sealed()?;
        PathName::from_bytes(name)?;
        let Some((_, slot)) = self.find(KIND_DIRECTORY, parent)? else {
            return Ok(BindingLookup::Unmentioned);
        };
        self.find_binding(&slot, name)
    }
}

impl RowSpool {
    fn find_binding(&self, slot: &Slot, target: &[u8]) -> ContentResult<BindingLookup> {
        if slot.records == 0 {
            return Ok(BindingLookup::Unmentioned);
        }
        let groups = 1 + checkpoint_count(u64::from(slot.records));
        let data_start = slot
            .offset
            .checked_add((groups - 1) * 8)
            .ok_or(ContentError::LengthOverflow)?;
        let end = slot
            .offset
            .checked_add(slot.bytes)
            .ok_or(ContentError::LengthOverflow)?;
        let mut low = 0_u64;
        let mut high = groups;
        while low < high {
            let middle = low + (high - low) / 2;
            let relative = self.checked_checkpoint(slot, middle, groups)?;
            let (name, _, _) = self.decode_binding(data_start + relative, end)?;
            let mut work = self.work.get();
            work.checkpoint_probes = work.checkpoint_probes.saturating_add(1);
            self.work.set(work);
            if name.as_bytes() <= target {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        if low == 0 {
            return Ok(BindingLookup::Unmentioned);
        }
        let group = low - 1;
        let mut at = data_start
            .checked_add(self.checked_checkpoint(slot, group, groups)?)
            .ok_or(ContentError::LengthOverflow)?;
        let block_end = if group + 1 < groups {
            data_start
                .checked_add(self.checked_checkpoint(slot, group + 1, groups)?)
                .ok_or(ContentError::LengthOverflow)?
        } else {
            end
        };
        let count = (u64::from(slot.records) - group * 16).min(16);
        let mut result = BindingLookup::Unmentioned;
        let mut previous: Option<PathName> = None;
        for _ in 0..count {
            let (name, child, next) = self.decode_binding(at, block_end)?;
            if previous.as_ref().is_some_and(|before| before >= &name) {
                return Err(ContentError::NonCanonicalOrdering);
            }
            if name.as_bytes() == target {
                result = child.map_or(BindingLookup::Absent, BindingLookup::Present);
            }
            previous = Some(name);
            at = next;
        }
        if at != block_end {
            return Err(ContentError::InvalidRecord("directory block span"));
        }
        Ok(result)
    }

    pub(super) fn checked_checkpoint(
        &self,
        slot: &Slot,
        block: u64,
        groups: u64,
    ) -> ContentResult<u64> {
        let relative = self.checkpoint(slot, block)?;
        if block > 0 && self.checkpoint(slot, block - 1)? >= relative {
            return Err(ContentError::InvalidRecord("directory checkpoint order"));
        }
        if block + 1 < groups && self.checkpoint(slot, block + 1)? <= relative {
            return Err(ContentError::InvalidRecord("directory checkpoint order"));
        }
        Ok(relative)
    }
}

struct SpoolHeaders<'a> {
    spool: &'a RowSpool,
    at: usize,
    failure: Option<ContentError>,
}

impl DirectoryHeaderSource for SpoolHeaders<'_> {
    fn next_header(&mut self) -> ContentResult<Option<DirectoryHeader>> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = self.spool.directory_slot(self.at).and_then(|slot| {
            slot.map(|slot| self.spool.slot_header(self.at, &slot))
                .transpose()
        });
        match &result {
            Ok(Some(_)) => self.at += 1,
            Err(error) => self.failure = Some(error.clone()),
            _ => {}
        }
        result
    }
}
