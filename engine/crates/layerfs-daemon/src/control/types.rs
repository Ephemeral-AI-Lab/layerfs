//! Original control operation receipts, independent of delivery acknowledgement.
use crate::{
    store::{BindRefusal, CommitFailure, CommitSuccess},
    OwnerError,
};
use layerfs_bridge::control::{ControlCode, Reply};
use layerfs_history::HistoryError;
use layerfs_workspace::WorkspaceError;
/// Original kernel terminal receipt; it already owns the drained connection
/// facts. Keep their concrete type instead of erasing counted work behind Debug.
#[cfg(target_os = "linux")]
pub type KernelEvidence = layerfs_fuse::session::Drained;
/// Kernel sessions cannot be constructed on other platforms.
#[cfg(not(target_os = "linux"))]
#[derive(Debug)]
pub enum KernelEvidence {}
/// Original known control result with every attempted engine/Commit completion.
#[derive(Debug)]
pub struct Success {
    pub reply: Reply,
    pub commit: Option<CommitSuccess>,
    pub observation_failure: Option<Box<Failure>>,
    /// Original kernel connection receipt of a terminal unmount.
    pub kernel: Option<Box<KernelEvidence>>,
}
/// A kernel attach or unmount refusal, or a Mount refused while maintenance
/// is stopped, with its original evidence.
#[derive(Debug)]
pub struct KernelFailure {
    pub code: ControlCode,
    /// Stable phase such as `attach:mount` or `unmount:kernel`.
    pub phase: &'static str,
    pub detail: String,
    /// Original connection evidence, when the failing boundary produced one.
    pub evidence: Option<Box<dyn std::fmt::Debug + Send>>,
}
/// Original deciding failure; publication/unknown custody stays in its typed carrier.
#[derive(Debug)]
pub enum Failure {
    Rejected(ControlCode, &'static str),
    Custody(Box<layerfs_bridge::control::ControlRefusal>),
    Bind(Box<BindRefusal>),
    Commit(Box<CommitFailure>),
    Owner(OwnerError),
    History(HistoryError),
    Workspace(WorkspaceError),
    Kernel(Box<KernelFailure>),
    /// A terminal operation stopped after effects. The registry entry keeps
    /// the exact owners; this is the bounded summary sent to the caller.
    Retained(Box<layerfs_bridge::control::TeardownCustody>),
    Poisoned,
    After {
        cause: Box<Failure>,
        original: Box<Success>,
    },
}
impl Success {
    pub(crate) fn reply(reply: Reply) -> Self {
        Self {
            reply,
            commit: None,
            observation_failure: None,
            kernel: None,
        }
    }
}
impl Failure {
    #[cfg(target_os = "linux")]
    pub(super) fn kernel(
        code: ControlCode,
        phase: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self::Kernel(Box::new(KernelFailure {
            code,
            phase,
            detail: detail.into(),
            evidence: None,
        }))
    }
}
