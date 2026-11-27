//! A forced teardown that stopped at its one plain detach because the kernel
//! refused it `EBUSY` (#538): a later unmount or force asks for that detach
//! again, since the process that held the mount may have exited. The abort
//! write is never repeated. A holder still present keeps the same custody,
//! with the facts of this attempt.
use super::super::{
    fuse::dispatcher::{Evidence, Kept, Leaving},
    kernel::Kernel,
    registry::bound_mut,
    Failure, Service, Success,
};
use layerfs_fuse::session::{Drained, SessionObserver, Undrained};
use layerfs_overlay::KernelMount;
use layerfs_protocol::daemon::{Activity, ForcedFacts, MountPhase, WorkspaceToken};
use layerfs_store_service::store::BoundWorkspace;
use std::sync::Arc;

/// A retained forced connection taken out of its entry for one more detach.
pub(super) struct Retry {
    workspace: Arc<BoundWorkspace>,
    undrained: Box<Undrained>,
    facts: ForcedFacts,
}
/// A forced connection whose one detach held: ready for Revoke and Close.
pub(super) struct Retried {
    pub workspace: Arc<BoundWorkspace>,
    pub mount: KernelMount,
    pub observer: SessionObserver,
    pub drained: Box<Drained>,
    pub facts: ForcedFacts,
}
impl Service {
    /// One short registry step: takes a retryable custody
    /// ([`Kept::retryable`]) out of its entry, which is `Stopping` and closing
    /// before the lock is released. `None` leaves any other entry unchanged.
    pub(super) fn take_retryable(&self, token: WorkspaceToken) -> Result<Option<Retry>, Failure> {
        let mut entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
        let value = bound_mut(&mut entries, token)?;
        if !matches!(&value.kernel, Kernel::Retained(kept) if kept.retryable()) {
            return Ok(None);
        }
        let Kernel::Retained(kept) = std::mem::replace(&mut value.kernel, Kernel::Attaching) else {
            unreachable!("checked retained custody")
        };
        let Kept {
            ready,
            observer,
            evidence,
            forced,
            ..
        } = *kept;
        let (Some(ready), Some(observer), Evidence::Forced(undrained), Some(facts)) =
            (ready, observer, evidence, forced)
        else {
            unreachable!("checked retryable custody")
        };
        // Leaving again, so that a detach still refused keeps its mount's
        // ready record and observer, as the first stop did.
        value.kernel = Kernel::Leaving(Box::new(Leaving {
            ready,
            observer,
            phase: MountPhase::Stopping,
        }));
        value.enter(Activity::Closing);
        value.epoch = value.epoch.saturating_add(1);
        Ok(Some(Retry {
            workspace: value.workspace.clone(),
            undrained,
            facts,
        }))
    }
    /// Asks once for the detach the kernel refused, after a fresh local
    /// drain. A refusal keeps the connection with this attempt's facts.
    pub(super) fn retry_detach(
        &self,
        token: WorkspaceToken,
        retry: Retry,
    ) -> Result<Retried, Failure> {
        let Retry {
            workspace,
            undrained,
            facts,
        } = retry;
        let observer = undrained.session.observer();
        let mount = undrained.session.mount();
        match undrained.retry_detach() {
            Ok(drained) => Ok(Retried {
                workspace,
                mount,
                observer,
                drained,
                facts,
            }),
            Err(undrained) => {
                Err(self.keep_forced(token, "detach retry", observer, undrained, facts))
            }
        }
    }
    /// A normal unmount's retry whose detach held: Revoke, then the logical
    /// Close, answered `Unmounted` like any normal unmount. The earlier
    /// force already dropped uncommitted changes, so no dirty check applies.
    pub(super) fn unmount_retried(
        &self,
        token: WorkspaceToken,
        retried: Retried,
    ) -> Result<Success, Failure> {
        let Retried {
            workspace,
            mount,
            observer,
            drained,
            mut facts,
        } = retried;
        facts.detach = layerfs_protocol::daemon::DetachDisposition::Detached;
        let drained = self.revoke(token, &workspace, mount, &observer, drained, &facts)?;
        self.close(token, &workspace, Some(drained))
    }
}
