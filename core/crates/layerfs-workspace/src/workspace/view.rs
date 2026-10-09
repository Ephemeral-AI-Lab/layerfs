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
    /// Bindings of a directory that are directories: its POSIX link count
    /// is two more while it has a name. Zero for other kinds.
    pub subdirs: u64,
}
impl ViewStat {
    /// The facts of a base inode. `subdirs` is the derived child-directory
    /// count of a directory and zero for any other kind.
    pub fn of_base(v: BaseStat, subdirs: u64) -> Self {
        Self {
            serial: v.serial,
            kind: v.value.kind,
            metadata: v.metadata,
            namespace_refs: v.value.namespace_ref_count,
            logical_len: v.logical_len,
            subdirs,
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
            subdirs: v.subdirs,
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
    /// Reuses this exact retained source/root with an admitted provider. This
    /// neither acquires a source nor reads the Store; the caller owns both.
    pub fn with_client(&self, client: std::sync::Arc<crate::CanonicalClient>) -> Self {
        Self {
            base: self.base.with_client(client),
            source: self.source,
        }
    }
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
            Ok(value) if value.value.kind == InodeKind::Directory => {
                let subdirs = self.base.subdirs(value.value)?;
                Some(ViewStat::of_base(value, subdirs))
            }
            Ok(value) => Some(ViewStat::of_base(value, 0)),
            Err(WorkspaceError::Content(ContentError::PathNotFound)) => None,
            Err(error) => return Err(error),
        };
        if let Some(value) = overlay.inode(self.source, serial)? {
            return Ok(value.into());
        }
        inherited.ok_or(ContentError::PathNotFound.into())
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
        if let Some(local) = overlay.directory_entry(self.source, parent, name.as_bytes())? {
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
        let serial = match overlay.directory_entry(self.source, parent, name.as_bytes())? {
            Some(local) => local.serial,
            None => inherited,
        }
        .ok_or(ContentError::PathNotFound)?;
        self.stat(overlay, serial)
    }
    /// Effective local symlink layers, followed by exact immutable demand.
    pub fn readlink(
        &self,
        overlay: &impl OverlayRead,
        serial: u64,
    ) -> WorkspaceResult<layerfs_content::filesystem::SymlinkTarget> {
        let local = overlay.read(self.source, serial, 0, layerfs_overlay::CELL_BYTES as u32)?;
        self.emit_link(serial, local, self.source.root())
    }
    /// An independent request reference survives lookup release. The caller
    /// releases it only after target/output consumers have finished.
    pub fn readlink_owned(
        &self,
        overlay: &impl crate::OverlayFileRead,
        read: layerfs_overlay::FileRead,
    ) -> WorkspaceResult<layerfs_content::filesystem::SymlinkTarget> {
        if read.source().route() != self.source.route() {
            return Err(layerfs_overlay::OverlayError::Stale.into());
        }
        let local = overlay.file_read(read, 0, layerfs_overlay::CELL_BYTES as u32)?;
        self.readlink_window(read, local)
    }
    /// Compose an already completed local window without another owner job.
    /// Its completion and independent read reference stay owned by the caller.
    pub fn readlink_window(
        &self,
        read: layerfs_overlay::FileRead,
        local: Option<layerfs_overlay::LocalRead>,
    ) -> WorkspaceResult<layerfs_content::filesystem::SymlinkTarget> {
        if read.source().route() != self.source.route() {
            return Err(layerfs_overlay::OverlayError::Stale.into());
        }
        self.emit_link(read.serial(), local, read.source().root())
    }
    pub fn readlink_captured(
        &self,
        overlay: &impl crate::OverlayFileRead,
        reader: layerfs_overlay::CapturedReader,
        serial: u64,
    ) -> WorkspaceResult<layerfs_content::filesystem::SymlinkTarget> {
        if reader.capture().route() != self.source.route() {
            return Err(layerfs_overlay::OverlayError::Stale.into());
        }
        let local = overlay.captured_read(reader, serial, 0, layerfs_overlay::CELL_BYTES as u32)?;
        self.emit_link(serial, local, reader.root())
    }
    fn emit_link(
        &self,
        serial: u64,
        local: Option<layerfs_overlay::LocalRead>,
        root: [u8; 32],
    ) -> WorkspaceResult<layerfs_content::filesystem::SymlinkTarget> {
        let retained = local.as_ref().and_then(|r| r.base_root).unwrap_or(root);
        let base = self.base.at(retained)?;
        match local {
            Some(local) => link_target(Some(base.as_ref()), serial, local),
            None => Ok(base.readlink(serial)?),
        }
    }
}
/// A symlink's local window with its inherited bytes read from `base`, which
/// the caller bound to the root those bytes belong to. A window with no
/// inherited byte needs no base.
pub(crate) fn link_target(
    base: Option<&BaseView>,
    serial: u64,
    mut local: layerfs_overlay::LocalRead,
) -> WorkspaceResult<layerfs_content::filesystem::SymlinkTarget> {
    if local.kind != layerfs_overlay::InodeKind::Symlink {
        return Err(ContentError::WrongLogicalRole.into());
    }
    if local.size > layerfs_overlay::CELL_BYTES as u64 || local.data.len() != local.size as usize {
        return Err(ContentError::InvalidRecord("symlink window").into());
    }
    if local.span.is_some() {
        let base = base.ok_or(ContentError::InvalidRecord("inherited window without base"))?;
        let target = base.readlink(serial)?;
        for (slot, byte) in local.data.iter_mut().enumerate() {
            if local
                .inherited
                .get(slot / 8)
                .is_some_and(|b| b & (1 << (slot % 8)) != 0)
            {
                *byte = *target
                    .as_bytes()
                    .get(slot)
                    .ok_or(ContentError::InvalidRecord("inherited symlink span"))?;
            }
        }
    }
    Ok(layerfs_content::filesystem::SymlinkTarget::new(local.data)?)
}
