//! A save's bounded immutable wave view over committed physical input.
use super::seal::Packer;
use crate::{
    error::StorageResult,
    location::{ObjectLocation, SignatureRow},
    read::Fetch,
    source::Source,
};
use layerfs_content::{inode_leaf::INODE_VALUE_BYTES, ObjectId};
use std::{cell::RefCell, collections::BTreeMap};
pub(super) struct WaveSource<'a> {
    pub(super) fetch: &'a Fetch,
    pub(super) packer: &'a Packer,
    pub(super) signatures: &'a RefCell<BTreeMap<usize, SignatureRow>>,
    /// Values this Save assigned and has not published yet, by ordinal.
    pub(super) values: &'a BTreeMap<u32, [u8; INODE_VALUE_BYTES]>,
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
    fn base_location(&self, id: ObjectId, ceiling: i64) -> StorageResult<Option<ObjectLocation>> {
        match self.packer.location(id) {
            Some(row) => Ok(Some(row)),
            None => self.fetch.base_location(id, ceiling),
        }
    }
    fn pack_bytes(&self, id: i64) -> StorageResult<Vec<u8>> {
        match self
            .packer
            .ready
            .iter()
            .find(|sealed| sealed.pack.info().pack_id == id)
        {
            Some(sealed) => Ok(sealed.pack.body().as_ref().clone()),
            None => self.fetch.pack_bytes(id),
        }
    }
    fn validate_cached_pack(&self, info: crate::location::PackInfo) -> StorageResult<()> {
        if let Some(sealed) = self
            .packer
            .ready
            .iter()
            .find(|sealed| sealed.pack.info().pack_id == info.pack_id)
        {
            if sealed.pack.info() != info {
                return Err(crate::StorageError::Integrity(
                    "private sealed descriptor changed",
                ));
            }
            Ok(())
        } else {
            self.fetch.validate_cached_pack(info)
        }
    }
    fn acquire_groups(
        &self,
        id: i64,
        groups: &[usize],
        whole_for_reuse: bool,
    ) -> StorageResult<crate::encoding::PackAcquisition> {
        if let Some(sealed) = self
            .packer
            .ready
            .iter()
            .find(|sealed| sealed.pack.info().pack_id == id)
        {
            return Ok(crate::encoding::PackAcquisition::Whole {
                info: Some(sealed.pack.info()),
                body: sealed.pack.body().as_ref().clone(),
            });
        }
        self.fetch.acquire_groups(id, groups, whole_for_reuse)
    }
    /// This Save's unpublished values are answered from memory; the stored
    /// ones are asked for together.
    fn inode_values(&self, ordinals: &[u32]) -> StorageResult<Vec<[u8; INODE_VALUE_BYTES]>> {
        let stored: Vec<u32> = ordinals
            .iter()
            .copied()
            .filter(|ordinal| !self.values.contains_key(ordinal))
            .collect();
        let mut stored = if stored.is_empty() {
            Vec::new()
        } else {
            self.fetch.inode_values(&stored)?
        }
        .into_iter();
        ordinals
            .iter()
            .map(|ordinal| match self.values.get(ordinal) {
                Some(value) => Ok(*value),
                None => stored
                    .next()
                    .ok_or(crate::StorageError::Integrity("metadata ordinal missing")),
            })
            .collect()
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
    fn note_pack_evictions(&self, entries: u64, bytes: u64) {
        self.fetch.note_pack_evictions(entries, bytes);
    }
    fn note_pack_directory_validation(&self, groups: usize, entries: usize) {
        self.fetch.note_pack_directory_validation(groups, entries);
    }
    fn note_pack_cache_hit(&self) {
        self.fetch.note_pack_cache_hit();
    }
}
