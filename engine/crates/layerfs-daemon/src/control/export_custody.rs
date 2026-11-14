//! The NFS route's kernel-half custody: a serving export, one a terminal
//! operation is tearing down, and one kept exactly as a terminal or attach
//! step left it.
//!
//! [`export`](super::export) owns the mount entry and the `Service` steps; the
//! types here are what an entry holds while an export lives, so status,
//! teardown and Force name one ownership model instead of two.
use super::kernel::bounded;
use layerfs_bridge::control::{
    ForcedFacts, MountPhase, MountWork, ReadyMount, TeardownCustody, TeardownStage, WorkspaceToken,
};
use layerfs_nfs::Server;
use layerfs_overlay::KernelMount;
use std::{fmt, sync::Arc};

/// A serving export and the mount the host client holds of it.
pub(super) struct Exported {
    pub server: Arc<Server>,
    pub mount: KernelMount,
    pub ready: ReadyMount,
    pub tracked: bool,
}
/// A terminal operation owns the export; status still observes it.
pub(super) struct ExportLeaving {
    pub ready: ReadyMount,
    pub phase: MountPhase,
    pub work: MountWork,
}
/// Export custody a terminal or attach step could not finish disposing:
/// kept exactly, never retried or settled by a later observation.
pub(super) struct ExportKept {
    pub ready: Option<ReadyMount>,
    pub stage: TeardownStage,
    pub detail: String,
    pub work: Option<MountWork>,
    pub exported: Option<Box<Exported>>,
    pub evidence: Box<dyn fmt::Debug + Send>,
    pub forced: Option<ForcedFacts>,
}
impl ExportKept {
    /// Whether one forced client unmount may still finish this custody: the
    /// export and its receipt are held, the first boundary not established is
    /// `Detach`, and nothing was forced yet.
    ///
    /// Custody a failed Attach left (`ready: None`, its inner export
    /// `tracked: false`, because the mount helper was interrupted) is never
    /// forceable: no receipt was published, so there is no mount to force off
    /// by name, and a failed force would leave the same custody. It stays
    /// retained until the daemon exits; the next start's `reclaim` forces the
    /// leftover host mount off. This predates #644.
    pub(super) fn forceable(&self) -> bool {
        self.exported.is_some()
            && self.stage == TeardownStage::Detach
            && self.forced.is_none()
            && self.ready.is_some()
    }
    pub(super) fn custody(&self, token: WorkspaceToken) -> TeardownCustody {
        TeardownCustody {
            token,
            stage: self.stage,
            detached: matches!(self.stage, TeardownStage::Revoke | TeardownStage::Close),
            work: self.work,
            detail: bounded(&self.detail),
            forced: self.forced.clone(),
        }
    }
}
impl fmt::Debug for ExportKept {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let evidence: &dyn fmt::Debug = self
            .exported
            .as_ref()
            .map_or(&*self.evidence as &dyn fmt::Debug, |e| e);
        f.debug_struct("ExportKept")
            .field("ready", &self.ready)
            .field("stage", &self.stage)
            .field("detail", &self.detail)
            .field("work", &self.work)
            .field("evidence", &evidence)
            .field("forced", &self.forced)
            .finish()
    }
}
impl fmt::Debug for Exported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Exported")
            .field("address", &self.server.local_addr())
            .field("mount", &self.mount)
            .field("held", &self.server.held())
            .finish()
    }
}
