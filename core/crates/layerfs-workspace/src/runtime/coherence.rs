//! One projection binding, bounded reply exclusion, and checked mutation completion.
use super::{lifecycle::MountLease, state::State};
use crate::{backing::budget::Charge, *};
use std::{alloc::Layout, io, mem::size_of, time::Instant};

const PROJECTION_REPLIES: usize = 2;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MutationOrigin {
    Local,
    Projection { append: bool },
}
impl MutationOrigin {
    pub fn projected(self) -> bool {
        matches!(self, Self::Projection { .. })
    }
    pub fn append(self, stored: bool) -> bool {
        match self {
            Self::Local => stored,
            Self::Projection { append } => append,
        }
    }
}

pub(crate) struct ProjectionState {
    pub delivery: Option<ProjectionInvalidation>,
    pub replies: usize,
    pub write_held: bool,
    pub status: CoherenceStatus,
    callback_charge: Option<Charge>,
    _charge: Charge,
}
impl ProjectionState {
    pub fn reserve(workspace: &Workspace) -> Result<Box<Self>, WorkspaceError> {
        let bytes = size_of::<Self>()
            + (PROJECTION_REPLIES - 1) * size_of::<ProjectionReplyPermit>()
            + size_of::<ProjectionWritePermit>()
            + 64;
        let charge = workspace.host.budget.reserve(bytes)?;
        Ok(Box::new(Self {
            delivery: None,
            replies: 0,
            write_held: false,
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

/// Owns one projected write attempt and its reply boundary. Keep this permit
/// until the projection has attempted to send the write reply, including errors.
pub struct ProjectionWritePermit {
    workspace: Workspace,
    deadline: Instant,
    used: bool,
}
impl ProjectionWritePermit {
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
            MutationOrigin::Projection { append },
        )
    }
}
impl Drop for ProjectionWritePermit {
    fn drop(&mut self) {
        if let Ok(mut state) = self.workspace.state() {
            if let Some(projection) = &mut state.projection {
                projection.replies = projection
                    .replies
                    .checked_sub(1)
                    .expect("one release per admitted projection write");
                projection.write_held = false;
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
    /// Reserves the exclusive projected write and one of two reply slots.
    pub fn begin_projection_write(
        &self,
        deadline: Instant,
    ) -> Result<ProjectionWritePermit, WorkspaceError> {
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
            || projection.write_held
            || projection.replies != 0
        {
            return Err(WorkspaceError::Busy);
        }
        projection.write_held = true;
        projection.replies = 1;
        Ok(ProjectionWritePermit {
            workspace: self.clone(),
            deadline,
            used: false,
        })
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

    pub(crate) fn check_projection_mutation(&self, state: &State) -> Result<(), WorkspaceError> {
        if !state.mounted {
            return Ok(());
        }
        let projection = state.projection.as_ref().ok_or(WorkspaceError::Io)?;
        if projection.status != CoherenceStatus::Ready
            || projection.delivery.is_none()
            || projection.write_held
            || projection.replies > 0
        {
            return Err(WorkspaceError::Busy);
        }
        Ok(())
    }

    pub(crate) fn check_projected_write(
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
            || !projection.write_held
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
        published_handle: Option<HandleId>,
        deadline: Instant,
    ) -> Result<(), WorkspaceError> {
        let result = if Instant::now() >= deadline {
            Err(io::ErrorKind::TimedOut.into())
        } else {
            delivery(receipt, deadline)
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
        let projection = state
            .projection
            .as_mut()
            .ok_or_else(|| WorkspaceError::Coherence(failure(cause)))?;
        if projection.status
            != (CoherenceStatus::Pending {
                receipt,
                published_handle,
            })
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
