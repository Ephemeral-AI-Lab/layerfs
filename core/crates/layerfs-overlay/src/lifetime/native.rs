//! Indexed native connection and kernel lookup ownership.
use crate::{
    db::{integer, unsigned},
    LeaseKind, NativeMount, NativeMountState, Overlay, OverlayError, OverlayResult, Route,
    StatementKind, WorkspaceState,
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
            let owner = self.mint_owner()?;
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
    /// This mount attached to its Workspace, closed or not, and the
    /// Workspace row read for it.
    pub(crate) fn check_native_attached(
        &self,
        mount: NativeMount,
    ) -> OverlayResult<WorkspaceState> {
        let state = self.state(mount.route)?;
        self.require_native_live(mount)?;
        Ok(state)
    }
    fn require_native_live(&self, mount: NativeMount) -> OverlayResult<()> {
        if self.native_mount_state(mount)? != NativeMountState::Live {
            return Err(OverlayError::Stale);
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
        self.insert_native_lookup_row(mount, serial, implicit, count)?;
        self.file_ref(
            mount.route.ns,
            integer(serial)?,
            LeaseKind::LookupOwner,
            true,
        )
    }
    /// The lookup row without its file reference, which the caller takes in
    /// the same transaction. A row that already exists is a failure. The
    /// row is the reference's whole custody: no `lease` row stands for it.
    pub(crate) fn insert_native_lookup_row(
        &self,
        mount: NativeMount,
        serial: u64,
        implicit: bool,
        count: u64,
    ) -> OverlayResult<()> {
        let owner = self.mint_owner()?;
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
        Ok(())
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
            let state = self.check_native_attached(mount)?;
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
            // The release changes no lifecycle, capture or base reader.
            self.queue_closed_at(mount.route, &state)
        })
    }
    /// Call only after detach and complete native/service consumer drain. A
    /// request records nothing here, so the fixed logical mark waits for no
    /// row. An open file or directory handle whose kernel RELEASE can no longer arrive
    /// stops representing a consumer here; bounded maintenance retires it.
    pub fn revoke_native_mount(&self, mount: NativeMount) -> OverlayResult<()> {
        self.atomic_cleanup(|| {
            self.check_native_attached(mount)?;
            self.execute(
                StatementKind::Lease,
                "UPDATE native_mount SET revoked=1 WHERE ns=?1 AND owner=?2",
                &[&mount.route.ns, &integer(mount.owner)?],
                16,
            )?;
            self.enqueue(
                mount.route.ns,
                crate::maintenance::NATIVE,
                integer(mount.owner)?,
                integer(mount.owner)?,
            )
        })
    }
}
