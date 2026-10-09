//! READ and READLINK as one owner visit that records nothing. The visit
//! copies the window's local bytes inside its job and names the base root
//! every other byte belongs to. Its request then reads those bytes, if there
//! are any, from the Store outside the owner and replies. The request holds
//! no source, reader or lease: nothing is released after the reply, and no
//! install, revoke or close waits for it. The reply is the file as it stood
//! at the visit, over the one base root the visit named.
use crate::{
    operations::file::read::file_window, view::link_target, BaseView, Refusal, Workspace,
    WorkspaceResult,
};
use layerfs_content::{
    object::inode_leaf::{InodeKind as BaseKind, InodeValue},
    ContentError,
};
use layerfs_overlay::{InodeKind, LocalRead, NativeMount, Overlay, OverlayError, OverlayResult};
use std::{fmt, sync::Arc};

/// One READ or READLINK as one read-only owner job.
#[derive(Clone)]
pub struct NativeDataVisit {
    mount: NativeMount,
    serial: u64,
    handle: Option<u64>,
    offset: u64,
    length: u32,
    resident: BaseView,
}
/// What the visit found, as plain values its request owns.
#[derive(Clone, Debug)]
pub struct NativeWindow {
    /// The base root every inherited byte of the window belongs to: the
    /// Workspace's base at the visit, or the root an unlinked file retains.
    pub root: [u8; 32],
    /// The local part of the window. None: the inode has no local row.
    pub local: Option<LocalRead>,
    /// With no local row: the base inode, when memory answered it in the job.
    pub base: Option<InodeValue>,
    /// And that regular file's length, when memory remembered it.
    pub length: Option<u64>,
}
impl fmt::Debug for NativeDataVisit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeDataVisit")
            .field("mount", &self.mount)
            .field("serial", &self.serial)
            .field("handle", &self.handle)
            .field("offset", &self.offset)
            .field("length", &self.length)
            .finish_non_exhaustive()
    }
}
impl Workspace {
    /// `resident` answers from memory only. No I/O here. A READ names its
    /// descriptor; a READLINK names none and is served under the kernel's
    /// lookup reference.
    pub fn native_data_visit(
        &self,
        resident: Arc<crate::CanonicalClient>,
        mount: NativeMount,
        serial: u64,
        handle: Option<u64>,
        offset: u64,
        length: u32,
    ) -> WorkspaceResult<NativeDataVisit> {
        let base = self.base()?;
        if mount.route() != self.route() || mount.root_serial() != base.root().root_inode().serial()
        {
            return Err(OverlayError::Stale.into());
        }
        Ok(NativeDataVisit {
            mount,
            serial,
            handle,
            offset,
            length,
            resident: base.with_client(resident),
        })
    }
}
impl NativeDataVisit {
    pub const fn mount(&self) -> NativeMount {
        self.mount
    }
    /// The reply's bound: the window's bytes and one inherited bit for each.
    pub fn charge(&self) -> usize {
        self.length as usize + (self.length as usize).div_ceil(8)
    }
    /// The visit always decides: what memory does not answer about the base
    /// is left to the one read that follows outside the owner.
    pub fn perform(&self, db: &Overlay) -> OverlayResult<NativeWindow> {
        let (root, local) = db.read_native_visit(
            self.mount,
            self.serial,
            self.handle,
            self.offset,
            self.length,
        )?;
        let (base, length) = if local.is_none() && self.resident.identity().0.to_bytes() == root {
            self.resident.extent(self.serial)
        } else {
            (None, None)
        };
        Ok(NativeWindow {
            root,
            local,
            base,
            length,
        })
    }
}
const NO_BASE: ContentError = ContentError::InvalidRecord("native window without its base");
impl NativeWindow {
    fn kind(&self) -> Option<InodeKind> {
        match (&self.local, self.base) {
            (Some(local), _) => Some(local.kind),
            (None, Some(value)) => Some(match value.kind {
                BaseKind::RegularFile => InodeKind::File,
                BaseKind::Directory => InodeKind::Directory,
                BaseKind::Symlink => InodeKind::Symlink,
            }),
            (None, None) => None,
        }
    }
    /// A READ of anything but a regular file and a READLINK of anything but
    /// a symlink are refused.
    fn refusal(&self, link: bool) -> Option<Refusal> {
        match (self.kind()?, link) {
            (InodeKind::File, false) | (InodeKind::Symlink, true) => None,
            (InodeKind::Directory, false) => Some(Refusal::IsDirectory),
            _ => Some(Refusal::Invalid),
        }
    }
    /// The reply itself, when the visit decided every byte of a file window.
    pub fn whole(&self, link: bool) -> Option<&[u8]> {
        match &self.local {
            Some(local) if !link && local.kind == InodeKind::File && local.span.is_none() => {
                Some(&local.data)
            }
            _ => None,
        }
    }
    /// Whether the reply needs the base: an inherited byte, or a fact the
    /// visit could not take from memory. A refusal, a window decided locally
    /// and a window at or beyond a known end of file need nothing.
    pub fn inherits(&self, offset: u64, link: bool) -> bool {
        match (&self.local, self.base) {
            (None, None) => true,
            _ if self.refusal(link).is_some() => false,
            (Some(local), _) => local.span.is_some(),
            (None, Some(_)) => link || self.length.is_none_or(|length| offset < length),
        }
    }
    /// The reply, or its refusal. `base` is the Workspace's base over an
    /// admitted Store reader and is required when the window `inherits`; it
    /// is read at the root the visit named, never at a later one.
    pub fn finish(
        mut self,
        base: Option<&BaseView>,
        serial: u64,
        offset: u64,
        length: u32,
        link: bool,
    ) -> WorkspaceResult<Result<Vec<u8>, Refusal>> {
        let base = base.map(|base| base.at(self.root)).transpose()?;
        let base = base.as_deref();
        if self.local.is_none() && self.base.is_none() {
            self.base = match base.ok_or(NO_BASE)?.inode(serial) {
                Ok(found) => Some(found.value),
                Err(ContentError::PathNotFound) => return Ok(Err(Refusal::Missing)),
                Err(error) => return Err(error.into()),
            };
        }
        if let Some(refusal) = self.refusal(link) {
            return Ok(Err(refusal));
        }
        Ok(Ok(match (self.local, self.base, link) {
            (Some(local), _, false) => file_window(base, serial, local)?,
            (Some(local), _, true) => link_target(base, serial, local)?.as_bytes().to_vec(),
            (None, _, true) => base.ok_or(NO_BASE)?.readlink(serial)?.as_bytes().to_vec(),
            (None, Some(value), false) => match base {
                Some(base) => {
                    let plan = base.plan_file(value, offset, length)?;
                    let mut data = Vec::with_capacity(plan.length() as usize);
                    plan.emit(&mut data)?;
                    data
                }
                None if self.length.is_some_and(|end| offset >= end) => Vec::new(),
                None => return Err(NO_BASE.into()),
            },
            (None, None, false) => return Err(NO_BASE.into()),
        }))
    }
}
