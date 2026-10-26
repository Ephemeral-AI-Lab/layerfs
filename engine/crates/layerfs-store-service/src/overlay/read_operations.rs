//! Direct connection operations, each one counted short job on the calling thread.
use crate::{OwnerClient, OwnerError, ServiceClass};
use layerfs_overlay::{BaseSource, CapturedReader, FileRead};
use layerfs_workspace::Route;
impl OwnerClient {
    pub fn acquire_diagnostic_source(
        &self,
        route: layerfs_workspace::Route,
    ) -> Result<BaseSource, OwnerError> {
        self.call(route, crate::ServiceClass::Source, None, move |db| {
            db.acquire_diagnostic_source()
        })
    }

    pub fn reader_inodes(
        &self,
        route: Route,
        reader: CapturedReader,
        after: u64,
    ) -> Result<Vec<layerfs_overlay::Inode>, OwnerError> {
        self.call(route, ServiceClass::General, None, move |db| {
            db.reader_inodes(reader, after)
        })
    }
    pub fn reader_directory_entries(
        &self,
        route: Route,
        reader: CapturedReader,
        after: Option<(u64, Vec<u8>)>,
    ) -> Result<Vec<layerfs_overlay::DirectoryEntry>, OwnerError> {
        self.call(route, ServiceClass::General, None, move |db| {
            db.reader_directory_entries(reader, after.as_ref().map(|(p, n)| (*p, n.as_slice())))
        })
    }
    pub fn reader_parent_directory_entries(
        &self,
        route: Route,
        reader: CapturedReader,
        parent: u64,
        after: Option<Vec<u8>>,
    ) -> Result<Vec<layerfs_overlay::DirectoryEntry>, OwnerError> {
        self.call(route, ServiceClass::General, None, move |db| {
            db.reader_parent_directory_entries(reader, parent, after.as_deref())
        })
    }
    pub fn reader_directory_entry(
        &self,
        route: Route,
        reader: CapturedReader,
        parent: u64,
        name: Vec<u8>,
    ) -> Result<Option<layerfs_overlay::DirectoryEntry>, OwnerError> {
        self.call(route, ServiceClass::General, None, move |db| {
            db.reader_directory_entry(reader, parent, &name)
        })
    }
    pub fn reader_symlink(
        &self,
        route: Route,
        reader: CapturedReader,
        serial: u64,
    ) -> Result<Vec<u8>, OwnerError> {
        self.call(route, ServiceClass::General, None, move |db| {
            db.reader_symlink(reader, serial)
        })
    }
    pub fn file_read(
        &self,
        route: Route,
        read: FileRead,
        offset: u64,
        length: u32,
    ) -> Result<Option<layerfs_overlay::LocalRead>, OwnerError> {
        self.call(route, ServiceClass::General, None, move |db| {
            db.read_file(read, offset, length)
        })
    }
    pub fn captured_read(
        &self,
        route: Route,
        reader: CapturedReader,
        serial: u64,
        offset: u64,
        length: u32,
    ) -> Result<Option<layerfs_overlay::LocalRead>, OwnerError> {
        self.call(route, ServiceClass::General, None, move |db| {
            db.read_captured(reader, serial, offset, length)
        })
    }
    pub fn captured_run(
        &self,
        route: Route,
        cursor: Box<layerfs_overlay::CapturedRunCursor>,
    ) -> Result<layerfs_overlay::CapturedRunReply, OwnerError> {
        self.call(route, ServiceClass::General, None, move |db| {
            db.captured_run_step(*cursor)
        })
    }
    pub fn reader_inode(
        &self,
        route: Route,
        reader: CapturedReader,
        serial: u64,
    ) -> Result<Option<layerfs_overlay::Inode>, OwnerError> {
        self.call(route, ServiceClass::General, None, move |db| {
            db.reader_inode(reader, serial)
        })
    }
    pub fn source_read(
        &self,
        route: Route,
        source: BaseSource,
        serial: u64,
        offset: u64,
        length: u32,
    ) -> Result<Option<layerfs_overlay::LocalRead>, OwnerError> {
        self.call(route, ServiceClass::General, None, move |db| {
            db.source_read(source, serial, offset, length)
        })
    }
    pub fn source_cell(
        &self,
        route: Route,
        source: BaseSource,
        serial: u64,
        generation: u64,
        offset: u64,
    ) -> Result<Option<layerfs_overlay::Cell>, OwnerError> {
        self.call(route, ServiceClass::General, None, move |db| {
            db.source_cell(source, serial, generation, offset)
        })
    }
    pub fn acquire_base_source(
        &self,
        route: Route,
        owner: u64,
    ) -> Result<layerfs_overlay::BaseSource, OwnerError> {
        self.call(route, ServiceClass::Source, None, move |db| {
            db.acquire_base_source(owner)
        })
    }
    pub fn source_inode(
        &self,
        route: Route,
        source: BaseSource,
        serial: u64,
    ) -> Result<Option<layerfs_overlay::Inode>, OwnerError> {
        self.call(route, ServiceClass::General, None, move |db| {
            db.source_inode(source, serial)
        })
    }
    pub fn source_directory_entry(
        &self,
        route: Route,
        source: BaseSource,
        parent: u64,
        name: Vec<u8>,
    ) -> Result<Option<layerfs_overlay::DirectoryEntry>, OwnerError> {
        self.call(route, ServiceClass::General, None, move |db| {
            db.source_directory_entry(source, parent, &name)
        })
    }
    pub fn source_directory_entries(
        &self,
        route: Route,
        source: BaseSource,
        parent: u64,
        after: Option<Vec<u8>>,
    ) -> Result<layerfs_overlay::DirectoryEntryWindow, OwnerError> {
        self.call(route, ServiceClass::General, None, move |db| {
            db.source_directory_entry_window(source, parent, after.as_deref())
        })
    }
}
