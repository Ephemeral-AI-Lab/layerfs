//! Exact lookup ownership, including non-file metadata after removal.
use crate::{
    db::{integer, unsigned},
    inode, BaseSource, Inode, InodeKind, LeaseKind, LookupOwner, Overlay, OverlayError,
    OverlayResult, Route, StatementKind,
};
impl Overlay {
    /// The authenticated root serial distinguishes its canonical zero count
    /// from removal. Base metadata is acquired outside SQL; current rows decide.
    pub fn acquire_lookup(
        &self,
        source: BaseSource,
        request: u64,
        base: &Inode,
        root: u64,
    ) -> OverlayResult<LookupOwner> {
        inode::check(base)?;
        let request = integer(request)?;
        integer(root)?;
        if request == 0 || root == 0 {
            return Err(OverlayError::Invalid("lookup identity"));
        }
        self.atomic(|| {
            let state = self.source_state(source)?;
            if state.closed {
                return Err(OverlayError::Closed);
            }
            let current = self
                .inode_at(source.route, base.serial, state.active, state.installed)?
                .unwrap_or_else(|| base.clone());
            if current.nlink == 0
                && !(current.kind == InodeKind::Directory && current.serial == root)
            {
                return Err(OverlayError::Missing);
            }
            let owner = self.mint_owner(source.route)?;
            let serial = integer(current.serial)?;
            self.execute(
                StatementKind::Lease,
                "INSERT INTO lookup_owner VALUES(?1,?2,?3,?4)",
                &[&source.route.ns, &request, &integer(owner)?, &serial],
                32,
            )?;
            self.execute(
                StatementKind::Lease,
                "INSERT INTO lease VALUES(?1,9,?2,?3)",
                &[&source.route.ns, &integer(owner)?, &serial],
                24,
            )?;
            self.file_ref(source.route.ns, serial, LeaseKind::LookupOwner, true)?;
            Ok(LookupOwner {
                route: source.route,
                owner,
                serial: current.serial,
            })
        })
    }
    /// One processing reference outlives lookup release and logical close.
    pub fn acquire_lookup_read(
        &self,
        source: BaseSource,
        lookup: LookupOwner,
        request: u64,
    ) -> OverlayResult<crate::FileRead> {
        let request = integer(request)?;
        if request == 0 {
            return Err(OverlayError::Invalid("zero read request"));
        }
        self.atomic(|| {
            let state = self.source_state(source)?;
            if state.closed {
                return Err(OverlayError::Closed);
            }
            if lookup.route != source.route {
                return Err(OverlayError::Stale);
            }
            self.check_lookup(lookup)?;
            self.retain_serial_read(source, lookup.serial, request)
        })
    }
    pub fn retained_lookup(
        &self,
        route: Route,
        request: u64,
    ) -> OverlayResult<Option<LookupOwner>> {
        self.state(route)?;
        self.query(
            StatementKind::Lease,
            "SELECT owner,serial FROM lookup_owner WHERE ns=?1 AND request=?2",
            &[&route.ns, &integer(request)?],
            16,
            |r| {
                Ok(LookupOwner {
                    route,
                    owner: unsigned(r, 0)?,
                    serial: unsigned(r, 1)?,
                })
            },
        )
        .map(|mut rows| rows.pop())
    }
    pub fn check_lookup(&self, lookup: LookupOwner) -> OverlayResult<()> {
        self.state(lookup.route)?;
        if self
            .query(
                StatementKind::Lease,
                "SELECT 1 FROM lookup_owner WHERE ns=?1 AND owner=?2 AND serial=?3",
                &[
                    &lookup.route.ns,
                    &integer(lookup.owner)?,
                    &integer(lookup.serial)?,
                ],
                24,
                |_| Ok(()),
            )?
            .is_empty()
        {
            return Err(OverlayError::Stale);
        }
        Ok(())
    }
    /// Call only after native/request continuations using this reference finish.
    pub fn release_lookup(&self, lookup: LookupOwner) -> OverlayResult<()> {
        self.atomic_cleanup(|| {
            self.check_lookup(lookup)?;
            self.execute(
                StatementKind::Lease,
                "DELETE FROM lookup_owner WHERE ns=?1 AND owner=?2",
                &[&lookup.route.ns, &integer(lookup.owner)?],
                16,
            )?;
            self.execute(
                StatementKind::Lease,
                "DELETE FROM lease WHERE ns=?1 AND kind=9 AND owner=?2 AND resource=?3",
                &[
                    &lookup.route.ns,
                    &integer(lookup.owner)?,
                    &integer(lookup.serial)?,
                ],
                24,
            )?;
            self.file_ref(
                lookup.route.ns,
                integer(lookup.serial)?,
                LeaseKind::LookupOwner,
                false,
            )?;
            self.queue_closed(lookup.route)
        })
    }
}
