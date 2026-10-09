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
        let owner = self.mint_owner()?;
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
    /// The original association of an open handle, or of a closed one whose
    /// cookie cleanup is pending; observing it never permits replaying
    /// OPENDIR or RELEASEDIR.
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
    /// RELEASEDIR in one visit: the fence reads this open descriptor's row
    /// and the Workspace row, then the handle is closed, its lease released
    /// and its published replies retired. A closed Workspace still releases
    /// its descriptors.
    pub fn close_native_directory(
        &self,
        mount: NativeMount,
        serial: u64,
        handle: u64,
    ) -> OverlayResult<()> {
        self.atomic_cleanup(|| {
            let held = super::native_visit::Held::Directory(handle);
            let (state, _) = self.native_fence(mount, serial, held, false)?;
            self.close_native_directory_inner(NativeDirectory {
                mount,
                owner: handle,
                serial,
            })?;
            self.queue_closed_at(mount.route, &state)
        })
    }
    /// The caller established that this exact handle row is still open, and
    /// decides afterwards whether its closed Workspace can be reclaimed.
    ///
    /// A handle with at most `INLINE_PAGES` published replies is deleted
    /// here, rows and header, and queues nothing. Past that the header is
    /// marked closed and its bounded, indexed maintenance item retires the
    /// rows a window at a time; nothing is refused either way.
    pub(crate) fn close_native_directory_inner(
        &self,
        directory: NativeDirectory,
    ) -> OverlayResult<()> {
        let (ns, owner) = (directory.mount.route.ns, integer(directory.owner)?);
        self.file_ref(ns, integer(directory.serial)?, LeaseKind::FileHandle, false)?;
        let pages = self.query(
            StatementKind::Lease,
            crate::sql::COOKIE_PAGES,
            &[&ns, &owner, &0_i64],
            24,
            |_| Ok(()),
        )?;
        if pages.len() > INLINE_PAGES {
            self.execute(
                StatementKind::Lease,
                "UPDATE native_directory SET closed=1 WHERE ns=?1 AND owner=?2",
                &[&ns, &owner],
                16,
            )?;
            return self.enqueue(ns, crate::maintenance::NATIVE_DIRECTORY, owner, owner);
        }
        if !pages.is_empty() {
            self.execute(
                StatementKind::Lease,
                "DELETE FROM native_cookie WHERE ns=?1 AND owner=?2",
                &[&ns, &owner],
                16,
            )?;
        }
        self.execute(
            StatementKind::Lease,
            "DELETE FROM native_directory WHERE ns=?1 AND owner=?2",
            &[&ns, &owner],
            16,
        )?;
        Ok(())
    }
}
/// Published replies one RELEASEDIR deletes in its own job, one maintenance
/// turn retires, and one publication on a rewound handle deletes below the
/// floor. The LIMIT of `sql::COOKIE_PAGES` is this value plus one, kept
/// equal by hand.
pub(crate) const INLINE_PAGES: usize = 8;
