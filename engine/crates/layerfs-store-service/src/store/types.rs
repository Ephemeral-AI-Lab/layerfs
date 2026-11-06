//! Exact bind intent and original owner completion custody.
use super::{BoundWorkspace, PortError};
use crate::OwnerError;
use layerfs_content::ContentError;
use layerfs_history::{BranchId, BranchSnapshot, HistoryError, WorkspaceId};
use std::{fmt, sync::Arc};

#[derive(Clone, Debug)]
pub struct BindRequest {
    pub branch: BranchId,
    pub workspace: WorkspaceId,
}
/// A bind of the Branch a project and Branch name select (#64): the daemon
/// finds it on the Workspace's own reader, so its caller reads no Store.
#[derive(Clone, Debug)]
pub struct NamedBindRequest {
    pub workspace: WorkspaceId,
    /// The project's name (its LayerStack).
    pub project: String,
    /// The Branch's name in that project.
    pub branch: String,
}
/// What a bind selected: a Branch by identity or by names.
#[derive(Clone, Debug)]
pub enum BindIntent {
    Id(BindRequest),
    Named(NamedBindRequest),
}
impl BindIntent {
    pub fn workspace(&self) -> WorkspaceId {
        match self {
            Self::Id(request) => request.workspace,
            Self::Named(request) => request.workspace,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindPhase {
    Snapshot,
    Root,
    Open,
}
#[derive(Debug)]
pub enum BindError {
    Cleanup {
        original: Box<BindError>,
        cause: Arc<PortError>,
    },
    MissingBranch(BranchId),
    /// A named bind's project does not exist (its name as given).
    NoSuchProject(String),
    /// A named bind's project has no such Branch (its name as given).
    NoSuchBranch(String),
    History(HistoryError),
    Read(Arc<PortError>),
    Content {
        error: ContentError,
        provider: Option<Arc<PortError>>,
    },
    Owner(OwnerError),
}
#[derive(Debug)]
pub struct BindRefusal {
    pub phase: BindPhase,
    pub request: BindIntent,
    pub snapshot: Option<BranchSnapshot>,
    pub error: BindError,
}
#[derive(Debug)]
pub enum CloseError {
    Owner(OwnerError),
    Reader(Arc<PortError>),
}
impl fmt::Display for CloseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Workspace Close: {self:?}")
    }
}
impl std::error::Error for CloseError {}
pub struct BindSuccess {
    pub workspace: BoundWorkspace,
}
impl fmt::Display for BindRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Store bind: {self:?}")
    }
}
impl std::error::Error for BindRefusal {}
