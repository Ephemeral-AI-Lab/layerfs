//! Typed release delegation preserves foreign custody and original storage failures.
use super::ScratchAdapter;
use crate::StorageError;
use layerfs_content::filesystem::state::{
    BaseFact, CanonicalScope, ReleaseFrame, ReleaseJob, ReleaseSeal, ReleaseState, ZeroSeal,
};
use layerfs_content::{ContentError, ContentResult, ObjectId};
impl ScratchAdapter<'_> {
    fn release_failure(&self, e: StorageError) -> ContentError {
        match e {
            StorageError::Content(
                error @ ContentError::InvalidOrderingRecord("release foreign scope"),
            ) => error,
            error => self.session.keep_failure(error),
        }
    }
}
impl ReleaseState for ScratchAdapter<'_> {
    fn release_begin(&mut self, scope: &CanonicalScope, seeds: &ZeroSeal) -> ContentResult<()> {
        self.session
            .release_begin(scope, seeds)
            .map_err(|e| self.release_failure(e))
    }
    fn release_seed(&mut self, scope: &CanonicalScope, records: &[BaseFact]) -> ContentResult<()> {
        self.session
            .release_seed(scope, records)
            .map_err(|e| self.release_failure(e))
    }
    fn release_close_seeds(&mut self, scope: &CanonicalScope) -> ContentResult<()> {
        self.session
            .release_close_seeds(scope)
            .map_err(|e| self.release_failure(e))
    }
    fn release_take(&mut self, scope: &CanonicalScope) -> ContentResult<Option<ReleaseJob>> {
        self.session
            .release_take(scope)
            .map_err(|e| self.release_failure(e))
    }
    fn release_complete_job(
        &mut self,
        scope: &CanonicalScope,
        job: &ReleaseJob,
        directory: Option<ObjectId>,
    ) -> ContentResult<()> {
        self.session
            .release_complete_job(scope, job, directory)
            .map_err(|e| self.release_failure(e))
    }
    fn release_frame(&mut self, scope: &CanonicalScope) -> ContentResult<Option<ReleaseFrame>> {
        self.session
            .release_frame(scope)
            .map_err(|e| self.release_failure(e))
    }
    fn release_advance(
        &mut self,
        scope: &CanonicalScope,
        before: &ReleaseFrame,
        after: &ReleaseFrame,
        children: &[BaseFact],
    ) -> ContentResult<ReleaseFrame> {
        self.session
            .release_advance(scope, before, after, children)
            .map_err(|e| self.release_failure(e))
    }
    fn release_pop(&mut self, scope: &CanonicalScope, frame: &ReleaseFrame) -> ContentResult<()> {
        self.session
            .release_pop(scope, frame)
            .map_err(|e| self.release_failure(e))
    }
    fn release_seal(&mut self, scope: &CanonicalScope) -> ContentResult<ReleaseSeal> {
        self.session
            .release_seal(scope)
            .map_err(|e| self.release_failure(e))
    }
    fn release_retire(&mut self, seal: &ReleaseSeal) -> ContentResult<()> {
        self.session
            .release_retire(seal)
            .map_err(|e| self.release_failure(e))
    }
    fn release_abandon(&mut self, scope: &CanonicalScope) -> ContentResult<()> {
        self.session
            .release_abandon(scope)
            .map_err(|e| self.release_failure(e))
    }
}
