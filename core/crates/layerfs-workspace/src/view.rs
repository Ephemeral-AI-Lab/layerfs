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
    /// Canonical root directory serial of the owned base; never rebound.
    pub fn root_serial(&self) -> u64 {
        self.base.root().root_inode().serial()
    }
    /// Whether the owned base can bind names under this directory, after the
    /// kind/removal checks every name read shares. A directory created above
    /// the installed floor has no base children, so no base demand is made.
    pub(crate) fn inherits(&self, serial: u64, local: Option<&Inode>) -> WorkspaceResult<bool> {
        match local {
            Some(row) if row.kind != layerfs_overlay::InodeKind::Directory => {
                Err(ContentError::WrongLogicalRole.into())
            }
            Some(row) if row.nlink == 0 && serial != self.root_serial() => {
                Err(ContentError::PathNotFound.into())
            }
            Some(row) => Ok(!self.source.created_above(row.born)),
            None if self.base.inode(serial)?.value.kind != InodeKind::Directory => {
                Err(ContentError::WrongLogicalRole.into())
            }
            None => Ok(true),
        }
    }
    pub fn lookup(
        &self,
        overlay: &impl OverlayRead,
        parent: u64,
        name: &PathName,
    ) -> WorkspaceResult<ViewStat> {
        let local = overlay.inode(self.source, parent)?;
        let inherits = self.inherits(parent, local.as_ref())?;
        if let Some(local) = overlay.dentry(self.source, parent, name.as_bytes())? {
            return self.stat(overlay, local.serial.ok_or(ContentError::PathNotFound)?);
        }
        if !inherits {
            return Err(ContentError::PathNotFound.into());
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
    /// Target of a symlink: the local creation cell, or the inherited object.
    pub fn readlink(
        &self,
        overlay: &impl OverlayRead,
        serial: u64,
    ) -> WorkspaceResult<layerfs_content::filesystem::SymlinkTarget> {
        let Some(local) = overlay.inode(self.source, serial)? else {
            return Ok(self.base.readlink(serial)?);
        };
        if local.kind != layerfs_overlay::InodeKind::Symlink {
            return Err(ContentError::WrongLogicalRole.into());
        }
        if !self.source.created_above(local.born) {
            return Ok(self.base.readlink(serial)?);
        }
        let cell = overlay
            .cell(self.source, serial, local.born, 0)?
            .ok_or(ContentError::InvalidRecord("local symlink target"))?;
        let length = usize::try_from(local.size)
            .ok()
            .filter(|length| *length <= cell.data.len())
            .ok_or(ContentError::InvalidRecord("local symlink target"))?;
        Ok(layerfs_content::filesystem::SymlinkTarget::new(
            cell.data[..length].to_vec(),
        )?)
    }
}
