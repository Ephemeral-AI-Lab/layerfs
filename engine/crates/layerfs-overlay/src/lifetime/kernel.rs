//! Indexed kernel connection and kernel lookup ownership.
use crate::{
    db::integer, KernelMount, KernelMountState, Overlay, OverlayError, OverlayResult,
    ReferenceKind, WorkspaceState,
};

impl Overlay {
    /// The caller supplies the authenticated bound root serial. Its implicit
    /// connection reference is separate from explicit kernel lookup counts.
    pub fn create_kernel_mount(&self, root: u64) -> OverlayResult<KernelMount> {
        if root == 0 {
            return Err(OverlayError::Invalid("zero kernel root"));
        }
        integer(root)?;
        self.atomic(|| {
            self.live()?;
            let held = self.open.borrow().mount;
            match held {
                Some((_, false)) => return Err(OverlayError::Invalid("duplicate kernel mount")),
                Some((revoked, true)) => self.retire_revoked_mount(revoked)?,
                None => {}
            }
            let owner = self.mint_owner()?;
            let mount = KernelMount {
                engine: self.identity,
                owner,
                root,
            };
            self.open.borrow_mut().mount = Some((mount, false));
            self.insert_kernel_lookup(mount, root, true, 0)?;
            self.set_kernel_parent(mount, root, root)?;
            Ok(mount)
        })
    }
    /// A revoked mount is drained and can receive nothing: its remaining
    /// kernel references and parents leave with it when the Workspace is
    /// attached again.
    fn retire_revoked_mount(&self, revoked: KernelMount) -> OverlayResult<()> {
        let (references, parents) = {
            let state = self.open.borrow();
            let references = state
                .kernel
                .rows
                .range((revoked.owner, 0)..=(revoked.owner, u64::MAX))
                .map(|(key, _)| *key)
                .collect::<Vec<_>>();
            let parents = state
                .parents
                .rows
                .iter()
                .filter(|(_, (owner, _))| *owner == revoked.owner)
                .map(|(key, _)| *key)
                .collect::<Vec<_>>();
            (references, parents)
        };
        for key in references {
            self.open.borrow_mut().kernel.remove(key);
            self.file_ref(integer(key.1)?, ReferenceKind::Lookup, false)?;
        }
        for key in parents {
            self.open.borrow_mut().parents.remove(key);
        }
        Ok(())
    }
    /// Observes the original connection, without adopting or replaying attach.
    pub fn retained_kernel_mount(&self) -> OverlayResult<Option<KernelMount>> {
        self.state()?;
        Ok(self.open.borrow().mount.map(|(mount, _)| mount))
    }
    pub fn kernel_mount_state(&self, mount: KernelMount) -> OverlayResult<KernelMountState> {
        self.available()?;
        if mount.engine != self.identity {
            return Err(OverlayError::Stale);
        }
        match self.open.borrow().mount.as_ref() {
            None => Ok(KernelMountState::Gone),
            Some((held, revoked)) if *held == mount => Ok(if *revoked {
                KernelMountState::Revoked
            } else {
                KernelMountState::Live
            }),
            Some(_) => Err(OverlayError::Stale),
        }
    }
    /// This mount attached to its Workspace, closed or not, and the
    /// Workspace row read for it.
    pub(crate) fn check_kernel_attached(
        &self,
        mount: KernelMount,
    ) -> OverlayResult<WorkspaceState> {
        let state = self.state()?;
        self.require_kernel_live(mount)?;
        Ok(state)
    }
    fn require_kernel_live(&self, mount: KernelMount) -> OverlayResult<()> {
        if self.kernel_mount_state(mount)? != KernelMountState::Live {
            return Err(OverlayError::Stale);
        }
        Ok(())
    }
    pub(crate) fn kernel_lookup_row(
        &self,
        mount: KernelMount,
        serial: u64,
    ) -> OverlayResult<Option<(u64, u64, bool)>> {
        self.available()?;
        integer(serial)?;
        Ok(self
            .open
            .borrow()
            .kernel
            .rows
            .get(&(mount.owner, serial))
            .map(|entry| (entry.owner, entry.count, entry.implicit)))
    }
    pub(crate) fn insert_kernel_lookup(
        &self,
        mount: KernelMount,
        serial: u64,
        implicit: bool,
        count: u64,
    ) -> OverlayResult<()> {
        self.insert_kernel_lookup_row(mount, serial, implicit, count)?;
        self.file_ref(integer(serial)?, ReferenceKind::Lookup, true)
    }
    /// The lookup row without its file reference, which the caller takes in
    /// the same transaction. A row that already exists is a failure. The
    /// row is the reference's whole custody: no `lease` row stands for it.
    pub(crate) fn insert_kernel_lookup_row(
        &self,
        mount: KernelMount,
        serial: u64,
        implicit: bool,
        count: u64,
    ) -> OverlayResult<()> {
        let key = (mount.owner, serial);
        integer(serial)?;
        integer(count)?;
        if count == 0 && !implicit {
            return Err(OverlayError::Invalid("empty kernel reference"));
        }
        let owner = self.mint_owner()?;
        let mut state = self.open.borrow_mut();
        if state.kernel.rows.contains_key(&key) {
            return Err(OverlayError::Invalid("duplicate kernel reference"));
        }
        state.kernel.put(
            key,
            crate::open::KernelReference {
                owner,
                count,
                implicit,
            },
        );
        Ok(())
    }
    pub(crate) fn add_kernel_lookup(&self, mount: KernelMount, serial: u64) -> OverlayResult<()> {
        if let Some((owner, count, implicit)) = self.kernel_lookup_row(mount, serial)? {
            let count = count
                .checked_add(1)
                .filter(|count| *count <= i64::MAX as u64)
                .ok_or(OverlayError::Invalid("kernel lookup overflow"))?;
            self.open.borrow_mut().kernel.put(
                (mount.owner, serial),
                crate::open::KernelReference {
                    owner,
                    count,
                    implicit,
                },
            );
            Ok(())
        } else {
            self.insert_kernel_lookup(mount, serial, false, 1)
        }
    }
    pub(crate) fn drop_kernel_lookup(
        &self,
        mount: KernelMount,
        serial: u64,
        owner: u64,
    ) -> OverlayResult<()> {
        let key = (mount.owner, serial);
        if self
            .open
            .borrow()
            .kernel
            .rows
            .get(&key)
            .map(|entry| entry.owner)
            != Some(owner)
        {
            return Err(OverlayError::Stale);
        }
        self.open.borrow_mut().kernel.remove(key);
        self.file_ref(integer(serial)?, ReferenceKind::Lookup, false)
    }
    /// FORGET's fence in one statement: the Workspace row, closed or not,
    /// this mount attached and not revoked, and the lookup row the request
    /// names (its owner, count and whether it is the connection's implicit
    /// one). A missing or foreign Workspace, mount or lookup row is Stale.
    fn forget_fence(
        &self,
        mount: KernelMount,
        serial: u64,
    ) -> OverlayResult<(WorkspaceState, u64, u64, bool)> {
        let state = self.check_kernel_attached(mount)?;
        let (owner, count, implicit) = self
            .kernel_lookup_row(mount, serial)?
            .ok_or(OverlayError::Stale)?;
        Ok((state, owner, count, implicit))
    }
    /// Exact no-reply kernel decrement. Failure retains the original caller's
    /// record; it never compensates an unavailable send/delivery outcome.
    pub fn kernel_forget(&self, mount: KernelMount, serial: u64, count: u64) -> OverlayResult<()> {
        if count == 0 {
            return Err(OverlayError::Invalid("zero kernel forget"));
        }
        self.atomic_cleanup(|| {
            let (_, owner, held, implicit) = self.forget_fence(mount, serial)?;
            let after = held
                .checked_sub(count)
                .ok_or(OverlayError::Invalid("kernel lookup underflow"))?;
            if after == 0 && !implicit {
                self.drop_kernel_lookup(mount, serial, owner)?;
            } else {
                self.open.borrow_mut().kernel.put(
                    (mount.owner, serial),
                    crate::open::KernelReference {
                        owner,
                        count: after,
                        implicit,
                    },
                );
            }
            // The release changes no lifecycle, capture or base reader.
            Ok(())
        })
    }
    /// Call only after detach and complete kernel/service consumer drain. A
    /// request records nothing here, so the fixed logical mark waits for no
    /// row. An open file or directory handle whose kernel RELEASE can no longer arrive
    /// stops representing a consumer here; its allocation goes with file close.
    pub fn revoke_kernel_mount(&self, mount: KernelMount) -> OverlayResult<()> {
        self.atomic_cleanup(|| {
            self.check_kernel_attached(mount)?;
            self.open.borrow_mut().mount = Some((mount, true));
            Ok(())
        })
    }
}
