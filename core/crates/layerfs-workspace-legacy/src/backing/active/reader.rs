//! One charged pack-page cache for a final ordered file upload.
use super::{
    extents::{Extent, ExtentKind},
    generation::{locator_key, parse_locator, ActiveBacking, ActiveSnapshot},
    pack::PackRecords,
};
use crate::WorkspaceError;
use std::{sync::Arc, time::Instant};

pub(crate) struct ActivePackReader {
    active: Arc<ActiveBacking>,
    view: Arc<ActiveSnapshot>,
    cached: Option<(u64, PackRecords)>,
    stats: SourceStats,
}

#[derive(Default, Clone, Copy)]
pub(crate) struct SourceStats {
    pub(crate) loads: u64,
    pub(crate) hits: u64,
    pub(crate) locator_lookups: u64,
    pub(crate) index_reads: u64,
    pub(crate) index_seeks: u64,
    pub(crate) locator_ns: u64,
    pub(crate) pack_ns: u64,
    pub(crate) decoded_records: u64,
    pub(crate) decoded_bytes: u64,
    pub(crate) copied_bytes: u64,
}

impl ActivePackReader {
    pub(crate) fn stats(&self) -> SourceStats {
        self.stats
    }

    pub fn new(active: Arc<ActiveBacking>, view: Arc<ActiveSnapshot>) -> Self {
        Self {
            active,
            view,
            cached: None,
            stats: SourceStats::default(),
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
            let before = self.active.source_counts();
            let begun = Instant::now();
            let locator = self
                .view
                .get(&locator_key(extent.logical_page))?
                .ok_or(WorkspaceError::Io)?;
            let after = self.active.source_counts();
            self.stats.index_reads += after.0.saturating_sub(before.0);
            self.stats.index_seeks += after.1.saturating_sub(before.1);
            self.stats.locator_lookups += 1;
            self.stats.locator_ns += begun.elapsed().as_nanos() as u64;
            let physical = parse_locator(&locator)?;
            let begun = Instant::now();
            let records = self.active.pack.records(extent.logical_page, physical)?;
            self.stats.pack_ns += begun.elapsed().as_nanos() as u64;
            self.stats.loads += 1;
            self.stats.decoded_records += records.records.len() as u64;
            self.stats.decoded_bytes += records.used as u64;
            self.cached = Some((extent.logical_page, records));
        } else {
            self.stats.hits += 1;
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
        self.stats.copied_bytes += output.len() as u64;
        Ok(())
    }
}
