//! External captured resident fixture adapter. No production provider is selected.
use layerfs_content::filesystem::rows::RowSource;
use layerfs_content::filesystem::{
    DirectoryChangeLookup, DirectoryChangeSource, DirectoryHeader, DirectoryHeaderSource,
    DirectoryUpdate, FilesystemInput, InodeRowSource, PathName, SerialRowSource,
    StreamedFilesystemInput, StreamedRowSource,
};
use layerfs_content::object::inode_leaf::InodeValue;
use layerfs_content::ContentResult;
struct Headers<'a>(std::slice::Iter<'a, DirectoryUpdate>);
impl DirectoryHeaderSource for Headers<'_> {
    fn next_row(&mut self) -> ContentResult<Option<DirectoryHeader>> {
        Ok(self.0.next().map(|row| DirectoryHeader {
            parent: row.parent,
            change_rows: row.changes.len() as u64,
        }))
    }
}
struct Changes<'a>(std::slice::Iter<'a, (PathName, Option<u64>)>);
impl DirectoryChangeSource for Changes<'_> {
    fn next_row(&mut self) -> ContentResult<Option<(PathName, Option<u64>)>> {
        Ok(self.0.next().cloned())
    }
}
pub struct Rows<'a>(pub &'a FilesystemInput<'a>);
impl StreamedRowSource for Rows<'_> {
    fn directory_rows(&self) -> usize {
        self.0.directories.len()
    }
    fn inode_rows(&self) -> usize {
        self.0.inodes.len()
    }
    fn new_rows(&self) -> usize {
        self.0.new_inodes.len()
    }
    fn directory_headers(&self) -> ContentResult<Box<dyn DirectoryHeaderSource + '_>> {
        Ok(Box::new(Headers(self.0.directories.iter())))
    }
    fn directory_header(&self, parent: u64) -> ContentResult<Option<DirectoryHeader>> {
        Ok(self
            .0
            .directories
            .iter()
            .find(|row| row.parent == parent)
            .map(|row| DirectoryHeader {
                parent,
                change_rows: row.changes.len() as u64,
            }))
    }
    fn directory_changes(&self, parent: u64) -> ContentResult<Box<dyn DirectoryChangeSource + '_>> {
        let row = self
            .0
            .directories
            .iter()
            .find(|row| row.parent == parent)
            .expect("sealed header");
        Ok(Box::new(Changes(row.changes.iter())))
    }
    fn directory_change(
        &self,
        parent: u64,
        name: &PathName,
    ) -> ContentResult<DirectoryChangeLookup> {
        Ok(self
            .0
            .directories
            .iter()
            .find(|row| row.parent == parent)
            .and_then(|row| row.changes.iter().find(|(key, _)| key == name))
            .map_or(DirectoryChangeLookup::Unchanged, |(_, binding)| {
                binding.map_or(DirectoryChangeLookup::Removed, DirectoryChangeLookup::Bound)
            }))
    }
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>> {
        self.0.inodes()
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        self.0.new_inodes()
    }
    fn value_for(&self, serial: u64) -> ContentResult<Option<InodeValue>> {
        self.0.value_for(serial)
    }
    fn new_position(&self, serial: u64) -> ContentResult<Option<usize>> {
        self.0.new_position(serial)
    }
}
pub fn prepared<'a>(rows: &'a Rows<'a>) -> StreamedFilesystemInput<'a> {
    StreamedFilesystemInput {
        base: rows.0.base,
        scope: rows.0.scope,
        root_serial: rows.0.root_serial,
        resources: rows.0.resources,
        rows,
    }
}
