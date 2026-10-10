//! Original control operation receipts, independent of delivery acknowledgement.
use crate::{
    store::{BindRefusal, CommitFailure, CommitSuccess},
    Completion, OwnerError,
};
use layerfs_bridge::control::{ControlCode, Reply};
use layerfs_history::HistoryError;
use layerfs_workspace::WorkspaceError;
/// Original native terminal receipt; it already owns the drained connection
/// facts. Keep their concrete type instead of erasing counted work behind Debug.
#[cfg(target_os = "linux")]
pub type NativeEvidence = layerfs_fuse::session::Drained;
/// Native sessions cannot be constructed on other platforms.
#[cfg(not(target_os = "linux"))]
#[derive(Debug)]
pub enum NativeEvidence {}
/// Original known control result with every attempted engine/Commit completion.
#[derive(Debug)]
pub struct Success {
    pub reply: Reply,
    pub completion: Option<Completion>,
    /// Earlier engine completions of the same operation, in attempt order.
    pub earlier: Vec<Completion>,
    pub commit: Option<CommitSuccess>,
    pub observation_failure: Option<Box<Failure>>,
    /// Original native connection receipt of a terminal unmount.
    pub native: Option<Box<NativeEvidence>>,
}
/// A native attach or unmount refusal, or a Mount refused while maintenance
/// is stopped, with its original evidence.
#[derive(Debug)]
pub struct NativeFailure {
    pub code: ControlCode,
    /// Stable phase such as `attach:mount` or `unmount:kernel`.
    pub phase: &'static str,
    pub detail: String,
    /// Engine completions this operation attempted, in order.
    pub completions: Vec<Completion>,
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
    Completion(Box<Completion>),
    History(HistoryError),
    Workspace(WorkspaceError),
    Native(Box<NativeFailure>),
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
            completion: None,
            earlier: Vec::new(),
            commit: None,
            observation_failure: None,
            native: None,
        }
    }
}
impl Failure {
    #[cfg(target_os = "linux")]
    pub(super) fn native(
        code: ControlCode,
        phase: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self::Native(Box::new(NativeFailure {
            code,
            phase,
            detail: detail.into(),
            completions: Vec::new(),
            evidence: None,
        }))
    }
}
