use super::super::session_legacy::{VfsError, VfsResult};
use layerfs_core::inode::{InodeId, InodeKind};
use layerfs_core::ObjectId;
use layerfs_storage::scratch::{DiskNamespace, DiskTable};
use layerfs_storage::Engine;
use std::cell::Cell;

pub(super) const SNAPSHOT_BYTES: usize = 105;
pub(super) const ORDERED_PATH_PREFIX_BYTES: usize = 2;

pub(super) struct RefreshScratch {
    pub(super) table: DiskTable,
    serial: Cell<u32>,
}

impl RefreshScratch {
    pub(super) fn create(engine: &Engine) -> VfsResult<Self> {
        let table = engine.create_scratch_table("refresh")?;
        Ok(Self {
            table,
            serial: Cell::new(0),
        })
    }

    pub(super) fn table(&self, label: &str) -> VfsResult<DiskNamespace<'_>> {
        let serial = self.serial.get();
        self.serial
            .set(serial.checked_add(1).ok_or(VfsError::InvalidState)?);
        let name = format!("{serial:04x}-{label}");
        Ok(self.table.namespace(name.as_bytes())?)
    }
}

pub(super) trait EntryPrefix {
    fn visit_prefix(
        &self,
        prefix: &[u8],
        callback: impl FnMut(&[u8], &[u8]) -> layerfs_storage::EngineResult<()>,
    ) -> layerfs_storage::EngineResult<()>;
}

impl EntryPrefix for DiskTable {
    fn visit_prefix(
        &self,
        prefix: &[u8],
        callback: impl FnMut(&[u8], &[u8]) -> layerfs_storage::EngineResult<()>,
    ) -> layerfs_storage::EngineResult<()> {
        self.for_each_entry_prefix(prefix, callback)
    }
}

impl EntryPrefix for DiskNamespace<'_> {
    fn visit_prefix(
        &self,
        prefix: &[u8],
        callback: impl FnMut(&[u8], &[u8]) -> layerfs_storage::EngineResult<()>,
    ) -> layerfs_storage::EngineResult<()> {
        self.for_each_entry_prefix(prefix, callback)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Snapshot {
    pub(super) inode: InodeId,
    pub(super) kind: InodeKind,
    pub(super) namespace_ref_count: u64,
    pub(super) content_root: ObjectId,
    pub(super) metadata_root: ObjectId,
}
