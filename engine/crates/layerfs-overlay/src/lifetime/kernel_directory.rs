//! Exact kernel directory owners and retained current parent relationships.
use crate::{
    db::integer, KernelDirectory, KernelMount, Overlay, OverlayError, OverlayResult, ReferenceKind,
};
impl Overlay {
    /// Called by the deciding lookup/mutation transaction for a directory only.
    /// Its FK follows existing independent file custody, including open dirs.
    pub(crate) fn set_kernel_parent(
        &self,
        mount: KernelMount,
        serial: u64,
        parent: u64,
    ) -> OverlayResult<()> {
        integer(serial)?;
        integer(parent)?;
        self.open
            .borrow_mut()
            .parents
            .put(serial, (mount.owner, parent));
        Ok(())
    }
    pub(crate) fn kernel_parent(&self, mount: KernelMount, serial: u64) -> OverlayResult<u64> {
        self.available()?;
        self.open
            .borrow()
            .parents
            .rows
            .get(&serial)
            .filter(|(owner, _)| *owner == mount.owner)
            .map(|(_, parent)| *parent)
            .ok_or(OverlayError::Stale)
    }
    pub(crate) fn retain_kernel_directory(
        &self,
        mount: KernelMount,
        request: &[u8; 8],
        serial: u64,
    ) -> OverlayResult<KernelDirectory> {
        self.kernel_parent(mount, serial)?;
        let owner = self.mint_owner()?;
        let directory = KernelDirectory {
            mount,
            owner,
            serial,
        };
        let request = u64::from_be_bytes(*request);
        {
            let mut state = self.open.borrow_mut();
            let held = crate::open::DirectoryState {
                directory,
                floor: 0,
            };
            let request = (mount.owner, request);
            state
                .directories
                .insert(owner, request, held, "duplicate directory request")?;
            state.open_cookies(owner)?;
        }
        self.file_ref(integer(serial)?, ReferenceKind::File, true)?;
        Ok(directory)
    }
    /// RELEASEDIR in one visit: the fence reads the Workspace and checks the
    /// exact descriptor in memory. The handle and its references are released,
    /// and its published replies detached. A closed Workspace still releases
    /// its descriptors.
    pub fn close_kernel_directory(
        &self,
        mount: KernelMount,
        serial: u64,
        handle: u64,
    ) -> OverlayResult<()> {
        self.atomic_cleanup(|| {
            let held = super::kernel_visit::Held::Directory(handle);
            let (_, _) = self.kernel_fence(mount, serial, held, false)?;
            self.close_kernel_directory_inner(KernelDirectory {
                mount,
                owner: handle,
                serial,
            })?;
            Ok(())
        })
    }
    /// The caller fenced this exact open handle. Detach its cookie store as
    /// one ownership transfer, so the caller can destroy arbitrarily many
    /// published pages after releasing the connection turn. No row sweep or
    /// terminal maintenance item runs here.
    pub(crate) fn close_kernel_directory_inner(
        &self,
        directory: KernelDirectory,
    ) -> OverlayResult<()> {
        let owner = directory.owner;
        {
            let mut state = self.open.borrow_mut();
            let exact = |held: &crate::open::DirectoryState| held.directory == directory;
            state.directories.remove(owner, exact)?;
            state.close_cookies(owner)?;
        }
        self.file_ref(integer(directory.serial)?, ReferenceKind::File, false)
    }
}
/// Published replies one RELEASEDIR deletes in its own job, one maintenance
/// turn retires, and one publication on a rewound handle deletes below the
/// floor. The LIMIT of `sql::COOKIE_PAGES` is this value plus one, kept
/// equal by hand.
pub(crate) const INLINE_PAGES: usize = 8;
