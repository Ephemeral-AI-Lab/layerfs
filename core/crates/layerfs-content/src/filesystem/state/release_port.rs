//! Narrow paged descendant-release pending/job/cursor custody.
use super::{BaseFact, CanonicalScope, ReleaseFrame, ReleaseJob, ReleaseSeal, ZeroSeal};
use crate::{ContentResult, ObjectId};
/// Exact FIFO jobs and LIFO frames through known/Unknown acknowledgements.
pub trait ReleaseState {
    /// Admit deferred owner and bind the exact independently checked zero snapshot.
    fn release_begin(&mut self, scope: &CanonicalScope, seeds: &ZeroSeal) -> ContentResult<()>;
    /// Ordered complete seed windows, preserving already known base answers.
    fn release_seed(&mut self, scope: &CanonicalScope, records: &[BaseFact]) -> ContentResult<()>;
    /// Verify complete seed count/key/digest EOF before traversal may start.
    fn release_close_seeds(&mut self, scope: &CanonicalScope) -> ContentResult<()>;
    /// Take the oldest pending rank once; the provider retains its exact current job.
    fn release_take(&mut self, scope: &CanonicalScope) -> ContentResult<Option<ReleaseJob>>;
    /// Complete that exact current job, pushing a directory frame or none.
    fn release_complete_job(
        &mut self,
        scope: &CanonicalScope,
        job: &ReleaseJob,
        directory: Option<ObjectId>,
    ) -> ContentResult<()>;
    /// Highest cursor only after current/pending jobs are empty.
    fn release_frame(&mut self, scope: &CanonicalScope) -> ContentResult<Option<ReleaseFrame>>;
    /// Exact before/proposed continuation and <=128 ordered page children, before SQL.
    fn release_advance(
        &mut self,
        scope: &CanonicalScope,
        before: &ReleaseFrame,
        after: &ReleaseFrame,
        children: &[BaseFact],
    ) -> ContentResult<ReleaseFrame>;
    /// Pop one exact finished highest frame, never all retained frames.
    fn release_pop(&mut self, scope: &CanonicalScope, frame: &ReleaseFrame) -> ContentResult<()>;
    /// Known exact current/pending/frame EOF and complete counters.
    fn release_seal(&mut self, scope: &CanonicalScope) -> ContentResult<ReleaseSeal>;
    /// Exact known retirement; no guessed native cleanup or capacity refund.
    fn release_retire(&mut self, seal: &ReleaseSeal) -> ContentResult<()>;
    /// Selected metadata-only failure terminalization.
    fn release_abandon(&mut self, scope: &CanonicalScope) -> ContentResult<()>;
}
