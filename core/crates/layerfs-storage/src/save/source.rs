//! A save's bounded immutable wave view over committed physical input.
use super::seal::Packer;
use crate::{
    error::StorageResult,
    location::{ObjectLocation, SignatureRow, ValueGroupRow},
    read::Fetch,
    source::Source,
};
use layerfs_content::ObjectId;
use std::{cell::RefCell, collections::BTreeMap};
pub(super) struct WaveSource<'a> {
    pub(super) fetch: &'a Fetch,
    pub(super) packer: &'a Packer,
    pub(super) signatures: &'a RefCell<BTreeMap<usize, SignatureRow>>,
}
impl Source for WaveSource<'_> {
    fn ordered_dependencies(&self) -> bool {
        false
    }
    fn location(&self, id: ObjectId, ceiling: i64) -> StorageResult<Option<ObjectLocation>> {
        match self.packer.location(id) {
            Some(row) => Ok(Some(row)),
            None => self.fetch.location(id, ceiling),
        }
    }
    fn pack_bytes(&self, id: i64) -> StorageResult<Vec<u8>> {
        if let Some(body) = self.packer.pooled_body(id)? {
            return Ok(body);
        }
        match self
            .packer
            .ready
            .iter()
            .find(|pack| pack.info.pack_id == id)
        {
            Some(pack) => Ok(pack.body.as_ref().clone()),
            None => self.fetch.pack_bytes(id),
        }
    }
    fn value_group(&self, ordinal: u32) -> StorageResult<Option<ValueGroupRow>> {
        match self.packer.pooled_row(ordinal) {
            Some(row) => Ok(Some(row)),
            None => self.fetch.value_group(ordinal),
        }
    }
    fn value_groups(
        &self,
        from: Option<u32>,
        visit: &mut dyn FnMut(ValueGroupRow) -> StorageResult<()>,
    ) -> StorageResult<()> {
        self.fetch.value_groups(from, visit)
    }
    fn window_start(&self) -> StorageResult<u32> {
        self.fetch.window_start()
    }
    fn signatures(&self) -> StorageResult<Vec<SignatureRow>> {
        self.fetch.signatures()
    }
    fn write_signatures(&self, rows: &[SignatureRow]) -> StorageResult<usize> {
        for row in rows {
            self.signatures.borrow_mut().insert(row.slot, *row);
        }
        Ok(rows.len())
    }
    fn note_pack_cache_hit(&self) {
        self.fetch.note_pack_cache_hit();
    }
}
