//! One charged pack-page cache for a final ordered file upload.
use super::{
    extents::{Extent, ExtentKind},
    generation::{locator_key, parse_locator, ActiveBacking, ActiveSnapshot},
    pack::PackRecords,
};
use crate::WorkspaceError;
use std::sync::Arc;

pub(crate) struct ActivePackReader {
    active: Arc<ActiveBacking>,
    view: Arc<ActiveSnapshot>,
    cached: Option<(u64, PackRecords)>,
}

impl ActivePackReader {
    pub fn new(active: Arc<ActiveBacking>, view: Arc<ActiveSnapshot>) -> Self {
        Self {
            active,
            view,
            cached: None,
        }
    }

    pub fn read(
        &mut self,
        inode: u64,
        extent: Extent,
        position: u64,
        output: &mut [u8],
    ) -> Result<(), WorkspaceError> {
        if extent.kind != ExtentKind::Packed
            || position < extent.start
            || position
                .checked_add(output.len() as u64)
                .is_none_or(|end| end > extent.end)
        {
            return Err(WorkspaceError::InvalidInput);
        }
        if self
            .cached
            .as_ref()
            .is_none_or(|(logical, _)| *logical != extent.logical_page)
        {
            let locator = self
                .view
                .get(&locator_key(extent.logical_page))?
                .ok_or(WorkspaceError::Io)?;
            let physical = parse_locator(&locator)?;
            self.cached = Some((
                extent.logical_page,
                self.active.pack.records(extent.logical_page, physical)?,
            ));
        }
        let (_, page) = self.cached.as_ref().ok_or(WorkspaceError::Io)?;
        let record = page
            .records
            .get(extent.ordinal as usize)
            .ok_or(WorkspaceError::Io)?;
        let slot = record.slot;
        if slot.logical_page != extent.logical_page
            || slot.ordinal != extent.ordinal
            || slot.inode != inode
            || slot.generation != extent.generation
            || slot.revision != extent.revision
            || slot.offset
                != extent
                    .start
                    .checked_sub(extent.source_offset)
                    .ok_or(WorkspaceError::Io)?
            || slot.length != extent.slot_length
        {
            return Err(WorkspaceError::Io);
        }
        let start = usize::try_from(extent.source_offset + position - extent.start)
            .map_err(|_| WorkspaceError::Io)?;
        let end = start.checked_add(output.len()).ok_or(WorkspaceError::Io)?;
        output.copy_from_slice(record.bytes.get(start..end).ok_or(WorkspaceError::Io)?);
        Ok(())
    }
}
