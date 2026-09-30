//! Explicit borrowed directory rows and the retained legacy-profile adapter.

use super::binding::{BindingAuthority, DirectoryHeader};
use super::resident_cursor::{binding_bytes, ResidentBindings};
use super::{
    BindingLookup, BindingRowSource, BindingRows, DirectoryHeaderSource, DirectoryRowSource,
    InodeRowSource, PreparedRows, RowSource, SerialRowSource,
};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::identity::InodeScope;
use crate::filesystem::input::{DirectoryUpdate, FilesystemInput, FilesystemResources};
use crate::filesystem::root::FilesystemRootId;
use crate::object::inode_leaf::InodeValue;

/// An explicit scalar view of the caller's existing immutable input slices.
pub struct SliceBindingRows<'a> {
    input: &'a FilesystemInput<'a>,
    authority: BindingAuthority,
}

impl<'a> SliceBindingRows<'a> {
    /// Issues one selection authority without cloning complete directory rows.
    pub fn new(input: &'a FilesystemInput<'a>) -> ContentResult<Self> {
        Ok(Self {
            input,
            authority: BindingAuthority::new()?,
        })
    }

    fn header_at(&self, index: usize) -> ContentResult<DirectoryHeader> {
        let row = self
            .input
            .directories
            .get(index)
            .ok_or(ContentError::InvalidRecord("directory selection"))?;
        header(
            &self.authority,
            u64::try_from(index).map_err(|_| ContentError::LengthOverflow)?,
            row,
        )
    }
}

impl BindingRows for SliceBindingRows<'_> {
    fn directory_headers(&self) -> ContentResult<Box<dyn DirectoryHeaderSource + '_>> {
        Ok(Box::new(SliceHeaders { rows: self, at: 0 }))
    }

    fn directory_header(&self, parent: u64) -> ContentResult<Option<DirectoryHeader>> {
        match self
            .input
            .directories
            .binary_search_by_key(&parent, |row| row.parent)
        {
            Ok(index) => self.header_at(index).map(Some),
            Err(_) => Ok(None),
        }
    }

    fn bindings(
        &self,
        selected: &DirectoryHeader,
    ) -> ContentResult<Box<dyn BindingRowSource + '_>> {
        if !self.authority.accepts(selected) {
            return Err(ContentError::InvalidRecord("directory issuer"));
        }
        let index =
            usize::try_from(selected.ordinal()).map_err(|_| ContentError::LengthOverflow)?;
        if self.header_at(index)? != *selected {
            return Err(ContentError::InvalidRecord("directory selection"));
        }
        Ok(Box::new(ResidentBindings::borrowed(
            *selected,
            &self.input.directories[index].changes,
        )))
    }

    fn binding_for(&self, parent: u64, name: &[u8]) -> ContentResult<BindingLookup> {
        // A point query never computes the complete row's byte total.
        self.input.legacy_binding_lookup(parent, name)
    }
}

impl RowSource for SliceBindingRows<'_> {
    fn directory_rows(&self) -> usize {
        self.input.directory_rows()
    }
    fn inode_rows(&self) -> usize {
        self.input.inode_rows()
    }
    fn new_rows(&self) -> usize {
        self.input.new_rows()
    }
    fn directories(&self) -> ContentResult<Box<dyn DirectoryRowSource + '_>> {
        self.input.directories()
    }
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>> {
        self.input.inodes()
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        self.input.new_inodes()
    }
    fn directory_for(&self, parent: u64) -> ContentResult<Option<DirectoryUpdate>> {
        self.input.directory_for(parent)
    }
    fn legacy_binding_lookup(&self, parent: u64, name: &[u8]) -> ContentResult<BindingLookup> {
        BindingRows::binding_for(self, parent, name)
    }
    fn value_for(&self, serial: u64) -> ContentResult<Option<InodeValue>> {
        self.input.value_for(serial)
    }
    fn new_position(&self, serial: u64) -> ContentResult<Option<usize>> {
        self.input.new_position(serial)
    }
}

impl PreparedRows for SliceBindingRows<'_> {
    fn base(&self) -> Option<FilesystemRootId> {
        self.input.base
    }
    fn scope(&self) -> InodeScope {
        self.input.scope
    }
    fn root_serial(&self) -> u64 {
        self.input.root_serial
    }
    fn resources(&self) -> FilesystemResources {
        self.input.resources
    }
}

struct SliceHeaders<'a> {
    rows: &'a SliceBindingRows<'a>,
    at: usize,
}

impl DirectoryHeaderSource for SliceHeaders<'_> {
    fn next_header(&mut self) -> ContentResult<Option<DirectoryHeader>> {
        if self.at == self.rows.input.directories.len() {
            return Ok(None);
        }
        let result = self.rows.header_at(self.at)?;
        self.at += 1;
        Ok(Some(result))
    }
}

/// Explicit old-profile adapter for a source that returns whole directory rows.
/// Parent serial is its stable descriptor, so point selection never opens or
/// rescans the directory sequence. This is not a fallback after a bounded error.
pub struct CompatibilityBindingRows<'a> {
    input: &'a dyn PreparedRows,
    authority: BindingAuthority,
}

impl<'a> CompatibilityBindingRows<'a> {
    /// Selects the legacy representation before construction begins.
    pub fn new(input: &'a dyn PreparedRows) -> ContentResult<Self> {
        Ok(Self {
            input,
            authority: BindingAuthority::new()?,
        })
    }
}

impl BindingRows for CompatibilityBindingRows<'_> {
    fn directory_headers(&self) -> ContentResult<Box<dyn DirectoryHeaderSource + '_>> {
        Ok(Box::new(CompatibilityHeaders {
            authority: &self.authority,
            rows: self.input.directories()?,
            failure: None,
            eof: false,
        }))
    }

    fn directory_header(&self, parent: u64) -> ContentResult<Option<DirectoryHeader>> {
        self.input
            .directory_for(parent)?
            .map(|row| {
                if row.parent != parent {
                    return Err(ContentError::InvalidRecord("directory selection"));
                }
                header(&self.authority, parent, &row)
            })
            .transpose()
    }

    fn bindings(
        &self,
        selected: &DirectoryHeader,
    ) -> ContentResult<Box<dyn BindingRowSource + '_>> {
        if !self.authority.accepts(selected) {
            return Err(ContentError::InvalidRecord("directory issuer"));
        }
        if selected.ordinal() != selected.parent() {
            return Err(ContentError::InvalidRecord("directory descriptor"));
        }
        let row = self
            .input
            .directory_for(selected.parent())?
            .ok_or(ContentError::InvalidRecord("directory selection"))?;
        if header(&self.authority, row.parent, &row)? != *selected {
            return Err(ContentError::InvalidRecord("directory selection"));
        }
        Ok(Box::new(ResidentBindings::owned(*selected, row.changes)))
    }

    fn binding_for(&self, parent: u64, name: &[u8]) -> ContentResult<BindingLookup> {
        self.input.legacy_binding_lookup(parent, name)
    }
}

impl RowSource for CompatibilityBindingRows<'_> {
    fn directory_rows(&self) -> usize {
        self.input.directory_rows()
    }
    fn inode_rows(&self) -> usize {
        self.input.inode_rows()
    }
    fn new_rows(&self) -> usize {
        self.input.new_rows()
    }
    fn directories(&self) -> ContentResult<Box<dyn DirectoryRowSource + '_>> {
        self.input.directories()
    }
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>> {
        self.input.inodes()
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        self.input.new_inodes()
    }
    fn directory_for(&self, parent: u64) -> ContentResult<Option<DirectoryUpdate>> {
        self.input.directory_for(parent)
    }
    fn legacy_binding_lookup(&self, parent: u64, name: &[u8]) -> ContentResult<BindingLookup> {
        self.input.legacy_binding_lookup(parent, name)
    }
    fn value_for(&self, serial: u64) -> ContentResult<Option<InodeValue>> {
        self.input.value_for(serial)
    }
    fn new_position(&self, serial: u64) -> ContentResult<Option<usize>> {
        self.input.new_position(serial)
    }
}

impl PreparedRows for CompatibilityBindingRows<'_> {
    fn base(&self) -> Option<FilesystemRootId> {
        self.input.base()
    }
    fn scope(&self) -> InodeScope {
        self.input.scope()
    }
    fn root_serial(&self) -> u64 {
        self.input.root_serial()
    }
    fn resources(&self) -> FilesystemResources {
        self.input.resources()
    }
}

struct CompatibilityHeaders<'a> {
    authority: &'a BindingAuthority,
    rows: Box<dyn DirectoryRowSource + 'a>,
    failure: Option<ContentError>,
    eof: bool,
}

impl DirectoryHeaderSource for CompatibilityHeaders<'_> {
    fn next_header(&mut self) -> ContentResult<Option<DirectoryHeader>> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if self.eof {
            return Ok(None);
        }
        let result = self.rows.next_row().and_then(|row| {
            row.map(|row| header(self.authority, row.parent, &row))
                .transpose()
        });
        match &result {
            Ok(Some(_)) => {}
            Ok(None) => self.eof = true,
            Err(error) => self.failure = Some(error.clone()),
        }
        result
    }
}

fn header(
    authority: &BindingAuthority,
    descriptor: u64,
    row: &DirectoryUpdate,
) -> ContentResult<DirectoryHeader> {
    authority.header(
        row.parent,
        descriptor,
        u32::try_from(row.changes.len()).map_err(|_| ContentError::LengthOverflow)?,
        binding_bytes(&row.changes)?,
    )
}
