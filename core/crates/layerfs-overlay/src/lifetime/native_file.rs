//! Encoded native file handles retain the existing exact OpenFile owner.
use crate::{
    db::{integer, unsigned},
    BaseSource, NativeMount, OpenFile, Overlay, OverlayError, OverlayResult, StatementKind,
};

impl Overlay {
    /// A kernel GETATTR handle can denote a file or a directory. Classify both
    /// indexed associations in one attempted transaction; do not try one failed
    /// acquisition and then fall back to another handle kind.
    pub fn acquire_native_handle_source(
        &self,
        mount: NativeMount,
        request: u64,
        serial: u64,
        handle: u64,
    ) -> OverlayResult<BaseSource> {
        self.atomic(|| {
            let state = self.check_native_mount(mount)?;
            let rows = self.query(StatementKind::Lease,
                "SELECT 1 FROM native_file n JOIN file_handle f ON f.ns=n.ns AND f.owner=n.owner WHERE n.ns=?1 AND n.mount=?2 AND n.owner=?3 AND f.serial=?4 UNION ALL SELECT 1 FROM native_directory d WHERE d.ns=?1 AND d.mount=?2 AND d.owner=?3 AND d.serial=?4 AND d.closed=0",
                &[&mount.route.ns, &integer(mount.owner)?, &integer(handle)?, &integer(serial)?],
                32, |_| Ok(()))?;
            if rows.len() != 1 { return Err(OverlayError::Stale); }
            self.retain_native_source(mount, state, request, serial)
        })
    }
    /// Validate all connection/route/serial/handle fields before file use.
    pub fn native_file(
        &self,
        mount: NativeMount,
        serial: u64,
        handle: u64,
    ) -> OverlayResult<OpenFile> {
        self.check_native_attached(mount)?;
        self.query(StatementKind::Lease,
            "SELECT f.writable FROM native_file n JOIN file_handle f ON f.ns=n.ns AND f.owner=n.owner WHERE n.ns=?1 AND n.mount=?2 AND n.owner=?3 AND f.serial=?4",
            &[&mount.route.ns, &integer(mount.owner)?, &integer(handle)?, &integer(serial)?], 32,
            |r| Ok(OpenFile { route: mount.route, owner: handle, serial, writable: r.get(0)? }))?
            .pop().ok_or(OverlayError::Stale)
    }
    /// Observes the original request's open owner, without replay or adoption.
    pub fn retained_native_file(
        &self,
        mount: NativeMount,
        request: u64,
    ) -> OverlayResult<Option<OpenFile>> {
        self.check_native_attached(mount)?;
        self.query(StatementKind::Lease,
            "SELECT f.owner,f.serial,f.writable FROM native_file n JOIN file_handle f ON f.ns=n.ns AND f.owner=n.owner WHERE n.ns=?1 AND n.mount=?2 AND n.request=?3",
            &[&mount.route.ns, &integer(mount.owner)?, &request.to_be_bytes().as_slice()], 24,
            |r| Ok(OpenFile { route: mount.route, owner: unsigned(r, 0)?, serial: unsigned(r, 1)?, writable: r.get(2)? }))
            .map(|mut rows| rows.pop())
    }
    /// The descriptor protects the target while this independent request source
    /// is acquired. Later RELEASE/FORGET cannot dispose its processing metadata.
    pub fn acquire_native_file_source(
        &self,
        mount: NativeMount,
        request: u64,
        serial: u64,
        handle: u64,
    ) -> OverlayResult<BaseSource> {
        self.atomic(|| {
            let state = self.check_native_mount(mount)?;
            self.native_file(mount, serial, handle)?;
            self.retain_native_source(mount, state, request, serial)
        })
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
            let file = self.native_file(mount, serial, handle)?;
            self.close_file_inner(file)
        })
    }
}
