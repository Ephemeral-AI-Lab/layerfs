//! Original control operation receipts, independent of delivery acknowledgement.
use crate::{
    store::{BindRefusal, CommitFailure, CommitSuccess},
    Completion, OwnerError,
};
use layerfs_bridge::control::{ControlCode, Reply};
use layerfs_history::HistoryError;
use layerfs_workspace::WorkspaceError;
/// Original known control result with every attempted engine/Commit completion.
#[derive(Debug)]
pub struct Success {
    pub reply: Reply,
    pub completion: Option<Completion>,
    pub commit: Option<CommitSuccess>,
    pub observation_failure: Option<Box<Failure>>,
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
    Poisoned,
    After {
        cause: Box<Failure>,
        original: Box<Success>,
    },
}
impl Success {
    pub(super) fn reply(reply: Reply) -> Self {
        Self {
            reply,
            completion: None,
            commit: None,
            observation_failure: None,
        }
    }
}
