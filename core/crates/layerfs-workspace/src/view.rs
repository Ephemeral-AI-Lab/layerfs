//! Effective read semantics over an owned unchanged base and short overlay jobs.
use crate::{BaseStat, BaseView, OverlayRead, Workspace, WorkspaceError, WorkspaceResult};
use layerfs_content::filesystem::{attributes::PortableMetadata, PathName};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::ContentError;
use layerfs_overlay::{BaseSource, Inode};

/// One retained source window. Its caller explicitly releases the exact engine
/// source after provider/reply work is fenced; Drop performs no SQL.
pub struct SourceView {
    pub(crate) base: BaseView,
    pub(crate) source: BaseSource,
}
/// Portable effective facts; namespace references are not POSIX directory nlink.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewStat {
    pub serial: u64,
    pub kind: InodeKind,
    pub metadata: PortableMetadata,
    pub namespace_refs: u64,
    pub logical_len: u64,
}
impl From<BaseStat> for ViewStat {
    fn from(v: BaseStat) -> Self {
        Self {
            serial: v.serial,
            kind: v.value.kind,
            metadata: v.metadata,
            namespace_refs: v.value.namespace_ref_count,
            logical_len: v.logical_len,
        }
    }
}
impl From<Inode> for ViewStat {
    fn from(v: Inode) -> Self {
        Self {
            serial: v.serial,
            kind: match v.kind {
                layerfs_overlay::InodeKind::File => InodeKind::RegularFile,
                layerfs_overlay::InodeKind::Directory => InodeKind::Directory,
                layerfs_overlay::InodeKind::Symlink => InodeKind::Symlink,
            },
            metadata: PortableMetadata {
                mode: u32::from(v.mode),
                mtime_seconds: v.mtime_seconds,
                mtime_nanoseconds: v.mtime_nanoseconds,
            },
            namespace_refs: v.nlink,
            logical_len: v.size,
        }
    }
}
impl Workspace {
    /// Retains the selected binding for the owner-issued source. The source must
    /// stay owned until this bounded processing window ends. No provider I/O here.
    pub fn view_for_source(&self, source: BaseSource) -> WorkspaceResult<SourceView> {
        if source.route() != self.route() {
            return Err(layerfs_overlay::OverlayError::Stale.into());
        }
        let base = self.base()?;
        if base.identity().0.to_bytes() != source.root() {
            return Err(WorkspaceError::BaseChanged {
                expected: source.root(),
                actual: base.identity().0.to_bytes(),
            });
        }
        Ok(SourceView { base, source })
    }
}
impl SourceView {
    pub const fn source(&self) -> BaseSource {
        self.source
    }
    pub fn base(&self) -> &BaseView {
        &self.base
    }
    pub fn stat(&self, overlay: &impl OverlayRead, serial: u64) -> WorkspaceResult<ViewStat> {
        if let Some(value) = overlay.inode(self.source, serial)? {
            return Ok(value.into());
        }
        let inherited = match self.base.stat(serial) {
            Ok(value) => Some(value),
            Err(WorkspaceError::Content(ContentError::PathNotFound)) => None,
            Err(error) => return Err(error),
        };
        if let Some(value) = overlay.inode(self.source, serial)? {
            return Ok(value.into());
        }
        inherited
            .map(Into::into)
            .ok_or(ContentError::PathNotFound.into())
    }
    pub fn lookup(
        &self,
        overlay: &impl OverlayRead,
        parent: u64,
        name: &PathName,
    ) -> WorkspaceResult<ViewStat> {
        if self.stat(overlay, parent)?.kind != InodeKind::Directory {
            return Err(ContentError::WrongLogicalRole.into());
        }
        if let Some(local) = overlay.dentry(self.source, parent, name.as_bytes())? {
            return self.stat(overlay, local.serial.ok_or(ContentError::PathNotFound)?);
        }
        let inherited = match self.base.child(parent, name) {
            Ok(value) => Some(value.serial),
            Err(ContentError::PathNotFound) => None,
            Err(error) => return Err(error.into()),
        };
        let serial = match overlay.dentry(self.source, parent, name.as_bytes())? {
            Some(local) => local.serial,
            None => inherited,
        }
        .ok_or(ContentError::PathNotFound)?;
        self.stat(overlay, serial)
    }
}
