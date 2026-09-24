//! One projection binding, bounded reply exclusion, and checked mutation completion.
use super::{lifecycle::MountLease, state::State};
use crate::{backing::budget::Charge, *};
use std::{alloc::Layout, io, mem::size_of, time::Instant};

const PROJECTION_REPLIES: usize = 2;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MutationOrigin {
    Local,
    ProjectionWrite { append: bool },
    ProjectionSize,
    ProjectionAttributes,
    ProjectionMkdir,
    ProjectionCreate,
    ProjectionSymlink,
    ProjectionMknod,
    ProjectionLink,
    ProjectionUnlink,
    ProjectionRmdir,
    ProjectionRename,
}
impl MutationOrigin {
    pub fn projected(self) -> bool {
        self != Self::Local
    }
    pub fn append(self, stored: bool) -> bool {
        match self {
            Self::Local => stored,
            Self::ProjectionWrite { append } => append,
            Self::ProjectionSize
            | Self::ProjectionAttributes
            | Self::ProjectionMkdir
            | Self::ProjectionCreate
            | Self::ProjectionSymlink
            | Self::ProjectionMknod
            | Self::ProjectionLink
            | Self::ProjectionUnlink
            | Self::ProjectionRmdir
            | Self::ProjectionRename => false,
        }
    }
}

pub(crate) struct ProjectionState {
    pub delivery: Option<ProjectionInvalidation>,
    pub replies: usize,
    pub mutation_held: bool,
    pub status: CoherenceStatus,
    callback_charge: Option<Charge>,
    _charge: Charge,
}
impl ProjectionState {
    pub fn reserve(workspace: &Workspace) -> Result<Box<Self>, WorkspaceError> {
        let bytes = size_of::<Self>()
            + (PROJECTION_REPLIES - 1) * size_of::<ProjectionReplyPermit>()
            + size_of::<ProjectionMutationPermit>()
            + 64;
        let charge = workspace.host.budget.reserve(bytes)?;
        Ok(Box::new(Self {
            delivery: None,
            replies: 0,
            mutation_held: false,
            status: CoherenceStatus::Unbound,
            callback_charge: None,
            _charge: charge,
        }))
    }
    pub fn in_flight(&self) -> bool {
        matches!(self.status, CoherenceStatus::Pending { .. })
    }
}

/// Covers projection result selection through its final reply attempt. The
/// guard is not cloneable; dropping it performs no backing or notification I/O.
pub struct ProjectionReplyPermit {
    workspace: Workspace,
}
impl Drop for ProjectionReplyPermit {
    fn drop(&mut self) {
        if let Ok(mut state) = self.workspace.state() {
            if let Some(projection) = &mut state.projection {
                projection.replies = projection
                    .replies
                    .checked_sub(1)
                    .expect("one release per admitted projection reply");
            }
        }
    }
}

/// Owns one projected mutation attempt and its reply boundary. Keep this permit
/// until the projection has attempted to send its reply, including errors.
pub struct ProjectionMutationPermit {
    workspace: Workspace,
    deadline: Instant,
    used: bool,
}
impl ProjectionMutationPermit {
    /// Makes one attempt, using the earlier of this deadline and admission's
    /// deadline. Current kernel append flags are supplied per write; append
    /// requires the supplied offset to equal live EOF.
    pub fn write_file(
        &mut self,
        handle: HandleId,
        offset: u64,
        replacement: &OwnedPayload,
        append: bool,
        deadline: Instant,
    ) -> Result<MutationReceipt, WorkspaceError> {
        if self.used {
            return Err(WorkspaceError::InvalidInput);
        }
        self.used = true;
        self.workspace.write_file_from(
            handle,
            offset,
            replacement,
            deadline.min(self.deadline),
            MutationOrigin::ProjectionWrite { append },
        )
    }
    /// Publishes a size-only SETATTR and returns its exact attributes. The kernel
    /// owns post-reply cache invalidation; this operation sends no notification.
    pub fn set_len(
        &mut self,
        serial: u64,
        length: u64,
        handle: Option<HandleId>,
        deadline: Instant,
    ) -> Result<NodeAttributes, WorkspaceError> {
        if self.used {
            return Err(WorkspaceError::InvalidInput);
        }
        self.used = true;
        self.workspace
            .set_len_projected(serial, length, handle, deadline.min(self.deadline))
    }
    /// Publishes one directory and returns one Projection lookup reference. The
    /// kernel owns entry installation and parent invalidation after the reply;
    /// this operation sends no notification while its parent lock is held.
    pub fn mkdir(
        &mut self,
        parent: u64,
        name: &[u8],
        mode: u32,
        umask: u32,
        deadline: Instant,
    ) -> Result<NodeAttributes, WorkspaceError> {
        if self.used {
            return Err(WorkspaceError::InvalidInput);
        }
        self.used = true;
        self.workspace.mkdir_from(
            parent,
            name,
            mode,
            umask,
            deadline.min(self.deadline),
            MutationOrigin::ProjectionMkdir,
        )
    }
    /// Creates or opens one regular file with one Projection lookup reference
    /// and ready handle. Keep this permit through the CREATE reply attempt;
    /// the kernel owns entry installation and cache invalidation, so this
    /// operation sends no notification while the parent lock is held.
    pub fn create_file(
        &mut self,
        parent: u64,
        name: &[u8],
        options: FileCreateOptions,
        deadline: Instant,
    ) -> Result<(NodeAttributes, HandleId), WorkspaceError> {
        if self.used {
            return Err(WorkspaceError::InvalidInput);
        }
        self.used = true;
        self.workspace.create_file_from(
            parent,
            name,
            options,
            deadline.min(self.deadline),
            MutationOrigin::ProjectionCreate,
        )
    }
    /// Publishes one symlink and returns one Projection lookup reference. Keep
    /// this permit through the entry reply; the kernel installs the entry and
    /// invalidates its parent, so this operation sends no reverse notification.
    pub fn symlink(
        &mut self,
        parent: u64,
        name: &[u8],
        target: &[u8],
        deadline: Instant,
    ) -> Result<NodeAttributes, WorkspaceError> {
        if self.used {
            return Err(WorkspaceError::InvalidInput);
        }
        self.used = true;
        self.workspace.symlink_from(
            parent,
            name,
            target,
            deadline.min(self.deadline),
            MutationOrigin::ProjectionSymlink,
        )
    }
    /// Publishes one regular file without opening a handle. Keep this permit
    /// through the entry reply; the kernel owns entry installation and parent
    /// invalidation, so this operation sends no reverse notification.
    pub fn mknod(
        &mut self,
        parent: u64,
        name: &[u8],
        mode: u32,
        umask: u32,
        deadline: Instant,
    ) -> Result<NodeAttributes, WorkspaceError> {
        if self.used {
            return Err(WorkspaceError::InvalidInput);
        }
        self.used = true;
        self.workspace.mknod_from(
            parent,
            name,
            mode,
            umask,
            deadline.min(self.deadline),
            MutationOrigin::ProjectionMknod,
        )
    }
    /// Publishes one additional name for an existing regular file.
    pub fn link(
        &mut self,
        parent: u64,
        name: &[u8],
        serial: u64,
        deadline: Instant,
    ) -> Result<NodeAttributes, WorkspaceError> {
        if self.used {
            return Err(WorkspaceError::InvalidInput);
        }
        self.used = true;
        self.workspace.link_from(
            parent,
            name,
            serial,
            deadline.min(self.deadline),
            MutationOrigin::ProjectionLink,
        )
    }
    /// Removes one regular-file or symlink name. The kernel owns the parent lock
    /// through the reply, so this operation sends no reverse notification.
    pub fn unlink(
        &mut self,
        parent: u64,
        name: &[u8],
        deadline: Instant,
    ) -> Result<(), WorkspaceError> {
        if self.used {
            return Err(WorkspaceError::InvalidInput);
        }
        self.used = true;
        self.workspace.unlink_from(
            parent,
            name,
            deadline.min(self.deadline),
            MutationOrigin::ProjectionUnlink,
        )
    }
    /// Removes one empty directory name under the same parent-lock ownership.
    pub fn rmdir(
        &mut self,
        parent: u64,
        name: &[u8],
        deadline: Instant,
    ) -> Result<(), WorkspaceError> {
        if self.used {
            return Err(WorkspaceError::InvalidInput);
        }
        self.used = true;
        self.workspace.rmdir_from(
            parent,
            name,
            deadline.min(self.deadline),
            MutationOrigin::ProjectionRmdir,
        )
    }
    /// Publishes one atomic rename between two parents.
    pub fn rename(
        &mut self,
        source_parent: u64,
        source: &[u8],
        destination_parent: u64,
        destination: &[u8],
        flags: crate::RenameFlags,
        deadline: Instant,
    ) -> Result<(), WorkspaceError> {
        if self.used {
            return Err(WorkspaceError::InvalidInput);
        }
        self.used = true;
        self.workspace.rename_from(
            crate::filesystem::rename::RenameRequest {
                source_parent,
                source,
                destination_parent,
                destination,
                flags,
            },
            deadline.min(self.deadline),
            MutationOrigin::ProjectionRename,
        )
    }
    /// Publishes one checked portable-attribute change. The kernel owns cache
    /// invalidation after the reply, so this operation sends no notification.
    pub fn set_attributes(
        &mut self,
        serial: u64,
        request: PortableAttributes,
        deadline: Instant,
    ) -> Result<NodeAttributes, WorkspaceError> {
        if self.used {
            return Err(WorkspaceError::InvalidInput);
        }
        self.used = true;
        self.workspace.set_attributes_from(
            serial,
            request,
            deadline.min(self.deadline),
            MutationOrigin::ProjectionAttributes,
        )
    }
}
impl Drop for ProjectionMutationPermit {
    fn drop(&mut self) {
        if let Ok(mut state) = self.workspace.state() {
            if let Some(projection) = &mut state.projection {
                projection.replies = projection
                    .replies
                    .checked_sub(1)
                    .expect("one release per admitted projection mutation");
                projection.mutation_held = false;
            }
        }
    }
}

impl MountLease {
    /// Installs one invalidator before callback workers start. The callback's
    /// concrete capture and Arc header are charged while the binding owns it;
    /// separately owned projection/session resources remain the adapter's account.
    pub fn bind_invalidation(
        &mut self,
        delivery: ProjectionInvalidation,
    ) -> Result<(), WorkspaceError> {
        if self.finished {
            return Err(WorkspaceError::Closed);
        }
        let bytes = Layout::new::<[usize; 2]>()
            .extend(Layout::for_value(delivery.as_ref()))
            .map_err(|_| WorkspaceError::Capacity)?
            .0
            .pad_to_align()
            .size();
        let charge = self.workspace.host.budget.reserve(bytes)?;
        let mut state = self.workspace.state()?;
        self.workspace.available(&state)?;
        if !state.mounted {
            return Err(WorkspaceError::Closed);
        }
        let projection = state.projection.as_mut().ok_or(WorkspaceError::Io)?;
        if projection.status != CoherenceStatus::Unbound || projection.replies > 0 {
            return Err(WorkspaceError::Busy);
        }
        projection.delivery = Some(delivery);
        projection.callback_charge = Some(charge);
        projection.status = CoherenceStatus::Ready;
        Ok(())
    }
}

impl Workspace {
    /// Reserves the exclusive projected mutation and one of two reply slots.
    pub fn begin_projection_mutation(
        &self,
        deadline: Instant,
    ) -> Result<ProjectionMutationPermit, WorkspaceError> {
        if self.inner.access != WorkspaceAccess::LocalEdit {
            return Err(WorkspaceError::ReadOnly);
        }
        let deadline = Self::callback_deadline(deadline);
        let mut state = self.state()?;
        self.available(&state)?;
        if Instant::now() >= deadline {
            return Err(WorkspaceError::Deadline);
        }
        if !state.mounted {
            return Err(WorkspaceError::Closed);
        }
        let projection = state.projection.as_mut().ok_or(WorkspaceError::Io)?;
        if projection.status != CoherenceStatus::Ready
            || projection.delivery.is_none()
            || projection.mutation_held
            || projection.replies != 0
        {
            return Err(WorkspaceError::Busy);
        }
        projection.mutation_held = true;
        projection.replies = 1;
        Ok(ProjectionMutationPermit {
            workspace: self.clone(),
            deadline,
            used: false,
        })
    }

    /// Records one projection callback for this Workspace.
    ///
    /// The adapter calls this at its single entry point, so the counts describe
    /// what the kernel actually asked for. Recording never gates the callback.
    pub fn record_projection_call(&self, op: crate::filesystem::projection_counters::ProjectionOp) {
        if let Ok(mut state) = self.state() {
            state.counters.record(op);
        }
    }

    /// Records one data callback's request and transferred byte counts.
    ///
    /// This is the ordinary product telemetry that shows what the kernel asked
    /// the projection to move; recording never gates the callback.
    pub fn record_projection_bytes(
        &self,
        op: crate::filesystem::projection_counters::ProjectionOp,
        request: u64,
        returned: u64,
    ) {
        if let Ok(mut state) = self.state() {
            state.counters.record_bytes(op, request, returned);
        }
    }

    pub fn begin_projection_reply(
        &self,
        deadline: Instant,
    ) -> Result<ProjectionReplyPermit, WorkspaceError> {
        let mut state = self.state()?;
        self.available(&state)?;
        if Instant::now() >= deadline {
            return Err(WorkspaceError::Deadline);
        }
        if !state.mounted {
            return Err(WorkspaceError::Closed);
        }
        let projection = state.projection.as_mut().ok_or(WorkspaceError::Io)?;
        if projection.replies >= PROJECTION_REPLIES
            || (self.inner.access == WorkspaceAccess::LocalEdit
                && projection.status == CoherenceStatus::Unbound)
        {
            return Err(WorkspaceError::Busy);
        }
        // Pending/failed notification never prevents a new-view kernel read.
        projection.replies += 1;
        Ok(ProjectionReplyPermit {
            workspace: self.clone(),
        })
    }

    pub(crate) fn check_mutation_coherence(
        &self,
        state: &State,
        origin: MutationOrigin,
        publication: bool,
    ) -> Result<(), WorkspaceError> {
        if origin.projected() {
            self.check_projected_mutation(state, publication)
        } else {
            self.check_projection_mutation(state)
        }
    }

    pub(crate) fn check_projection_mutation(&self, state: &State) -> Result<(), WorkspaceError> {
        if !state.mounted {
            return Ok(());
        }
        let projection = state.projection.as_ref().ok_or(WorkspaceError::Io)?;
        if projection.status != CoherenceStatus::Ready
            || projection.delivery.is_none()
            || projection.mutation_held
            || projection.replies > 0
        {
            return Err(WorkspaceError::Busy);
        }
        Ok(())
    }

    pub(crate) fn check_projected_mutation(
        &self,
        state: &State,
        publication: bool,
    ) -> Result<(), WorkspaceError> {
        if !state.mounted {
            return Err(WorkspaceError::Closed);
        }
        let projection = state.projection.as_ref().ok_or(WorkspaceError::Io)?;
        if projection.status != CoherenceStatus::Ready
            || projection.delivery.is_none()
            || !projection.mutation_held
            || projection.replies == 0
            || projection.replies > PROJECTION_REPLIES
            || (publication && projection.replies != 1)
        {
            return Err(WorkspaceError::Busy);
        }
        Ok(())
    }

    pub(crate) fn complete_projection_mutation(
        &self,
        delivery: ProjectionInvalidation,
        receipt: MutationReceipt,
        entry: Option<(u64, &[u8])>,
        published_handle: Option<HandleId>,
        deadline: Instant,
    ) -> Result<(), WorkspaceError> {
        let result = if Instant::now() >= deadline {
            Err(io::ErrorKind::TimedOut.into())
        } else {
            delivery(receipt, entry, deadline)
        };
        let notifier_returned_ok = result.is_ok();
        let cause = result
            .err()
            .map(|error| (error.kind(), error.raw_os_error()));
        let failure = |cause: Option<(io::ErrorKind, Option<i32>)>| {
            let (kind, raw_os_error) = cause.unwrap_or((io::ErrorKind::Other, None));
            CoherenceFailure {
                receipt,
                published_handle,
                kind,
                raw_os_error,
                notifier_returned_ok,
            }
        };
        let mut state = self
            .state()
            .map_err(|_| WorkspaceError::Coherence(failure(cause)))?;
        let revision = state.revision;
        let projection = state
            .projection
            .as_mut()
            .ok_or_else(|| WorkspaceError::Coherence(failure(cause)))?;
        // One mutation can own more than one affected entry: a cross-directory
        // rename notifies both parents through one pending publication. Every
        // later entry of that same mutation finds the status its own earlier
        // entry left behind, which the unchanged revision still names.
        let mine = revision == receipt.revision
            && match &projection.status {
                CoherenceStatus::Ready => true,
                CoherenceStatus::Failed(previous) => previous.receipt == receipt,
                _ => false,
            };
        if projection.status
            != (CoherenceStatus::Pending {
                receipt,
                published_handle,
            })
            && !mine
        {
            return Err(WorkspaceError::Coherence(failure(cause)));
        }
        let cause = cause
            .or_else(|| (Instant::now() >= deadline).then_some((io::ErrorKind::TimedOut, None)));
        if cause.is_some() {
            let failed = failure(cause);
            projection.status = CoherenceStatus::Failed(failed);
            return Err(WorkspaceError::Coherence(failed));
        }
        projection.status = CoherenceStatus::Ready;
        Ok(())
    }
}
