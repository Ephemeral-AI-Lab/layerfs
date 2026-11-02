//! Logical terminal control through the existing daemon lifetime owner.
use super::WorkspaceApi;
use crate::OperationFailure;
use layerfs_bridge::control::{
    DirtyReport, ForcedOutcome, Reply, Request, UnmountControl, WorkspaceToken,
};

/// The answer to a normal unmount.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Unmounted {
    /// Closed.
    Closed,
    /// Refused for uncommitted changes; nothing was closed.
    Dirty(Box<DirtyReport>),
}

impl WorkspaceApi<'_> {
    /// Requests one explicit normal terminal unmount that discards any
    /// uncommitted changes. The daemon probes the
    /// kernel reversibly: Busy leaves the Workspace Ready and usable. Success is
    /// acknowledged only after detach, every loop join, daemon-work drain,
    /// kernel-owner revocation and logical Close. Retained custody is returned
    /// as its own typed cause; this facade adds no teardown shortcut.
    pub fn unmount(
        &mut self,
        token: WorkspaceToken,
        force: bool,
    ) -> Result<Option<ForcedOutcome>, Box<OperationFailure>> {
        if force {
            return self.force_unmount(token, false).map(Some);
        }
        match self.unmount_checked(token, true)? {
            Unmounted::Closed => Ok(None),
            Unmounted::Dirty(_) => unreachable!("a discarding unmount is never Dirty"),
        }
    }
    /// One normal terminal unmount that refuses uncommitted changes, with
    /// their paths, unless `discard`.
    pub fn unmount_checked(
        &mut self,
        token: WorkspaceToken,
        discard: bool,
    ) -> Result<Unmounted, Box<OperationFailure>> {
        let request = Request::Unmount(UnmountControl { token, discard });
        match self.exchange(request.clone())? {
            Reply::Unmounted(_) => Ok(Unmounted::Closed),
            Reply::Dirty(report) if !discard => Ok(Unmounted::Dirty(report)),
            reply => Err(OperationFailure::unexpected(request, reply)),
        }
    }
    /// Requests one explicit forced terminal unmount. `relinquish_unknown` is
    /// required and has no default: it alone permits the daemon to close over
    /// stopped unknown Commit custody, which the outcome still reports as
    /// unknown. Success returns the daemon's abort, detach, Commit-knowledge
    /// and cleanup dispositions. A refusal before any effect is a remote
    /// cause; retained custody, with its forced facts, is its own typed cause.
    /// This facade signals no process and never replays the request.
    pub(crate) fn force_unmount(
        &mut self,
        token: WorkspaceToken,
        relinquish_unknown: bool,
    ) -> Result<ForcedOutcome, Box<OperationFailure>> {
        let request = Request::ForceUnmount {
            token,
            relinquish_unknown,
        };
        match self.exchange(request.clone())? {
            Reply::ForceUnmounted(closed) => Ok(closed.outcome),
            reply => Err(OperationFailure::unexpected(request, reply)),
        }
    }
}
