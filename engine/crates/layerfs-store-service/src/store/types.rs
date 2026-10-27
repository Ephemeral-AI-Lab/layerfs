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
    pub request: BindRequest,
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
