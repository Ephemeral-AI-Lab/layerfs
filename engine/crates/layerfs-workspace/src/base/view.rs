//! Demand reads through current public content APIs, without a base-tree mirror.
use crate::{CanonicalClient, WorkspaceResult};
use layerfs_content::filesystem::attributes::PortableMetadata;
use layerfs_content::filesystem::directory::read::{list_after, lookup_counted, DirectoryReadWork};
use layerfs_content::filesystem::inode::{lookup, lookup_many, InodeReadWork, InodeTable};
use layerfs_content::filesystem::{
    DirectoryListing, DirectoryRoot, FilesystemRead, FilesystemRoot, FilesystemRootId, InodeScope,
    PathName, Resolved, SymlinkTarget,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, FileView};
use layerfs_telemetry::timer::Timing;
use std::{io::Write, sync::Arc};

/// Directory entries after a looked-up file whose lengths one demand
/// remembers with its own (#356 F-032).
pub(crate) const SIBLING_LENGTHS: usize = 128;
/// Encoded listing bytes of that window. One row is at most 265 bytes.
const SIBLING_BYTES: usize = 64 * 1024;

/// Immutable binding whose identity remains valid across later local installs.
#[derive(Clone)]
pub struct BaseView {
    client: Arc<CanonicalClient>,
    identity: FilesystemRootId,
    root: FilesystemRoot,
}
/// Exact portable metadata plus content-derived length of one immutable inode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BaseStat {
    pub serial: u64,
    pub value: InodeValue,
    pub metadata: PortableMetadata,
    pub logical_len: u64,
}
/// Stable immutable file classification and source, retained before external IO.
pub struct BaseRead {
    client: Arc<CanonicalClient>,
    file: FileView,
    start: u64,
    end: u64,
}
impl BaseView {
    /// Checks root/profile/scope with one canonical root acquisition, no scan.
    pub fn open(
        client: Arc<CanonicalClient>,
        identity: FilesystemRootId,
        scope: InodeScope,
    ) -> ContentResult<Self> {
        let root = FilesystemRead::new(client.as_ref(), identity)?.root();
        if root.scope() != scope {
            return Err(ContentError::ScopeMismatch {
                what: "base root scope",
            });
        }
        Ok(Self {
            client,
            identity,
            root,
        })
    }
    pub const fn identity(&self) -> FilesystemRootId {
        self.identity
    }
    pub const fn root(&self) -> FilesystemRoot {
        self.root
    }
    /// Retains the already checked root metadata with an operation's provider.
    /// No acquisition, namespace traversal or materialization occurs here.
    /// The caller must authorize the provider for this binding's context.
    pub fn with_client(&self, client: Arc<CanonicalClient>) -> Self {
        Self {
            client,
            identity: self.identity,
            root: self.root,
        }
    }
    pub(crate) fn client(&self) -> Arc<CanonicalClient> {
        self.client.clone()
    }
    pub(crate) fn rebind(&self, identity: FilesystemRootId) -> ContentResult<Self> {
        Self::open(self.client.clone(), identity, self.root.scope())
    }
    /// This binding when `root` is its own, else this client bound to `root`:
    /// the base a retained read window names, whatever was installed since.
    pub(crate) fn at(&self, root: [u8; 32]) -> ContentResult<std::borrow::Cow<'_, Self>> {
        if root == self.identity.0.to_bytes() {
            return Ok(std::borrow::Cow::Borrowed(self));
        }
        let identity = FilesystemRootId(layerfs_content::ObjectId::from_bytes(&root)?);
        Ok(std::borrow::Cow::Owned(self.rebind(identity)?))
    }
    /// The inode table of the root checked at open. The root is immutable,
    /// so no lookup acquires or decodes that object again.
    pub(crate) fn table(&self) -> InodeTable {
        InodeTable {
            root: self.root.inode_table(),
            root_serial: self.root.root_inode().serial(),
        }
    }
    pub fn child(&self, parent: u64, name: &PathName) -> ContentResult<Resolved> {
        let directory = self.inode(parent)?.value;
        if directory.kind != InodeKind::Directory {
            return Err(ContentError::WrongLogicalRole);
        }
        let found = lookup_counted(
            self.client.as_ref(),
            DirectoryRoot(directory.content_root),
            name,
            &mut DirectoryReadWork::default(),
        )?;
        self.inode(found.ok_or(ContentError::PathNotFound)?.1)
    }
    /// Remembers, with one length demand, the lengths of the regular file
    /// `first` and of the regular files listed after `name` in the base
    /// directory `parent`, at most [`SIBLING_LENGTHS`] of them (#356 F-032).
    /// A tree walk looks names up in listing order, so their stats then come
    /// from memory instead of one Store demand per file. Bounded by the
    /// window and by the cache's own allowance; nothing is held otherwise.
    pub(crate) fn provide_sibling_lengths(
        &self,
        parent: u64,
        name: &PathName,
        first: layerfs_content::ObjectId,
    ) -> WorkspaceResult<()> {
        let directory = self.inode(parent)?.value;
        let page = list_after(
            self.client.as_ref(),
            DirectoryRoot(directory.content_root),
            Some(name),
            SIBLING_LENGTHS,
            SIBLING_BYTES,
            &mut DirectoryReadWork::default(),
        )?;
        let serials: Vec<u64> = page.entries.iter().map(|(_, serial)| *serial).collect();
        let mut roots = vec![first];
        let work = &mut InodeReadWork::default();
        for value in lookup_many(self.client.as_ref(), self.table(), &serials, work)?
            .into_iter()
            .flatten()
        {
            if value.kind == InodeKind::RegularFile {
                roots.push(value.content_root);
            }
        }
        self.client.provide_lengths(&roots)
    }
    /// A serial the table does not hold is a logical absence.
    pub fn inode(&self, serial: u64) -> ContentResult<Resolved> {
        let work = &mut InodeReadWork::default();
        let value = lookup(self.client.as_ref(), self.table(), serial, work)?;
        Ok(Resolved {
            serial,
            value: value.ok_or(ContentError::PathNotFound)?,
        })
    }
    pub fn list(
        &self,
        serial: u64,
        after: Option<&PathName>,
        entries: usize,
        bytes: usize,
    ) -> ContentResult<DirectoryListing> {
        let value = self.inode(serial)?.value;
        if value.kind != InodeKind::Directory {
            return Err(ContentError::WrongLogicalRole);
        }
        list_after(
            self.client.as_ref(),
            DirectoryRoot(value.content_root),
            after,
            entries,
            bytes,
            &mut DirectoryReadWork::default(),
        )
    }
    pub fn list_after_bytes(
        &self,
        serial: u64,
        after: Option<&[u8]>,
        entries: usize,
        bytes: usize,
    ) -> ContentResult<DirectoryListing> {
        let value = self.inode(serial)?.value;
        if value.kind != InodeKind::Directory {
            return Err(ContentError::WrongLogicalRole);
        }
        layerfs_content::filesystem::directory::read::list_after_bytes(
            self.client.as_ref(),
            DirectoryRoot(value.content_root),
            after,
            entries,
            bytes,
            &mut DirectoryReadWork::default(),
        )
    }
    /// Bindings recorded by a directory's root page: one authenticated object,
    /// no listing. This is the exact base entry count of that directory.
    pub fn entries(&self, value: InodeValue) -> ContentResult<u64> {
        if value.kind != InodeKind::Directory {
            return Err(ContentError::WrongLogicalRole);
        }
        Ok(layerfs_content::filesystem::directory::root_shape(
            self.client.as_ref(),
            layerfs_content::filesystem::DirectoryRoot(value.content_root),
        )?
        .1)
    }
    pub fn readlink(&self, serial: u64) -> ContentResult<SymlinkTarget> {
        let value = self.inode(serial)?.value;
        if value.kind != InodeKind::Symlink {
            return Err(ContentError::WrongLogicalRole);
        }
        SymlinkTarget::decode(&self.client.read_canonical(value.content_root)?)
    }
    pub fn stat(&self, serial: u64) -> WorkspaceResult<BaseStat> {
        self.stat_with_lengths(serial, self.client.as_ref())
    }
    /// Explicit scoped owning metadata port; no payload classification fallback.
    pub fn stat_with_lengths(
        &self,
        serial: u64,
        lengths: &dyn crate::FileLengths,
    ) -> WorkspaceResult<BaseStat> {
        self.stat_resolved_with_lengths(self.inode(serial)?, lengths)
    }
    /// Complete stat from the child already authenticated in this base.
    pub(crate) fn stat_resolved_with_lengths(
        &self,
        resolved: Resolved,
        lengths: &dyn crate::FileLengths,
    ) -> WorkspaceResult<BaseStat> {
        let Resolved { serial, value } = resolved;
        let metadata = self.client.portable(value.metadata_root, value.kind)?;
        let logical_len = match value.kind {
            InodeKind::RegularFile => lengths.file_length(value.content_root)?,
            InodeKind::Symlink => {
                SymlinkTarget::decode(&self.client.read_canonical(value.content_root)?)?
                    .as_bytes()
                    .len() as u64
            }
            InodeKind::Directory => 0,
        };
        Ok(BaseStat {
            serial,
            value,
            metadata,
            logical_len,
        })
    }
    /// One inode's value and, for a regular file, its length, as far as this
    /// view's client answers. A memory-only client answers a resident inode
    /// and a remembered length; what it cannot answer is absent, not an error.
    pub(crate) fn extent(&self, serial: u64) -> (Option<InodeValue>, Option<u64>) {
        let Ok(value) = self.inode(serial).map(|found| found.value) else {
            return (None, None);
        };
        let length = (value.kind == InodeKind::RegularFile)
            .then(|| crate::FileLengths::file_length(self.client.as_ref(), value.content_root).ok())
            .flatten();
        (Some(value), length)
    }
    /// Plans a bounded file read, clamps EOF and retains its immutable source.
    pub fn plan_read(&self, serial: u64, offset: u64, length: u32) -> ContentResult<BaseRead> {
        if length > 128 * 1024 {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "base read window",
                limit: 128 * 1024,
                actual: u64::from(length),
            });
        }
        self.plan_file(self.inode(serial)?.value, offset, length)
    }
    /// The same plan for an inode value already resolved in this base.
    pub(crate) fn plan_file(
        &self,
        value: InodeValue,
        offset: u64,
        length: u32,
    ) -> ContentResult<BaseRead> {
        let file = self.file(value)?;
        let start = offset.min(file.logical_len());
        let end = start
            .saturating_add(u64::from(length))
            .min(file.logical_len());
        Ok(BaseRead {
            client: self.client.clone(),
            file,
            start,
            end,
        })
    }
}
/// One committed regular file named by its content root alone: the source a
/// reference piece names, whichever base bound that file.
pub struct BaseContent {
    client: Arc<CanonicalClient>,
    file: FileView,
}
impl BaseView {
    /// Opens committed file content by its root: one authenticated object
    /// through this view's client and no namespace lookup.
    pub fn content(&self, root: [u8; 32]) -> ContentResult<BaseContent> {
        let root = layerfs_content::ObjectId::from_bytes(&root)?;
        let file = Timing::disabled("base.content", |scope| {
            FileView::open(self.client.as_ref(), root, scope.child("file"))
        })
        .0?;
        Ok(BaseContent {
            client: self.client.clone(),
            file,
        })
    }
}
impl BaseContent {
    pub fn root(&self) -> [u8; 32] {
        self.file.root().to_bytes()
    }
    pub fn length(&self) -> u64 {
        self.file.logical_len()
    }
    /// The mapping root of content in chunks; none for a whole-file payload.
    pub(crate) fn state(&self) -> ContentResult<Option<layerfs_content::file::mapping::FileState>> {
        self.file.file_state()
    }
    /// Appends exactly `length` committed bytes from `offset`. A range that
    /// passes the end of the content is an error, never zeros.
    pub fn read(&self, offset: u64, length: u64, sink: &mut dyn Write) -> ContentResult<()> {
        let end = offset
            .checked_add(length)
            .ok_or(ContentError::LengthOverflow)?;
        Timing::disabled("base.content", |scope| {
            self.file
                .read_range(self.client.as_ref(), offset..end, sink, scope)
        })
        .0
    }
}
impl BaseRead {
    pub fn length(&self) -> u64 {
        self.end - self.start
    }
    /// Streams the already planned range; a failed sink may contain partial data.
    pub fn emit(&self, sink: &mut dyn Write) -> ContentResult<()> {
        Timing::disabled("base.read", |scope| {
            self.file
                .read_range(self.client.as_ref(), self.start..self.end, sink, scope)
        })
        .0
    }
}
