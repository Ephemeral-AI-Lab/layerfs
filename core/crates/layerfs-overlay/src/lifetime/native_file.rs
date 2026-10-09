//! Encoded native file handles retain the existing exact OpenFile owner.
use crate::{
    db::{integer, unsigned},
    NativeMount, OpenFile, Overlay, OverlayError, OverlayResult, StatementKind,
};

/// A kernel request identity as the descriptor row stores it: the same 64
/// bits, so two requests of one mount have one row key exactly when they are
/// one request.
pub(crate) const fn native_request(request: u64) -> i64 {
    i64::from_ne_bytes(request.to_ne_bytes())
}
impl Overlay {
    /// Validate all connection/route/serial/handle fields before file use.
    pub fn native_file(
        &self,
        mount: NativeMount,
        serial: u64,
        handle: u64,
    ) -> OverlayResult<OpenFile> {
        self.check_native_attached(mount)?;
        self.native_file_row(mount, serial, handle)
    }
    /// The same descriptor inside a job that already checked its mount.
    pub(crate) fn native_file_row(
        &self,
        mount: NativeMount,
        serial: u64,
        handle: u64,
    ) -> OverlayResult<OpenFile> {
        self.query(
            StatementKind::Lease,
            "SELECT writable FROM file_handle WHERE ns=?1 AND owner=?3 AND mount=?2 AND serial=?4",
            &[
                &mount.route.ns,
                &integer(mount.owner)?,
                &integer(handle)?,
                &integer(serial)?,
            ],
            32,
            |r| {
                Ok(OpenFile {
                    route: mount.route,
                    owner: handle,
                    serial,
                    writable: r.get(0)?,
                })
            },
        )?
        .pop()
        .ok_or(OverlayError::Stale)
    }
    /// Observes the original request's open owner, without replay or adoption.
    pub fn retained_native_file(
        &self,
        mount: NativeMount,
        request: u64,
    ) -> OverlayResult<Option<OpenFile>> {
        self.check_native_attached(mount)?;
        self.query(
            StatementKind::Lease,
            "SELECT owner,serial,writable FROM file_handle WHERE ns=?1 AND mount=?2 AND request=?3",
            &[
                &mount.route.ns,
                &integer(mount.owner)?,
                &native_request(request),
            ],
            24,
            |r| {
                Ok(OpenFile {
                    route: mount.route,
                    owner: unsigned(r, 0)?,
                    serial: unsigned(r, 1)?,
                    writable: r.get(2)?,
                })
            },
        )
        .map(|mut rows| rows.pop())
    }
    /// Close the exact handle once. Existing independently acquired sources/read
    /// windows survive; foreign or reused encoded handles are rejected.
    pub fn close_native_file(
        &self,
        mount: NativeMount,
        serial: u64,
        handle: u64,
    ) -> OverlayResult<()> {
        self.atomic_cleanup(|| {
            // The fence read this descriptor's row and the Workspace row;
            // a closed Workspace still releases its descriptors.
            let (state, writable) = self.native_fence(
                mount,
                serial,
                super::native_visit::Held::File(handle),
                false,
            )?;
            let file = OpenFile {
                route: mount.route,
                owner: handle,
                serial,
                writable,
            };
            self.close_held_file(file, &state)
        })
    }
}
