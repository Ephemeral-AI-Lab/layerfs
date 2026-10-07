//! Exact bind intent and original owner completion custody.
use super::{BoundWorkspace, PortError};
use crate::{Completion, OwnerError};
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
    MissingBranch(BranchId),
    History(HistoryError),
    Content {
        error: ContentError,
        provider: Option<Arc<PortError>>,
    },
    Owner(OwnerError),
    Completion(Box<Completion>),
}
#[derive(Debug)]
pub struct BindRefusal {
    pub phase: BindPhase,
    pub request: BindRequest,
    pub snapshot: Option<BranchSnapshot>,
    pub error: BindError,
}
pub struct BindSuccess {
    pub workspace: BoundWorkspace,
    /// Original Open receipt and credit. Dropping it does not unmount.
    pub open: Completion,
}
impl fmt::Display for BindRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Store bind: {self:?}")
    }
}
impl std::error::Error for BindRefusal {}
