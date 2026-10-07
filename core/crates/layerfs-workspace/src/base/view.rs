//! Demand reads through current public content APIs, without a base-tree mirror.
use crate::{CanonicalClient, WorkspaceResult};
use layerfs_content::filesystem::attributes::PortableMetadata;
use layerfs_content::filesystem::{
    DirectoryListing, FilesystemRead, FilesystemRoot, FilesystemRootId, InodeScope, PathName,
    Resolved, SymlinkTarget,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, FileView};
use layerfs_telemetry::timer::Timing;
use std::{io::Write, sync::Arc};

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
    fn reader(&self) -> ContentResult<FilesystemRead<'_>> {
        FilesystemRead::new(self.client.as_ref(), self.identity)
    }
    pub fn child(&self, parent: u64, name: &PathName) -> ContentResult<Resolved> {
        self.reader()?.resolve_child(parent, name)
    }
    pub fn inode(&self, serial: u64) -> ContentResult<Resolved> {
        self.reader()?.resolve_inode(serial)
    }
    pub fn list(
        &self,
        serial: u64,
        after: Option<&PathName>,
        entries: usize,
        bytes: usize,
    ) -> ContentResult<DirectoryListing> {
        self.reader()?.list_inode(serial, after, entries, bytes)
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
            layerfs_content::filesystem::DirectoryRoot(value.content_root),
            after,
            entries,
            bytes,
            &mut layerfs_content::filesystem::directory::DirectoryReadWork::default(),
        )
    }
    /// Bindings recorded by a directory's root page: one authenticated object,
    /// no listing. This is the exact base entry count of that directory.
    pub fn entries(&self, value: InodeValue) -> ContentResult<u64> {
        if value.kind != InodeKind::Directory {
            return Err(ContentError::WrongLogicalRole);
        }
        let page = self.client.read_canonical(value.content_root)?;
        Ok(layerfs_content::filesystem::directory::codec::page_shape(&page)?.1)
    }
    pub fn readlink(&self, serial: u64) -> ContentResult<SymlinkTarget> {
        self.reader()?.readlink_inode(serial)
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
        let mut read = self.reader()?;
        let value = read.resolve_inode(serial)?.value;
        let metadata = read.read_portable_inode(serial)?;
        let logical_len = match value.kind {
            InodeKind::RegularFile => lengths.file_length(value.content_root)?,
            InodeKind::Symlink => read.readlink_inode(serial)?.as_bytes().len() as u64,
            InodeKind::Directory => 0,
        };
        Ok(BaseStat {
            serial,
            value,
            metadata,
            logical_len,
        })
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
        let file = self.file(self.inode(serial)?.value)?;
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
