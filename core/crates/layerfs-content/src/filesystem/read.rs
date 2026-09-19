//! Bounded logical reads: resolve, stat, list, readlink and attribute lookup.
//!
//! One operation loads the checked root once, reuses its scope and inode table,
//! and descends each demand with the shared batch readers. A resolve walks one
//! path component at a time and never loads the whole inode table; listing is
//! bounded by count and bytes and resumes from the last delivered name.

use crate::error::{ContentError, ContentResult};
use crate::filesystem::attributes::keys::AttributeKey;
use crate::filesystem::attributes::portable::PortableMetadata;
use crate::filesystem::attributes::read::{read_portable, AttributeReadWork};
use crate::filesystem::directory::read::{
    list_after, lookup_counted as directory_lookup_counted, lookup_many as directory_lookup_many,
    DirectoryReadWork, ListingPage,
};
use crate::filesystem::inode::read::{
    lookup as inode_lookup, lookup_many, InodeReadWork, InodeTable,
};
use crate::filesystem::limits::MAXIMUM_ATTRIBUTE_KEYS;
use crate::filesystem::path::{LogicalPath, PathName};
use crate::filesystem::root::{FilesystemRoot, FilesystemRootId};
use crate::filesystem::symlink::SymlinkTarget;
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::object::{AuthenticatedObjects, ObjectId};

/// Work one logical read performed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FilesystemReadWork {
    /// Directory pages read.
    pub directory: DirectoryReadWork,
    /// Inode pages read.
    pub inode: InodeReadWork,
    /// Attribute pages and values read.
    pub attributes: AttributeReadWork,
}

/// One resolved inode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Resolved {
    /// Inode serial.
    pub serial: u64,
    /// Typed value.
    pub value: InodeValue,
}

/// One checked stat result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Stat {
    /// Inode kind.
    pub kind: InodeKind,
    /// Derived namespace reference count.
    pub namespace_ref_count: u64,
    /// Content root of the inode.
    pub content_root: ObjectId,
    /// Attribute root of the inode.
    pub metadata_root: ObjectId,
}

impl From<InodeValue> for Stat {
    fn from(value: InodeValue) -> Self {
        Self {
            kind: value.kind,
            namespace_ref_count: value.namespace_ref_count,
            content_root: value.content_root,
            metadata_root: value.metadata_root,
        }
    }
}

/// One bounded listing page.
pub use crate::filesystem::directory::read::ListingPage as DirectoryListing;

/// A checked filesystem root bound to its reader for one operation.
pub struct FilesystemRead<'a> {
    reader: &'a dyn AuthenticatedObjects,
    root: FilesystemRoot,
    work: FilesystemReadWork,
}

impl<'a> FilesystemRead<'a> {
    /// Loads and checks one root.
    pub fn new(
        reader: &'a dyn AuthenticatedObjects,
        root: FilesystemRootId,
    ) -> ContentResult<Self> {
        let canonical = reader.read_canonical(root.0)?;
        Ok(Self {
            reader,
            root: FilesystemRoot::decode(&canonical)?,
            work: FilesystemReadWork::default(),
        })
    }

    /// The checked root this reader walks.
    pub const fn root(&self) -> FilesystemRoot {
        self.root
    }

    /// Work performed so far.
    pub const fn work(&self) -> FilesystemReadWork {
        self.work
    }

    /// Resolves one canonical path to its inode.
    pub fn resolve(&mut self, path: &LogicalPath) -> ContentResult<Resolved> {
        let table = self.table();
        let mut serial = self.root.root_inode().serial();
        let mut value = self.inode(table, serial)?;
        for component in path.components() {
            if value.kind != InodeKind::Directory {
                return Err(ContentError::InvalidRecord(
                    "path component is not a directory",
                ));
            }
            let name = PathName::from_bytes(component)?;
            // The walk charges the directory pages it reads. Resolving a path is
            // directory work, and the uncounted `lookup` wrapper used to throw its
            // `DirectoryReadWork` away, so a reader could stat a whole path and
            // report `directory.pages_read = 0`.
            // A name no directory binds is a **logical** absence, not a provider
            // one. Reporting it as `MissingObject` made "this path does not
            // exist" and "the provider does not hold an object this tree names"
            // the same answer, which `object::access`, `cas::provider` and
            // `error::MissingObject` all say they must not be.
            serial = directory_lookup_counted(
                self.reader,
                crate::filesystem::sorted::finish::DirectoryRoot(value.content_root),
                &name,
                &mut self.work.directory,
            )?
            .map(|(_, serial)| serial)
            .ok_or(ContentError::PathNotFound)?;
            value = self.inode(table, serial)?;
        }
        Ok(Resolved { serial, value })
    }

    /// Stats one canonical path.
    pub fn stat(&mut self, path: &LogicalPath) -> ContentResult<Stat> {
        Ok(self.resolve(path)?.value.into())
    }

    /// Lists one directory, bounded by count and bytes.
    pub fn list(
        &mut self,
        path: &LogicalPath,
        after: Option<&PathName>,
        max_entries: usize,
        max_bytes: usize,
    ) -> ContentResult<ListingPage> {
        let resolved = self.resolve(path)?;
        if resolved.value.kind != InodeKind::Directory {
            return Err(ContentError::WrongLogicalRole);
        }
        let page = list_after(
            self.reader,
            crate::filesystem::sorted::finish::DirectoryRoot(resolved.value.content_root),
            after,
            max_entries,
            max_bytes,
            &mut self.work.directory,
        )?;
        Ok(page)
    }

    /// Resolves several names inside one directory, sharing each level's wave.
    pub fn lookup_names(
        &mut self,
        path: &LogicalPath,
        names: &[PathName],
    ) -> ContentResult<Vec<Option<u64>>> {
        let resolved = self.resolve(path)?;
        if resolved.value.kind != InodeKind::Directory {
            return Err(ContentError::WrongLogicalRole);
        }
        Ok(directory_lookup_many(
            self.reader,
            crate::filesystem::sorted::finish::DirectoryRoot(resolved.value.content_root),
            names,
            &mut self.work.directory,
        )?
        .into_iter()
        .collect())
    }

    /// Resolves several inode serials, sharing each level's wave.
    pub fn lookup_inodes(&mut self, serials: &[u64]) -> ContentResult<Vec<Option<InodeValue>>> {
        let table = self.table();
        lookup_many(self.reader, table, serials, &mut self.work.inode)
    }

    /// Reads one symlink's stored target.
    pub fn readlink(&mut self, path: &LogicalPath) -> ContentResult<SymlinkTarget> {
        let resolved = self.resolve(path)?;
        if resolved.value.kind != InodeKind::Symlink {
            return Err(ContentError::WrongLogicalRole);
        }
        let canonical = self.reader.read_canonical(resolved.value.content_root)?;
        SymlinkTarget::decode(&canonical)
    }

    /// Reads the typed portable mode and mtime of one inode.
    pub fn read_portable(&mut self, path: &LogicalPath) -> ContentResult<PortableMetadata> {
        let resolved = self.resolve(path)?;
        read_portable(
            self.reader,
            resolved.value.metadata_root,
            resolved.value.kind,
            &mut self.work.attributes,
        )
    }

    /// Reads one attribute value with an explicit byte bound.
    pub fn read_attribute(
        &mut self,
        path: &LogicalPath,
        key: &AttributeKey,
        maximum_bytes: usize,
    ) -> ContentResult<Option<Vec<u8>>> {
        let resolved = self.resolve(path)?;
        crate::filesystem::attributes::read::read_opaque(
            self.reader,
            resolved.value.metadata_root,
            key,
            maximum_bytes,
            &mut self.work.attributes,
        )
    }

    /// Lists the attribute keys of one inode in order, within a declared bound.
    ///
    /// The listing is bounded twice: a key beyond
    /// [`MAXIMUM_ATTRIBUTE_KEYS`](crate::filesystem::limits::MAXIMUM_ATTRIBUTE_KEYS)
    /// is refused rather than collected, so one inode cannot make this call own an
    /// unbounded name set, and the pages it walks are charged to the reader's own
    /// work.
    pub fn attribute_keys(&mut self, path: &LogicalPath) -> ContentResult<Vec<AttributeKey>> {
        let resolved = self.resolve(path)?;
        let mut keys = Vec::new();
        let mut work = crate::filesystem::attributes::patch::AttributePatchWork::default();
        crate::filesystem::attributes::patch::visit_keys_counted(
            self.reader,
            resolved.value.metadata_root,
            &mut work,
            |key, _| {
                if keys.len() == MAXIMUM_ATTRIBUTE_KEYS {
                    return Err(ContentError::ObjectLimitExceeded {
                        limit: MAXIMUM_ATTRIBUTE_KEYS,
                        actual: keys.len() + 1,
                    });
                }
                keys.push(key.clone());
                Ok(())
            },
        )?;
        self.work.attributes.pages_read = self
            .work
            .attributes
            .pages_read
            .saturating_add(work.base_pages);
        self.work.attributes.read_waves = self
            .work
            .attributes
            .read_waves
            .saturating_add(work.base_pages);
        Ok(keys)
    }

    fn table(&self) -> InodeTable {
        InodeTable {
            root: self.root.inode_table(),
            root_serial: self.root.root_inode().serial(),
        }
    }

    /// Reads one inode record the walk demands.
    ///
    /// A serial the table does not hold is a **logical** absence, not provider
    /// absence: the lookup miss is `Ok(None)` from
    /// [`inode_lookup`](crate::filesystem::inode::read::lookup), whose own contract
    /// is that *"an absent serial is reported as absent rather than as a missing
    /// object"*. `MissingObject` is the provider's answer and for nothing else —
    /// `object::access`, `cas::provider` and `error.rs` all say so — and a provider
    /// that does not hold an object this walk names never reaches this line: it
    /// returns `Err(MissingObject)` from the read itself.
    ///
    /// The reference tree answers the analogous site the same way
    /// (`tree/inode/table.rs`). This layer holds no completeness proof for the tree
    /// it walks — a resolve reads one path component at a time and never loads the
    /// whole inode table — so it is not entitled to call a lookup miss a torn tree;
    /// that judgement belongs where inputs are checked, in `validate`.
    fn inode(&mut self, table: InodeTable, serial: u64) -> ContentResult<InodeValue> {
        inode_lookup(self.reader, table, serial, &mut self.work.inode)?
            .ok_or(ContentError::PathNotFound)
    }
}
