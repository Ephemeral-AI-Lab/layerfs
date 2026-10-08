//! Indexed native connection, source and kernel lookup ownership.
use crate::{
    db::{integer, unsigned},
    BaseSource, LeaseKind, NativeMount, NativeMountState, Overlay, OverlayError, OverlayResult,
    Route, StatementKind,
};

impl Overlay {
    /// The caller supplies the authenticated bound root serial. Its implicit
    /// connection reference is separate from explicit kernel lookup counts.
    pub fn create_native_mount(&self, route: Route, root: u64) -> OverlayResult<NativeMount> {
        if root == 0 {
            return Err(OverlayError::Invalid("zero native root"));
        }
        integer(root)?;
        self.atomic(|| {
            self.live(route)?;
            let owner = self.mint_owner(route)?;
            self.execute(
                StatementKind::Lease,
                "INSERT INTO native_mount(ns,owner,root,revoked) VALUES(?1,?2,?3,0)",
                &[&route.ns, &integer(owner)?, &integer(root)?],
                24,
            )?;
            let mount = NativeMount { route, owner, root };
            self.insert_native_lookup(mount, root, true, 0)?;
            self.set_native_parent(mount, root, root)?;
            Ok(mount)
        })
    }
    /// Observes the original connection, without adopting or replaying attach.
    pub fn retained_native_mount(&self, route: Route) -> OverlayResult<Option<NativeMount>> {
        self.state(route)?;
        self.query(
            StatementKind::Lease,
            "SELECT owner,root FROM native_mount WHERE ns=?1",
            &[&route.ns],
            8,
            |r| {
                Ok(NativeMount {
                    route,
                    owner: unsigned(r, 0)?,
                    root: unsigned(r, 1)?,
                })
            },
        )
        .map(|mut rows| rows.pop())
    }
    pub fn native_mount_state(&self, mount: NativeMount) -> OverlayResult<NativeMountState> {
        self.check_route(mount.route)?;
        let row = self
            .query(
                StatementKind::Lease,
                "SELECT owner,root,revoked FROM native_mount WHERE ns=?1",
                &[&mount.route.ns],
                8,
                |r| Ok((unsigned(r, 0)?, unsigned(r, 1)?, r.get::<_, bool>(2)?)),
            )?
            .pop();
        match row {
            None => Ok(NativeMountState::Gone),
            Some((owner, root, revoked)) if owner == mount.owner && root == mount.root => {
                Ok(if revoked {
                    NativeMountState::Revoked
                } else {
                    NativeMountState::Live
                })
            }
            Some(_) => Err(OverlayError::Stale),
        }
    }
    pub(crate) fn check_native_mount(&self, mount: NativeMount) -> OverlayResult<()> {
        self.live(mount.route)?;
        self.check_native_attached(mount)
    }
    pub(crate) fn check_native_attached(&self, mount: NativeMount) -> OverlayResult<()> {
        self.state(mount.route)?;
        if self.native_mount_state(mount)? != NativeMountState::Live {
            return Err(OverlayError::Stale);
        }
        Ok(())
    }
    /// One exact native request's source. Engine-generated class2 identities
    /// cannot collide with caller-issued class0 source IDs or class1 read IDs.
    pub fn acquire_native_source(
        &self,
        mount: NativeMount,
        request: u64,
        serial: u64,
    ) -> OverlayResult<BaseSource> {
        self.atomic(|| {
            self.check_native_mount(mount)?;
            if self.native_lookup_row(mount, serial)?.is_none() {
                return Err(OverlayError::Stale);
            }
            self.retain_native_source(mount, request, serial)
        })
    }
    pub(crate) fn retain_native_source(
        &self,
        mount: NativeMount,
        request: u64,
        serial: u64,
    ) -> OverlayResult<BaseSource> {
        let state = self.state(mount.route)?;
        let owner = self.mint_owner(mount.route)?;
        self.execute(
            StatementKind::Lease,
            "INSERT INTO native_source VALUES(?1,?2,?3,?4,?5,0)",
            &[
                &mount.route.ns,
                &integer(mount.owner)?,
                &request.to_be_bytes().as_slice(),
                &integer(owner)?,
                &integer(serial)?,
            ],
            40,
        )?;
        self.execute(
            StatementKind::Lease,
            crate::sql::BASE_SOURCE_INSERT,
            &[
                &mount.route.ns,
                &integer(owner)?,
                &state.base_root.as_slice(),
                &2_i64,
            ],
            56,
        )?;
        self.execute(
            StatementKind::Workspace,
            crate::sql::BASE_SOURCE_INCREMENT,
            &[&mount.route.ns],
            8,
        )?;
        self.execute(
            StatementKind::Lease,
            "INSERT INTO lease VALUES(?1,5,?2,?3)",
            &[&mount.route.ns, &integer(owner)?, &integer(serial)?],
            24,
        )?;
        self.file_ref(
            mount.route.ns,
            integer(serial)?,
            LeaseKind::FileReader,
            true,
        )?;
        Ok(BaseSource {
            route: mount.route,
            owner,
            class: 2,
            root: state.base_root,
            installed: state.installed,
        })
    }

    pub fn retained_native_source(
        &self,
        mount: NativeMount,
        request: u64,
    ) -> OverlayResult<Option<BaseSource>> {
        self.check_native_attached(mount)?;
        let installed = self.state(mount.route)?.installed;
        self.query(StatementKind::Lease,
            "SELECT s.owner,b.base_root FROM native_source s JOIN base_source b ON b.ns=s.ns AND b.kind=2 AND b.owner=s.owner WHERE s.ns=?1 AND s.mount=?2 AND s.request=?3",
            &[&mount.route.ns, &integer(mount.owner)?, &request.to_be_bytes().as_slice()], 24,
            |r| Ok(BaseSource { route: mount.route, owner: unsigned(r, 0)?, class: 2,
                root: r.get::<_, Vec<u8>>(1)?.try_into().map_err(|_| rusqlite::Error::InvalidQuery)?, installed }))
            .map(|mut rows| rows.pop())
    }
    pub(crate) fn check_native_source(
        &self,
        mount: NativeMount,
        source: BaseSource,
    ) -> OverlayResult<([u8; 8], u64)> {
        self.check_native_mount(mount)?;
        if source.route != mount.route || source.class != 2 {
            return Err(OverlayError::Stale);
        }
        self.source_state(source)?;
        self.query(StatementKind::Lease,
            "SELECT request,serial FROM native_source WHERE ns=?1 AND mount=?2 AND owner=?3 AND decided=0",
            &[&mount.route.ns, &integer(mount.owner)?, &integer(source.owner)?], 24,
            |r| Ok((r.get::<_, Vec<u8>>(0)?.try_into().map_err(|_| rusqlite::Error::InvalidQuery)?, unsigned(r, 1)?)))?
            .pop().ok_or(OverlayError::Stale)
    }
    pub(crate) fn release_native_source(&self, source: BaseSource) -> OverlayResult<()> {
        let (serial, directory) = self
            .query(
                StatementKind::Lease,
                "SELECT serial,(SELECT directory FROM native_directory_read WHERE ns=?1 AND owner=?2) FROM native_source WHERE ns=?1 AND owner=?2",
                &[&source.route.ns, &integer(source.owner)?],
                16,
                |r| Ok((unsigned(r, 0)?, r.get::<_, Option<i64>>(1)?)),
            )?
            .pop()
            .ok_or(OverlayError::Stale)?;
        self.execute(
            StatementKind::Lease,
            "DELETE FROM native_source WHERE ns=?1 AND owner=?2",
            &[&source.route.ns, &integer(source.owner)?],
            16,
        )?;
        self.execute(
            StatementKind::Lease,
            "DELETE FROM lease WHERE ns=?1 AND kind=5 AND owner=?2 AND resource=?3",
            &[&source.route.ns, &integer(source.owner)?, &integer(serial)?],
            24,
        )?;
        self.file_ref(
            source.route.ns,
            integer(serial)?,
            LeaseKind::FileReader,
            false,
        )?;
        if let Some(directory) = directory {
            self.queue_native_directory(source.route.ns, directory)?;
        }
        Ok(())
    }
    pub(crate) fn native_lookup_row(
        &self,
        mount: NativeMount,
        serial: u64,
    ) -> OverlayResult<Option<(u64, u64, bool)>> {
        self.query(StatementKind::Lease,
            "SELECT owner,nlookup,implicit FROM native_lookup WHERE ns=?1 AND mount=?2 AND serial=?3",
            &[&mount.route.ns, &integer(mount.owner)?, &integer(serial)?], 24,
            |r| Ok((unsigned(r, 0)?, unsigned(r, 1)?, r.get(2)?)))
            .map(|mut rows| rows.pop())
    }
    pub fn native_lookup_count(
        &self,
        mount: NativeMount,
        serial: u64,
    ) -> OverlayResult<Option<u64>> {
        self.check_native_attached(mount)?;
        Ok(self
            .native_lookup_row(mount, serial)?
            .map(|(_, count, _)| count))
    }
    pub(crate) fn insert_native_lookup(
        &self,
        mount: NativeMount,
        serial: u64,
        implicit: bool,
        count: u64,
    ) -> OverlayResult<()> {
        let owner = self.mint_owner(mount.route)?;
        self.execute(
            StatementKind::Lease,
            "INSERT INTO native_lookup VALUES(?1,?2,?3,?4,?5,?6)",
            &[
                &mount.route.ns,
                &integer(mount.owner)?,
                &integer(serial)?,
                &integer(owner)?,
                &integer(count)?,
                &implicit,
            ],
            48,
        )?;
        self.execute(
            StatementKind::Lease,
            "INSERT INTO lease VALUES(?1,9,?2,?3)",
            &[&mount.route.ns, &integer(owner)?, &integer(serial)?],
            24,
        )?;
        self.file_ref(
            mount.route.ns,
            integer(serial)?,
            LeaseKind::LookupOwner,
            true,
        )
    }
    pub(crate) fn add_native_lookup(&self, mount: NativeMount, serial: u64) -> OverlayResult<()> {
        if let Some((_, count, _)) = self.native_lookup_row(mount, serial)? {
            let next = count
                .checked_add(1)
                .ok_or(OverlayError::Invalid("native lookup overflow"))?;
            self.execute(
                StatementKind::Lease,
                "UPDATE native_lookup SET nlookup=?4 WHERE ns=?1 AND mount=?2 AND serial=?3",
                &[
                    &mount.route.ns,
                    &integer(mount.owner)?,
                    &integer(serial)?,
                    &integer(next)?,
                ],
                32,
            )?;
            Ok(())
        } else {
            self.insert_native_lookup(mount, serial, false, 1)
        }
    }
    pub(crate) fn drop_native_lookup(
        &self,
        mount: NativeMount,
        serial: u64,
        owner: u64,
    ) -> OverlayResult<()> {
        self.execute(
            StatementKind::Lease,
            "DELETE FROM native_lookup WHERE ns=?1 AND mount=?2 AND serial=?3 AND owner=?4",
            &[
                &mount.route.ns,
                &integer(mount.owner)?,
                &integer(serial)?,
                &integer(owner)?,
            ],
            32,
        )?;
        self.execute(
            StatementKind::Lease,
            "DELETE FROM lease WHERE ns=?1 AND kind=9 AND owner=?2 AND resource=?3",
            &[&mount.route.ns, &integer(owner)?, &integer(serial)?],
            24,
        )?;
        self.file_ref(
            mount.route.ns,
            integer(serial)?,
            LeaseKind::LookupOwner,
            false,
        )
    }
    /// Exact no-reply kernel decrement. Failure retains the original caller's
    /// record; it never compensates an unavailable send/delivery outcome.
    pub fn forget_native(&self, mount: NativeMount, serial: u64, count: u64) -> OverlayResult<()> {
        if count == 0 {
            return Err(OverlayError::Invalid("zero native forget"));
        }
        self.atomic_cleanup(|| {
            self.check_native_attached(mount)?;
            let (owner, held, implicit) = self
                .native_lookup_row(mount, serial)?
                .ok_or(OverlayError::Stale)?;
            let after = held
                .checked_sub(count)
                .ok_or(OverlayError::Invalid("native lookup underflow"))?;
            if after == 0 && !implicit {
                self.drop_native_lookup(mount, serial, owner)?;
            } else {
                self.execute(
                    StatementKind::Lease,
                    "UPDATE native_lookup SET nlookup=?4 WHERE ns=?1 AND mount=?2 AND serial=?3",
                    &[
                        &mount.route.ns,
                        &integer(mount.owner)?,
                        &integer(serial)?,
                        &integer(after)?,
                    ],
                    32,
                )?;
            }
            self.queue_closed(mount.route)
        })
    }
    /// Call only after detach and complete native/service consumer drain.
    /// Backed source/read associations provide an additional before-effect fence.
    pub fn revoke_native_mount(&self, mount: NativeMount) -> OverlayResult<()> {
        self.atomic_cleanup(|| {
            self.check_native_attached(mount)?;
            let held = self.query(StatementKind::Lease,
                "SELECT EXISTS(SELECT 1 FROM native_source WHERE ns=?1 AND mount=?2) OR EXISTS(SELECT 1 FROM native_read WHERE ns=?1 AND mount=?2) OR EXISTS(SELECT 1 FROM native_file WHERE ns=?1 AND mount=?2) OR EXISTS(SELECT 1 FROM native_directory WHERE ns=?1 AND mount=?2 AND closed=0)",
                &[&mount.route.ns, &integer(mount.owner)?], 16, |r| r.get::<_, bool>(0))?[0];
            if held { return Err(OverlayError::BaseSourcesPending); }
            self.execute(StatementKind::Lease, "UPDATE native_mount SET revoked=1 WHERE ns=?1 AND owner=?2",
                &[&mount.route.ns, &integer(mount.owner)?], 16)?;
            self.enqueue(mount.route.ns, crate::maintenance::NATIVE, integer(mount.owner)?, integer(mount.owner)?)
        })
    }
}
