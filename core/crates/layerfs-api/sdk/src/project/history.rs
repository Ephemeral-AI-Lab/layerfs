//! Project history facade over caller-owned authenticated filesystem control.
use super::ProjectApi;
use crate::{control::Control, operation::exchange, OperationFailure};
use layerfs_bridge::control::{Reply, Request};
use layerfs_history::{
    BranchSnapshot, CommitHistoryRequest, CommitRecord, ForkRequest, PageResult,
};

impl ProjectApi {
    /// Forks once through the daemon's direct History owner, preserving a refusal.
    pub fn fork(
        &self,
        control: &mut Control,
        request: ForkRequest,
    ) -> Result<BranchSnapshot, Box<OperationFailure>> {
        let request = Request::Fork(request);
        match exchange(control, request.clone())? {
            Reply::Forked(binding) => Ok(binding),
            reply => Err(OperationFailure::unexpected(request, reply)),
        }
    }
    /// Reads one anchored bounded ancestry page. No Branch refresh or hidden scan.
    pub fn history(
        &self,
        control: &mut Control,
        request: CommitHistoryRequest,
    ) -> Result<PageResult<CommitRecord>, Box<OperationFailure>> {
        let request = Request::History(request);
        match exchange(control, request.clone())? {
            Reply::History(page) => Ok(page),
            reply => Err(OperationFailure::unexpected(request, reply)),
        }
    }
}
