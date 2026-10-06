//! Caller-retained responses keep credits and exact registry receipt owners.
use super::{credits::Credit, Request, Response};
use crate::runtime::{Binding, RuntimeResult};

/// Exact adapter invocation or a fenced request which was never invoked.
#[derive(Debug)]
pub enum ServiceOutcome {
    /// Original response/error. Receipt capabilities do not imply publication success.
    Dispatched(RuntimeResult<Response>),
    /// Original owned request cancelled before any adapter call.
    Unattempted(Request),
}
/// Owned original result; last release returns admission and Save-receipt credit.
pub struct ServiceCompletion {
    pub(super) binding: Binding,
    pub(super) outcome: ServiceOutcome,
    pub(super) _credit: Credit,
    pub(super) queue_wait_ns: u64,
    pub(super) service_ns: u64,
}
impl ServiceCompletion {
    /// Original typed outcome, without repeating its operation.
    pub fn outcome(&self) -> &ServiceOutcome {
        &self.outcome
    }
    /// Admission-to-invocation wall; diagnostic, not a latency gate.
    pub const fn queue_wait_ns(&self) -> u64 {
        self.queue_wait_ns
    }
    /// Original invocation wall; overlaps the underlying provider work.
    pub const fn service_ns(&self) -> u64 {
        self.service_ns
    }
}
