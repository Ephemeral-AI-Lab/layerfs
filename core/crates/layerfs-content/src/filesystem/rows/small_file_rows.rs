//! Immutable fixed row cursors and hard-zero namespace/fresh selections.
use super::*;
use crate::filesystem::{DirectoryUpdate, InodeUpdate, PathName};
use crate::object::inode_leaf::InodeValue;
use crate::{ContentError, ContentResult};
struct Empty;
impl DirectoryRowSource for Empty {
    fn next_row(&mut self) -> ContentResult<Option<DirectoryUpdate>> {
        Ok(None)
    }
}
impl SerialRowSource for Empty {
    fn next_row(&mut self) -> ContentResult<Option<u64>> {
        Ok(None)
    }
}
impl DirectoryHeaderSource for Empty {
    fn next_header(&mut self) -> ContentResult<Option<DirectoryHeader>> {
        Ok(None)
    }
}
struct Inodes<'a> {
    rows: &'a VerifiedSmallFileRows,
    at: usize,
    _memory: crate::filesystem::state::GraphMemoryLease,
}
impl InodeRowSource for Inodes<'_> {
    fn next_row(&mut self) -> ContentResult<Option<InodeUpdate>> {
        let row = self.rows.row(self.at);
        self.at += usize::from(row.is_some());
        Ok(row)
    }
}
fn serial(value: u64) -> ContentResult<()> {
    if !super::serial_in_range(value) {
        return Err(ContentError::InvalidRecord("small file selected serial"));
    }
    Ok(())
}
impl RowSource for VerifiedSmallFileRows {
    fn directory_rows(&self) -> usize {
        0
    }
    fn inode_rows(&self) -> usize {
        self.len()
    }
    fn new_rows(&self) -> usize {
        0
    }
    fn directories(&self) -> ContentResult<Box<dyn DirectoryRowSource + '_>> {
        Ok(Box::new(Empty))
    }
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>> {
        let memory = self.memory.reserve(std::mem::size_of::<Inodes<'_>>())?;
        Ok(Box::new(Inodes {
            rows: self,
            at: 0,
            _memory: memory,
        }))
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        Ok(Box::new(Empty))
    }
    fn directory_for(&self, parent: u64) -> ContentResult<Option<DirectoryUpdate>> {
        serial(parent)?;
        Ok(None)
    }
    fn value_for(&self, value: u64) -> ContentResult<Option<InodeValue>> {
        serial(value)?;
        Ok((0..self.len())
            .filter_map(|at| self.row(at))
            .find(|row| row.serial == value)
            .map(|row| row.value))
    }
    fn new_position(&self, value: u64) -> ContentResult<Option<usize>> {
        serial(value)?;
        Ok(None)
    }
    fn legacy_binding_lookup(&self, parent: u64, name: &[u8]) -> ContentResult<BindingLookup> {
        self.binding_for(parent, name)
    }
    fn legacy_binding_at(
        &self,
        _parent: u64,
        _ordinal: u32,
    ) -> ContentResult<(PathName, Option<u64>)> {
        Err(ContentError::InvalidRecord("small file binding ordinal"))
    }
}
impl BindingRows for VerifiedSmallFileRows {
    fn binding_source_id(&self) -> ContentResult<BindingSourceId> {
        Ok(self.source_id())
    }
    fn binding_at(&self, _point: &BindingPoint) -> ContentResult<(PathName, Option<u64>)> {
        Err(ContentError::InvalidRecord("small file binding point"))
    }
    fn directory_headers(&self) -> ContentResult<Box<dyn DirectoryHeaderSource + '_>> {
        Ok(Box::new(Empty))
    }
    fn directory_header(&self, parent: u64) -> ContentResult<Option<DirectoryHeader>> {
        serial(parent)?;
        Ok(None)
    }
    fn bindings(&self, _header: &DirectoryHeader) -> ContentResult<Box<dyn BindingRowSource + '_>> {
        Err(ContentError::InvalidRecord("small file directory header"))
    }
    fn binding_for(&self, parent: u64, name: &[u8]) -> ContentResult<BindingLookup> {
        serial(parent)?;
        PathName::from_bytes(name)?;
        Ok(BindingLookup::Unmentioned)
    }
}
