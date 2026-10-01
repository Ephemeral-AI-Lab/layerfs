//! Concrete zero source proof after declared admission and actual provided EOF.
use super::{
    BindingAuthority, BindingLookup, BindingPoint, BindingRowSource, BindingRows, BindingSourceId,
    DirectoryHeader, DirectoryHeaderSource, DirectoryRowSource, InodeRowSource, RowSource,
    SerialRowSource, SpoolPreparation,
};
use crate::filesystem::{DirectoryUpdate, InodeUpdate, PathName};
use crate::object::inode_leaf::InodeValue;
use crate::{ContentError, ContentResult};
use std::io::Read;

/// One immutable issued zero source; no native spool or raw-ID adoption.
#[derive(Debug)]
pub struct VerifiedEmptyRows {
    authority: BindingAuthority,
}
impl SpoolPreparation {
    /// True only for all five exact declared populations, not just directory/name counts.
    pub fn declares_empty(&self) -> bool {
        let d = self.declaration;
        d.directories == 0
            && d.inodes == 0
            && d.fresh == 0
            && d.bindings == 0
            && d.wire_name_bytes == 0
    }
    /// Consume the original issued authority after an actual remaining-input EOF.
    /// A wire adapter first runs its existing tag/grammar parser; this does not
    /// invent a wire version or turn a Boolean into completion authority.
    pub fn verify_empty_eof(self, input: &mut dyn Read) -> ContentResult<VerifiedEmptyRows> {
        if !self.declares_empty() {
            return Err(ContentError::InvalidRecord("empty source declaration"));
        }
        let mut byte = [0u8; 1];
        if input.read(&mut byte).map_err(|_| ContentError::Io)? != 0 {
            return Err(ContentError::TrailingBytes);
        }
        Ok(VerifiedEmptyRows {
            authority: self.authority,
        })
    }
}
impl VerifiedEmptyRows {
    /// Exact originally issued source, preserved without a spool/native token.
    pub fn source_id(&self) -> BindingSourceId {
        self.authority.source_id()
    }
    /// Logical end of a zero immutable source; there is no native cleanup effect.
    pub fn cleanup(self) -> ContentResult<()> {
        Ok(())
    }
}
struct Cursor;
impl DirectoryRowSource for Cursor {
    fn next_row(&mut self) -> ContentResult<Option<DirectoryUpdate>> {
        Ok(None)
    }
}
impl InodeRowSource for Cursor {
    fn next_row(&mut self) -> ContentResult<Option<InodeUpdate>> {
        Ok(None)
    }
}
impl SerialRowSource for Cursor {
    fn next_row(&mut self) -> ContentResult<Option<u64>> {
        Ok(None)
    }
}
impl DirectoryHeaderSource for Cursor {
    fn next_header(&mut self) -> ContentResult<Option<DirectoryHeader>> {
        Ok(None)
    }
}
fn serial(value: u64) -> ContentResult<()> {
    if !super::serial_in_range(value) {
        return Err(ContentError::InvalidRecord("empty source serial"));
    }
    Ok(())
}
impl RowSource for VerifiedEmptyRows {
    fn directory_rows(&self) -> usize {
        0
    }
    fn inode_rows(&self) -> usize {
        0
    }
    fn new_rows(&self) -> usize {
        0
    }
    fn directories(&self) -> ContentResult<Box<dyn DirectoryRowSource + '_>> {
        Ok(Box::new(Cursor))
    }
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>> {
        Ok(Box::new(Cursor))
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        Ok(Box::new(Cursor))
    }
    fn directory_for(&self, parent: u64) -> ContentResult<Option<DirectoryUpdate>> {
        serial(parent)?;
        Ok(None)
    }
    fn value_for(&self, value: u64) -> ContentResult<Option<InodeValue>> {
        serial(value)?;
        Ok(None)
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
        Err(ContentError::InvalidRecord("empty source ordinal"))
    }
}
impl BindingRows for VerifiedEmptyRows {
    fn binding_source_id(&self) -> ContentResult<BindingSourceId> {
        Ok(self.source_id())
    }
    fn binding_at(&self, _point: &BindingPoint) -> ContentResult<(PathName, Option<u64>)> {
        Err(ContentError::InvalidRecord("empty source point"))
    }
    fn directory_headers(&self) -> ContentResult<Box<dyn DirectoryHeaderSource + '_>> {
        Ok(Box::new(Cursor))
    }
    fn directory_header(&self, parent: u64) -> ContentResult<Option<DirectoryHeader>> {
        serial(parent)?;
        Ok(None)
    }
    fn bindings(&self, _header: &DirectoryHeader) -> ContentResult<Box<dyn BindingRowSource + '_>> {
        Err(ContentError::InvalidRecord("empty source header"))
    }
    fn binding_for(&self, parent: u64, name: &[u8]) -> ContentResult<BindingLookup> {
        serial(parent)?;
        PathName::from_bytes(name)?;
        Ok(BindingLookup::Unmentioned)
    }
}
