//! Exact native directory owners and retained current parent relationships.
use crate::{
    db::{integer, unsigned},
    LeaseKind, NativeDirectory, NativeMount, Overlay, OverlayError, OverlayResult, StatementKind,
};
impl Overlay {
    /// Called by the deciding lookup/mutation transaction for a directory only.
    /// Its FK follows existing independent file custody, including open dirs.
    pub(crate) fn set_native_parent(
        &self,
        mount: NativeMount,
        serial: u64,
        parent: u64,
    ) -> OverlayResult<()> {
        self.execute(StatementKind::Lease,
            "INSERT INTO native_parent VALUES(?1,?2,?3,?4) ON CONFLICT(ns,serial) DO UPDATE SET mount=excluded.mount,parent=excluded.parent",
            &[&mount.route.ns, &integer(mount.owner)?, &integer(serial)?, &integer(parent)?], 32)?;
        Ok(())
    }
    pub(crate) fn native_parent(&self, mount: NativeMount, serial: u64) -> OverlayResult<u64> {
        self.query(
            StatementKind::Lease,
            "SELECT parent FROM native_parent WHERE ns=?1 AND serial=?2 AND mount=?3",
            &[&mount.route.ns, &integer(serial)?, &integer(mount.owner)?],
            24,
            |r| unsigned(r, 0),
        )?
        .pop()
        .ok_or(OverlayError::Stale)
    }
    pub(crate) fn retain_native_directory(
        &self,
        mount: NativeMount,
        request: &[u8; 8],
        serial: u64,
    ) -> OverlayResult<NativeDirectory> {
        self.native_parent(mount, serial)?;
        let owner = self.mint_owner(mount.route)?;
        self.execute(
            StatementKind::Lease,
            "INSERT INTO native_directory(ns,mount,owner,serial,request) VALUES(?1,?2,?3,?4,?5)",
            &[
                &mount.route.ns,
                &integer(mount.owner)?,
                &integer(owner)?,
                &integer(serial)?,
                &request.as_slice(),
            ],
            40,
        )?;
        self.execute(
            StatementKind::Lease,
            "INSERT INTO lease VALUES(?1,7,?2,?3)",
            &[&mount.route.ns, &integer(owner)?, &integer(serial)?],
            24,
        )?;
        self.file_ref(
            mount.route.ns,
            integer(serial)?,
            LeaseKind::FileHandle,
            true,
        )?;
        Ok(NativeDirectory {
            mount,
            owner,
            serial,
        })
    }
    /// Full mount/route/serial/encoded-handle validation, including open state.
    pub fn native_directory(
        &self,
        mount: NativeMount,
        serial: u64,
        handle: u64,
    ) -> OverlayResult<NativeDirectory> {
        self.check_native_attached(mount)?;
        self.query(StatementKind::Lease,
            "SELECT 1 FROM native_directory WHERE ns=?1 AND owner=?2 AND mount=?3 AND serial=?4 AND closed=0",
            &[&mount.route.ns, &integer(handle)?, &integer(mount.owner)?, &integer(serial)?], 32,
            |_| Ok(NativeDirectory { mount, owner: handle, serial }))?.pop().ok_or(OverlayError::Stale)
    }
    /// Original association remains observable while closed-cookie cleanup is
    /// pending; observing it never permits replaying OPENDIR or RELEASEDIR.
    pub fn retained_native_directory(
        &self,
        mount: NativeMount,
        request: u64,
    ) -> OverlayResult<Option<NativeDirectory>> {
        self.check_native_attached(mount)?;
        self.query(
            StatementKind::Lease,
            "SELECT owner,serial FROM native_directory WHERE ns=?1 AND mount=?2 AND request=?3",
            &[
                &mount.route.ns,
                &integer(mount.owner)?,
                &request.to_be_bytes().as_slice(),
            ],
            24,
            |r| {
                Ok(NativeDirectory {
                    mount,
                    owner: unsigned(r, 0)?,
                    serial: unsigned(r, 1)?,
                })
            },
        )
        .map(|mut rows| rows.pop())
    }
    /// Revokes new use, releases the descriptor lease and retains cookie/header
    /// storage for existing independent read sources. No cookie sweep here.
    pub fn close_native_directory(&self, directory: NativeDirectory) -> OverlayResult<()> {
        self.atomic_cleanup(|| {
            self.native_directory(directory.mount, directory.serial, directory.owner)?;
            self.close_native_directory_inner(directory)
        })
    }
    /// The caller established that this exact handle row is still open.
    pub(crate) fn close_native_directory_inner(
        &self,
        directory: NativeDirectory,
    ) -> OverlayResult<()> {
        let ns = directory.mount.route.ns;
        self.execute(
            StatementKind::Lease,
            "UPDATE native_directory SET closed=1 WHERE ns=?1 AND owner=?2",
            &[&ns, &integer(directory.owner)?],
            16,
        )?;
        self.execute(
            StatementKind::Lease,
            "DELETE FROM lease WHERE ns=?1 AND kind=7 AND owner=?2 AND resource=?3",
            &[&ns, &integer(directory.owner)?, &integer(directory.serial)?],
            24,
        )?;
        self.file_ref(ns, integer(directory.serial)?, LeaseKind::FileHandle, false)?;
        self.queue_native_directory(ns, integer(directory.owner)?)?;
        self.queue_closed(directory.mount.route)
    }
    pub(crate) fn queue_native_directory(&self, ns: i64, owner: i64) -> OverlayResult<()> {
        let ready = self.query(StatementKind::Lease,
            "SELECT closed=1 AND NOT EXISTS(SELECT 1 FROM native_directory_read WHERE ns=?1 AND directory=?2) FROM native_directory WHERE ns=?1 AND owner=?2",
            &[&ns, &owner], 16, |r| r.get::<_, bool>(0))?.pop().unwrap_or(false);
        if ready {
            self.enqueue(ns, crate::maintenance::NATIVE_DIRECTORY, owner, owner)?;
        }
        Ok(())
    }
}
