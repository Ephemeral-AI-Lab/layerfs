//! One private operation view for legacy resident rows and streamed directories.
use super::{
    DirectoryChangeLookup, DirectoryChangeSource, DirectoryHeader, DirectoryHeaderSource,
    DirectoryRowSource, InodeRowSource, PreparedDirectoryStreams, PreparedRows, SerialRowSource,
};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::{
    DirectoryUpdate, FilesystemResources, FilesystemRootId, InodeScope, PathName,
};
use crate::object::inode_leaf::InodeValue;
use std::{cell::RefCell, collections::BTreeMap};

pub(crate) trait OperationInput {
    fn base(&self) -> Option<FilesystemRootId>;
    fn scope(&self) -> InodeScope;
    fn root_serial(&self) -> u64;
    fn resources(&self) -> FilesystemResources;
    fn directory_rows(&self) -> usize;
    fn inode_rows(&self) -> usize;
    fn new_rows(&self) -> usize;
    fn directories(&self) -> ContentResult<Box<dyn OperationDirectories<'_> + '_>>;
    fn directory_for(&self, parent: u64) -> ContentResult<Option<DirectoryRow<'_>>>;
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>>;
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>>;
    fn value_for(&self, serial: u64) -> ContentResult<Option<InodeValue>>;
    fn new_position(&self, serial: u64) -> ContentResult<Option<usize>>;
    fn is_new(&self, serial: u64) -> ContentResult<bool> {
        Ok(self.new_position(serial)?.is_some())
    }
    fn binding_change(&self, parent: u64, name: &PathName) -> ContentResult<DirectoryChangeLookup> {
        match self.directory_for(parent)? {
            Some(row) => row.binding_for(name),
            None => Ok(DirectoryChangeLookup::Unchanged),
        }
    }
    fn prepare_binding_points(&self) -> ContentResult<()> {
        Ok(())
    }
    fn clear_binding_points(&self) {}
}

pub(crate) trait OperationDirectories<'a> {
    fn next_row(&mut self) -> ContentResult<Option<DirectoryRow<'a>>>;
}

type ChangeIterator<'a> = Box<dyn Iterator<Item = ContentResult<(PathName, Option<u64>)>> + 'a>;

enum Changes<'a> {
    Resident(Vec<(PathName, Option<u64>)>),
    Streamed(&'a dyn PreparedDirectoryStreams),
}
pub(crate) struct DirectoryRow<'a> {
    pub header: DirectoryHeader,
    changes: Changes<'a>,
}
impl DirectoryRow<'_> {
    fn resident(row: DirectoryUpdate) -> Self {
        Self {
            header: DirectoryHeader {
                parent: row.parent,
                change_rows: row.changes.len() as u64,
            },
            changes: Changes::Resident(row.changes),
        }
    }
    pub fn binding_for(&self, name: &PathName) -> ContentResult<DirectoryChangeLookup> {
        let result = match &self.changes {
            Changes::Resident(changes) => changes
                .binary_search_by(|(key, _)| key.cmp(name))
                .ok()
                .map_or(DirectoryChangeLookup::Unchanged, |index| {
                    match changes[index].1 {
                        Some(serial) => DirectoryChangeLookup::Bound(serial),
                        None => DirectoryChangeLookup::Removed,
                    }
                }),
            Changes::Streamed(source) => source.directory_change(self.header.parent, name)?,
        };
        if matches!(result, DirectoryChangeLookup::Bound(serial) if !super::serial_in_range(serial))
        {
            return Err(ContentError::InvalidRecord("inode serial"));
        }
        Ok(result)
    }
    pub fn changes(&self) -> ContentResult<ChangeIterator<'_>> {
        let source = match &self.changes {
            Changes::Resident(changes) => return Ok(Box::new(changes.iter().cloned().map(Ok))),
            Changes::Streamed(source) => source.directory_changes(self.header.parent)?,
        };
        Ok(Box::new(CheckedChanges {
            source,
            expected: self.header.change_rows,
            seen: 0,
            previous: None,
            ended: false,
        }))
    }
    pub fn check_header(&self) -> ContentResult<()> {
        if self.header.parent == 0 {
            return Err(ContentError::InvalidRecord("directory parent"));
        }
        if let Changes::Streamed(source) = &self.changes {
            if source.directory_header(self.header.parent)? != Some(self.header) {
                return Err(ContentError::InvalidRecord("directory header lookup"));
            }
        } else if let Changes::Resident(changes) = &self.changes {
            if changes.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
                return Err(ContentError::NonCanonicalOrdering);
            }
            if changes
                .iter()
                .any(|(_, binding)| binding.is_some_and(|serial| !super::serial_in_range(serial)))
            {
                return Err(ContentError::InvalidRecord("inode serial"));
            }
        }
        Ok(())
    }
    pub fn check_point(&self, name: &PathName, binding: Option<u64>) -> ContentResult<()> {
        if matches!(&self.changes, Changes::Streamed(_)) {
            let expected =
                binding.map_or(DirectoryChangeLookup::Removed, DirectoryChangeLookup::Bound);
            if self.binding_for(name)? != expected {
                return Err(ContentError::InvalidRecord("directory change lookup"));
            }
        }
        Ok(())
    }
}

struct CheckedChanges<'a> {
    source: Box<dyn DirectoryChangeSource + 'a>,
    expected: u64,
    seen: u64,
    previous: Option<PathName>,
    ended: bool,
}
impl Iterator for CheckedChanges<'_> {
    type Item = ContentResult<(PathName, Option<u64>)>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.ended {
            return None;
        }
        let row = self.source.next_row();
        let result = match row {
            Err(error) => Err(error),
            Ok(None) => {
                self.ended = true;
                return (self.seen != self.expected).then_some(Err(ContentError::InvalidRecord(
                    "directory change row count",
                )));
            }
            Ok(Some((name, binding))) => {
                if self.seen >= self.expected {
                    Err(ContentError::InvalidRecord("directory change row count"))
                } else if self
                    .previous
                    .as_ref()
                    .is_some_and(|previous| previous >= &name)
                {
                    Err(ContentError::NonCanonicalOrdering)
                } else if binding.is_some_and(|serial| !super::serial_in_range(serial)) {
                    Err(ContentError::InvalidRecord("inode serial"))
                } else {
                    self.seen += 1;
                    self.previous = Some(name.clone());
                    Ok((name, binding))
                }
            }
        };
        if result.is_err() {
            self.ended = true;
        }
        Some(result)
    }
}

type BindingPoints = BTreeMap<u64, BTreeMap<PathName, Option<u64>>>;
pub(crate) struct ResidentInput<'a> {
    input: &'a dyn PreparedRows,
    points: RefCell<Option<BindingPoints>>,
}
impl<'a> ResidentInput<'a> {
    pub fn new(input: &'a dyn PreparedRows) -> Self {
        Self {
            input,
            points: RefCell::new(None),
        }
    }
}
struct ResidentDirectories<'a> {
    rows: Box<dyn DirectoryRowSource + 'a>,
}
impl<'a> OperationDirectories<'a> for ResidentDirectories<'a> {
    fn next_row(&mut self) -> ContentResult<Option<DirectoryRow<'a>>> {
        Ok(self.rows.next_row()?.map(DirectoryRow::resident))
    }
}
impl OperationInput for ResidentInput<'_> {
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
    fn directory_rows(&self) -> usize {
        self.input.directory_rows()
    }
    fn inode_rows(&self) -> usize {
        self.input.inode_rows()
    }
    fn new_rows(&self) -> usize {
        self.input.new_rows()
    }
    fn directories(&self) -> ContentResult<Box<dyn OperationDirectories<'_> + '_>> {
        Ok(Box::new(ResidentDirectories {
            rows: self.input.directories()?,
        }))
    }
    fn directory_for(&self, parent: u64) -> ContentResult<Option<DirectoryRow<'_>>> {
        Ok(self
            .input
            .directory_for(parent)?
            .map(DirectoryRow::resident))
    }
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>> {
        self.input.inodes()
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        self.input.new_inodes()
    }
    fn value_for(&self, serial: u64) -> ContentResult<Option<InodeValue>> {
        self.input.value_for(serial)
    }
    fn new_position(&self, serial: u64) -> ContentResult<Option<usize>> {
        self.input.new_position(serial)
    }
    fn prepare_binding_points(&self) -> ContentResult<()> {
        // This is the old route's explicit resident changed-name map. Streamed
        // input never enters this adapter and never creates this materialization.
        let mut points = BTreeMap::new();
        let mut rows = self.input.directories()?;
        while let Some(row) = rows.next_row()? {
            points.insert(row.parent, row.changes.into_iter().collect());
        }
        *self.points.borrow_mut() = Some(points);
        Ok(())
    }
    fn clear_binding_points(&self) {
        let _ = self.points.borrow_mut().take();
    }
    fn binding_change(&self, parent: u64, name: &PathName) -> ContentResult<DirectoryChangeLookup> {
        let points = self.points.borrow();
        if let Some(points) = points.as_ref() {
            return Ok(points
                .get(&parent)
                .and_then(|names| names.get(name))
                .map_or(DirectoryChangeLookup::Unchanged, |binding| {
                    binding.map_or(DirectoryChangeLookup::Removed, DirectoryChangeLookup::Bound)
                }));
        }
        drop(points);
        match self.directory_for(parent)? {
            Some(row) => row.binding_for(name),
            None => Ok(DirectoryChangeLookup::Unchanged),
        }
    }
}

pub(crate) struct StreamedInput<'a> {
    input: &'a dyn PreparedDirectoryStreams,
}
impl<'a> StreamedInput<'a> {
    pub fn new(input: &'a dyn PreparedDirectoryStreams) -> Self {
        Self { input }
    }
}
struct StreamedDirectories<'a> {
    input: &'a dyn PreparedDirectoryStreams,
    headers: Box<dyn DirectoryHeaderSource + 'a>,
}
impl<'a> OperationDirectories<'a> for StreamedDirectories<'a> {
    fn next_row(&mut self) -> ContentResult<Option<DirectoryRow<'a>>> {
        Ok(self.headers.next_row()?.map(|header| DirectoryRow {
            header,
            changes: Changes::Streamed(self.input),
        }))
    }
}
impl OperationInput for StreamedInput<'_> {
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
    fn directory_rows(&self) -> usize {
        self.input.directory_rows()
    }
    fn inode_rows(&self) -> usize {
        self.input.inode_rows()
    }
    fn new_rows(&self) -> usize {
        self.input.new_rows()
    }
    fn directories(&self) -> ContentResult<Box<dyn OperationDirectories<'_> + '_>> {
        Ok(Box::new(StreamedDirectories {
            input: self.input,
            headers: self.input.directory_headers()?,
        }))
    }
    fn directory_for(&self, parent: u64) -> ContentResult<Option<DirectoryRow<'_>>> {
        let header = self.input.directory_header(parent)?;
        if header.is_some_and(|header| header.parent != parent) {
            return Err(ContentError::InvalidRecord("directory header lookup"));
        }
        Ok(header.map(|header| DirectoryRow {
            header,
            changes: Changes::Streamed(self.input),
        }))
    }
    fn binding_change(&self, parent: u64, name: &PathName) -> ContentResult<DirectoryChangeLookup> {
        let header = self.input.directory_header(parent)?;
        if header.is_some_and(|header| header.parent != parent) {
            return Err(ContentError::InvalidRecord("directory header lookup"));
        }
        let result = self.input.directory_change(parent, name)?;
        if header.is_none() && result != DirectoryChangeLookup::Unchanged {
            return Err(ContentError::InvalidRecord("directory change lookup"));
        }
        if matches!(result, DirectoryChangeLookup::Bound(serial) if !super::serial_in_range(serial))
        {
            return Err(ContentError::InvalidRecord("inode serial"));
        }
        Ok(result)
    }
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>> {
        self.input.inodes()
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        self.input.new_inodes()
    }
    fn value_for(&self, serial: u64) -> ContentResult<Option<InodeValue>> {
        self.input.value_for(serial)
    }
    fn new_position(&self, serial: u64) -> ContentResult<Option<usize>> {
        self.input.new_position(serial)
    }
}
