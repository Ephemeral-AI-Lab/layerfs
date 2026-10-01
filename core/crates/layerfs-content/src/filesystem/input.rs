//! Native filesystem inputs: final bindings, typed values and declared resources.
//!
//! The caller supplies stable logical changes and authorized identities; C1
//! validates and consumes them. A directory update lists the *final* binding of
//! every name it changes, strictly sorted and unique, so `None` means "this name
//! is absent from the new directory" rather than "delete and hope". An inode
//! update carries a typed final value. A `new_inodes` entry declares a serial the
//! caller's allocator just created: the operation derives that inode's count from
//! the bindings it actually retains and never trusts a caller-supplied count.
//!
//! **Allocator precondition.** A declared new serial must never have been exposed
//! before: reusing an exposed serial would reinterpret retained records. C1 checks
//! that the serial has no base record, but uniqueness across the caller's lifetime
//! is the allocator's contract, exactly as the reference allocator's durable burn
//! of an exposed range is its own (and not a reason to add durability here).

use crate::error::{ContentError, ContentResult};
use crate::filesystem::identity::InodeScope;
use crate::filesystem::path::PathName;
use crate::filesystem::root::FilesystemRootId;
use crate::filesystem::rows::{
    BindingLookup, DirectoryRowSource, InodeRowSource, PreparedRows, RowSource, SerialRowSource,
    SliceDirectoryRows, SliceInodeRows, SliceSerialRows,
};
use crate::object::inode_leaf::InodeValue;

/// One directory's final binding changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryUpdate {
    /// Serial of the directory whose bindings change.
    pub parent: u64,
    /// Final bindings, strictly sorted by name and unique.
    pub changes: Vec<(PathName, Option<u64>)>,
}

impl DirectoryUpdate {
    /// Checks the order and uniqueness of this update's bindings.
    pub fn check(&self) -> ContentResult<()> {
        if self.parent == 0 {
            return Err(ContentError::InvalidRecord("directory parent"));
        }
        if self.changes.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
            return Err(ContentError::NonCanonicalOrdering);
        }
        Ok(())
    }
}

/// One typed final inode value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InodeUpdate {
    /// Inode serial.
    pub serial: u64,
    /// Final typed value. Its reference count is derived by the operation.
    pub value: InodeValue,
}

/// Resource ceilings of one operation, all supplied by the caller.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FilesystemResources {
    /// Bytes the sorted engine may hold for its own unfinished pages.
    pub scratch_bytes: usize,
    /// Ordering rows the reference reducer may hold in memory.
    pub maximum_pending_records: usize,
    /// Bytes one ordering merge buffer may use.
    pub merge_buffer_bytes: usize,
    /// Base inode records one read wave may request.
    pub base_read_batch: usize,
    /// Bytes of ordering rows this operation may own across its bounded pending
    /// map, its live runs and the output a merge is about to create.
    pub ordering_bytes: u64,
}

impl Default for FilesystemResources {
    fn default() -> Self {
        Self {
            scratch_bytes: crate::filesystem::sorted::MAXIMUM_SCRATCH_BYTES,
            maximum_pending_records: crate::filesystem::references::reduce::DEFAULT_MAXIMUM_PENDING,
            merge_buffer_bytes: crate::filesystem::references::runs::DEFAULT_MERGE_BUFFER_BYTES,
            base_read_batch: crate::filesystem::references::reduce::DEFAULT_BASE_BATCH,
            ordering_bytes: crate::filesystem::references::runs::DEFAULT_ORDERING_BYTES,
        }
    }
}

impl FilesystemResources {
    /// Serials the operation may collect while it derives final counts.
    ///
    /// The collection is one `(u64, PendingState)` per touched inode — the serial
    /// and the state carried out of the visit that read it — and it is taken from
    /// the same declared ordering budget as the pending rows and the runs. A
    /// caller that declares fewer ordering bytes therefore declares a smaller
    /// touched set, and an operation whose touched set does not fit is refused
    /// instead of allocating past its own ceiling.
    pub fn maximum_touched_serials(&self) -> usize {
        match usize::try_from(self.ordering_bytes / 16) {
            Ok(serials) => serials,
            Err(_) => usize::MAX,
        }
    }

    /// Checks that every declared ceiling is usable.
    pub fn check(&self) -> ContentResult<()> {
        if self.scratch_bytes < 1024 {
            return Err(ContentError::ResourceUnavailable {
                what: "operation scratch",
            });
        }
        if self.maximum_pending_records == 0 {
            return Err(ContentError::ResourceUnavailable {
                what: "ordering pending records",
            });
        }
        if self.merge_buffer_bytes < crate::filesystem::references::record::ROW_BYTES {
            return Err(ContentError::ResourceUnavailable {
                what: "ordering merge buffer",
            });
        }
        if self.base_read_batch == 0 {
            return Err(ContentError::ResourceUnavailable {
                what: "base read batch",
            });
        }
        if self.ordering_bytes < crate::filesystem::references::record::ROW_BYTES as u64 {
            return Err(ContentError::ResourceUnavailable {
                what: "ordering bytes",
            });
        }
        Ok(())
    }
}

/// One complete filesystem operation request.
pub struct FilesystemInput<'a> {
    /// Checked immutable base root; `None` builds a new filesystem.
    pub base: Option<FilesystemRootId>,
    /// Allocation scope of every identity in the result.
    pub scope: InodeScope,
    /// Root directory serial. For a build, it must be one of the supplied values.
    pub root_serial: u64,
    /// Final directory bindings, ordered by parent serial.
    pub directories: &'a [DirectoryUpdate],
    /// Typed final inode values, ordered by serial.
    pub inodes: &'a [InodeUpdate],
    /// Serials the caller's allocator just created, ordered and unique.
    pub new_inodes: &'a [u64],
    /// Declared resource ceilings.
    pub resources: FilesystemResources,
}

impl RowSource for FilesystemInput<'_> {
    fn legacy_binding_at(
        &self,
        parent: u64,
        ordinal: u32,
    ) -> ContentResult<(PathName, Option<u64>)> {
        let index = self
            .directories
            .binary_search_by_key(&parent, |row| row.parent)
            .map_err(|_| ContentError::InvalidRecord("directory selection"))?;
        crate::filesystem::rows::ordinal_binding(&self.directories[index], ordinal)
    }
    fn directory_rows(&self) -> usize {
        self.directories.len()
    }
    fn inode_rows(&self) -> usize {
        self.inodes.len()
    }
    fn new_rows(&self) -> usize {
        self.new_inodes.len()
    }
    fn directories(&self) -> ContentResult<Box<dyn DirectoryRowSource + '_>> {
        Ok(Box::new(SliceDirectoryRows::new(self.directories)))
    }
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>> {
        Ok(Box::new(SliceInodeRows::new(self.inodes)))
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        Ok(Box::new(SliceSerialRows::new(self.new_inodes)))
    }
    fn directory_for(&self, parent: u64) -> ContentResult<Option<DirectoryUpdate>> {
        Ok(self
            .directories
            .binary_search_by_key(&parent, |update| update.parent)
            .ok()
            .map(|index| self.directories[index].clone()))
    }
    fn legacy_binding_lookup(&self, parent: u64, name: &[u8]) -> ContentResult<BindingLookup> {
        let row = self
            .directories
            .binary_search_by_key(&parent, |row| row.parent)
            .ok()
            .map(|index| &self.directories[index]);
        crate::filesystem::rows::lookup_binding(row, name)
    }
    fn value_for(&self, serial: u64) -> ContentResult<Option<InodeValue>> {
        Ok(self
            .inodes
            .binary_search_by_key(&serial, |update| update.serial)
            .ok()
            .map(|index| self.inodes[index].value))
    }
    fn new_position(&self, serial: u64) -> ContentResult<Option<usize>> {
        Ok(self.new_inodes.binary_search(&serial).ok())
    }
}

impl PreparedRows for FilesystemInput<'_> {
    fn base(&self) -> Option<FilesystemRootId> {
        self.base
    }
    fn scope(&self) -> InodeScope {
        self.scope
    }
    fn root_serial(&self) -> u64 {
        self.root_serial
    }
    fn resources(&self) -> FilesystemResources {
        self.resources
    }
}
