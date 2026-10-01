//! One owned prepared input: typed slots and streamed, indexed final names.
//!
//! Format2 retains the48-byte header and32-byte slots. Scalar slot fields replace
//! duplicated payload identities, and one immutable offset per16 names supports
//! exact lookup without replaying a whole directory prefix. All cursors use the
//! same live file; flush/seal provide visibility and exact shape, not durability
//! or a cryptographic/native tamper guarantee.

use std::cell::Cell;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use super::binding::BindingAuthority;
use super::declaration::{table_bytes, FORMAT_VERSION, HEADER_BYTES};
use super::spool_writer::DirectoryWrite;
use super::{
    BindingPoint, BindingRows, InodeRowSource, RowSource, SerialRowSource, SpoolDeclaration,
    SpoolPreparation,
};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::input::DirectoryUpdate;
use crate::object::inode_leaf::InodeValue;

/// Bytes one row occupies in the fixed typed slot table.
pub const SPOOL_SLOT_BYTES: u64 = 32;
pub(super) const KIND_DIRECTORY: u8 = 1;
pub(super) const KIND_INODE: u8 = 2;
pub(super) const KIND_SERIAL: u8 = 3;

/// Application read work of the actual spool, distinct from kernel/cache work.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SpoolReadWork {
    /// Absolute read_exact requests attempted, including failed I/O.
    pub read_requests: u64,
    /// Bytes acknowledged by successful read_exact requests.
    pub read_bytes: u64,
    /// Fixed typed slots requested.
    pub slot_reads: u64,
    /// Checkpoint offsets read by traversal or point lookup.
    pub checkpoint_reads: u64,
    /// Full-name checkpoints compared by a point lookup.
    pub checkpoint_probes: u64,
    /// Binding records decoded by traversal or point lookup.
    pub bindings_decoded: u64,
}

/// One first-party private file containing a prepared update.
pub struct RowSpool {
    pub(super) path: PathBuf,
    pub(super) file: File,
    pub(super) directory_rows: usize,
    pub(super) inode_rows: usize,
    pub(super) new_rows: usize,
    pub(super) written: [usize; 3],
    pub(super) bytes: u64,
    pub(super) table_end: u64,
    pub(super) directory_end: u64,
    pub(super) capacity: u64,
    pub(super) sealed: bool,
    pub(super) last: [u64; 4],
    pub(super) authority: BindingAuthority,
    pub(super) declaration: Option<SpoolDeclaration>,
    pub(super) bindings: u64,
    pub(super) wire_name_bytes: u64,
    pub(super) active: Option<DirectoryWrite>,
    pub(super) failure: Option<ContentError>,
    pub(super) work: Cell<SpoolReadWork>,
}

impl RowSpool {
    /// Explicit compatibility creation with row counts but undeclared name totals.
    /// Actual name totals are accumulated and written at seal; they are not guessed.
    pub fn create(
        path: PathBuf,
        directory_rows: usize,
        inode_rows: usize,
        new_rows: usize,
        capacity: u64,
    ) -> ContentResult<Self> {
        Self::create_inner(
            path,
            directory_rows,
            inode_rows,
            new_rows,
            capacity,
            None,
            None,
        )
    }

    /// Admits the complete declared shape before file creation or any write.
    pub fn create_declared(
        path: PathBuf,
        declaration: SpoolDeclaration,
        capacity: u64,
    ) -> ContentResult<Self> {
        Self::create_prepared(path, SpoolPreparation::new(declaration, capacity)?)
    }

    /// Moves the already admitted source authority into this exact private file.
    pub fn create_prepared(path: PathBuf, preparation: SpoolPreparation) -> ContentResult<Self> {
        let SpoolPreparation {
            declaration,
            capacity,
            authority,
        } = preparation;
        Self::create_inner(
            path,
            declaration.directories,
            declaration.inodes,
            declaration.fresh,
            capacity,
            Some(declaration),
            Some(authority),
        )
    }

    fn create_inner(
        path: PathBuf,
        directory_rows: usize,
        inode_rows: usize,
        new_rows: usize,
        capacity: u64,
        declaration: Option<SpoolDeclaration>,
        authority: Option<BindingAuthority>,
    ) -> ContentResult<Self> {
        let table_end = table_bytes(directory_rows, inode_rows, new_rows)?;
        if table_end > capacity {
            return Err(ContentError::ResourceUnavailable {
                what: "prepared row spool",
            });
        }
        let authority = match authority {
            Some(authority) => authority,
            None => BindingAuthority::new()?,
        };
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| ContentError::Io)?;
        let mut spool = Self {
            path,
            file,
            directory_rows,
            inode_rows,
            new_rows,
            written: [0; 3],
            bytes: table_end,
            table_end,
            directory_end: table_end,
            capacity,
            sealed: false,
            last: [0; 4],
            authority,
            declaration,
            bindings: 0,
            wire_name_bytes: 0,
            active: None,
            failure: None,
            work: Cell::new(SpoolReadWork::default()),
        };
        let header = spool.header_bytes(false)?;
        spool.write_at(0, &header)?;
        spool
            .file
            .set_len(table_end)
            .map_err(|_| ContentError::Io)?;
        Ok(spool)
    }

    /// Path of the owned private file; no reopen/adoption API is provided.
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Acknowledged bytes including the reserved slot table and active region.
    pub fn held_bytes(&self) -> u64 {
        self.bytes
    }
    /// Acknowledged directory rows, excluding an unfinished streaming row.
    pub fn written_directories(&self) -> usize {
        self.written[0]
    }
    /// Acknowledged typed values.
    pub fn written_inodes(&self) -> usize {
        self.written[1]
    }
    /// Acknowledged fresh serials.
    pub fn written_new(&self) -> usize {
        self.written[2]
    }
    /// Read-work snapshot; differences establish selected-phase application work.
    /// These are not kernel syscall, storage-read or physical-residency counters.
    pub fn read_work(&self) -> SpoolReadWork {
        self.work.get()
    }
    /// True when every row was acknowledged and no streaming row is unfinished.
    pub fn is_complete(&self) -> bool {
        self.active.is_none()
            && self.failure.is_none()
            && self.written == [self.directory_rows, self.inode_rows, self.new_rows]
            && self.declaration.is_none_or(|d| {
                d.bindings == self.bindings && d.wire_name_bytes == self.wire_name_bytes
            })
    }

    /// Narrow receive-time presence check for an acknowledged directory slot.
    /// It does not read names, require global seal, or expose a partial row.
    pub fn completed_directory(&self, parent: u64) -> ContentResult<bool> {
        self.ensure_known()?;
        Ok(self.find(KIND_DIRECTORY, parent)?.is_some())
    }

    /// Confirms exact acknowledged shape/layout, then flushes ordinary visibility.
    pub fn seal(&mut self) -> ContentResult<()> {
        self.ensure_known()?;
        if self.sealed {
            return Err(ContentError::InvalidRecord("spool already sealed"));
        }
        let result = self.seal_once();
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }

    fn seal_once(&mut self) -> ContentResult<()> {
        if !self.is_complete() {
            return Err(ContentError::InvalidRecord("prepared row count"));
        }
        self.validate_layout()?;
        self.check_header()?;
        let header = self.header_bytes(true)?;
        self.write_at(0, &header)?;
        self.file.flush().map_err(|_| ContentError::Io)?;
        self.sealed = true;
        Ok(())
    }

    /// Removes the owned legacy private file with a checked result.
    /// Its retained Drop retry/native authority limitation is unchanged.
    pub fn cleanup(&mut self) -> ContentResult<()> {
        if self.path.as_os_str().is_empty() {
            return Ok(());
        }
        std::fs::remove_file(&self.path).map_err(|_| ContentError::Io)?;
        self.path.clear();
        Ok(())
    }

    pub(super) fn ensure_known(&self) -> ContentResult<()> {
        if self.path.as_os_str().is_empty() {
            return Err(ContentError::IncompleteOperation);
        }
        match &self.failure {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }

    pub(super) fn ensure_sealed(&self) -> ContentResult<()> {
        self.ensure_known()?;
        if !self.sealed {
            return Err(ContentError::IncompleteOperation);
        }
        self.check_header()
    }

    pub(super) fn ensure_writable(&self) -> ContentResult<()> {
        self.ensure_known()?;
        if self.sealed {
            return Err(ContentError::InvalidRecord("spool sealed"));
        }
        Ok(())
    }

    pub(super) fn write_at(&mut self, at: u64, bytes: &[u8]) -> ContentResult<()> {
        self.file
            .seek(SeekFrom::Start(at))
            .and_then(|_| self.file.write_all(bytes))
            .map_err(|_| ContentError::Io)
    }

    pub(super) fn read_at(&self, at: u64, bytes: &mut [u8]) -> ContentResult<()> {
        let end = at
            .checked_add(bytes.len() as u64)
            .ok_or(ContentError::LengthOverflow)?;
        if end > self.bytes {
            return Err(ContentError::InvalidRecord("spool read span"));
        }
        let mut work = self.work.get();
        work.read_requests = work.read_requests.saturating_add(1);
        self.work.set(work);
        let mut file = &self.file;
        file.seek(SeekFrom::Start(at))
            .and_then(|_| file.read_exact(bytes))
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::UnexpectedEof {
                    ContentError::UnexpectedEof
                } else {
                    ContentError::Io
                }
            })?;
        let mut work = self.work.get();
        work.read_bytes = work.read_bytes.saturating_add(bytes.len() as u64);
        self.work.set(work);
        Ok(())
    }

    fn header_bytes(&self, actual: bool) -> ContentResult<[u8; 48]> {
        let (bindings, name_bytes) = if actual {
            (self.bindings, self.wire_name_bytes)
        } else {
            self.declaration
                .map_or((0, 0), |d| (d.bindings, d.wire_name_bytes))
        };
        let values = [
            u64::try_from(self.directory_rows).map_err(|_| ContentError::LengthOverflow)?,
            u64::try_from(self.inode_rows).map_err(|_| ContentError::LengthOverflow)?,
            u64::try_from(self.new_rows).map_err(|_| ContentError::LengthOverflow)?,
            bindings,
            name_bytes,
            FORMAT_VERSION,
        ];
        let mut header = [0; HEADER_BYTES as usize];
        for (index, value) in values.into_iter().enumerate() {
            header[index * 8..index * 8 + 8].copy_from_slice(&value.to_be_bytes());
        }
        Ok(header)
    }

    pub(super) fn check_header(&self) -> ContentResult<()> {
        let mut header = [0; HEADER_BYTES as usize];
        self.read_at(0, &mut header)?;
        if header != self.header_bytes(self.sealed)? {
            return Err(ContentError::InvalidRecord("spool header"));
        }
        if self.file.metadata().map_err(|_| ContentError::Io)?.len() != self.bytes {
            return Err(ContentError::InvalidRecord("spool file length"));
        }
        Ok(())
    }
}

impl RowSource for RowSpool {
    fn legacy_binding_at(
        &self,
        parent: u64,
        ordinal: u32,
    ) -> ContentResult<(crate::filesystem::path::PathName, Option<u64>)> {
        self.ensure_sealed()?;
        let (index, slot) = self
            .find(KIND_DIRECTORY, parent)?
            .ok_or(ContentError::InvalidRecord("directory selection"))?;
        let header = self.slot_header(index, &slot)?;
        self.binding_at(&BindingPoint::new(&header, ordinal)?)
    }
    fn directory_rows(&self) -> usize {
        self.directory_rows
    }
    fn inode_rows(&self) -> usize {
        self.inode_rows
    }
    fn new_rows(&self) -> usize {
        self.new_rows
    }
    fn directories(&self) -> ContentResult<Box<dyn super::DirectoryRowSource + '_>> {
        self.ensure_sealed()?;
        Ok(Box::new(super::spool_compatibility::SpoolDirectories::new(
            self,
        )))
    }
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>> {
        self.ensure_sealed()?;
        Ok(Box::new(super::spool_slots::SpoolInodes::new(self)))
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        self.ensure_sealed()?;
        Ok(Box::new(super::spool_slots::SpoolSerials::new(self)))
    }
    fn directory_for(&self, parent: u64) -> ContentResult<Option<DirectoryUpdate>> {
        self.compatibility_directory(parent)
    }
    fn legacy_binding_lookup(
        &self,
        parent: u64,
        name: &[u8],
    ) -> ContentResult<super::BindingLookup> {
        super::BindingRows::binding_for(self, parent, name)
    }
    fn value_for(&self, serial: u64) -> ContentResult<Option<InodeValue>> {
        self.ensure_sealed()?;
        self.find(KIND_INODE, serial)?
            .map(|(_, slot)| self.inode(&slot).map(|row| row.value))
            .transpose()
    }
    fn new_position(&self, serial: u64) -> ContentResult<Option<usize>> {
        self.ensure_sealed()?;
        Ok(self.find(KIND_SERIAL, serial)?.map(|(index, _)| index))
    }
}

impl Drop for RowSpool {
    fn drop(&mut self) {
        if !self.path.as_os_str().is_empty() {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
