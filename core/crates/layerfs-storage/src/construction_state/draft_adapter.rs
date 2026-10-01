//! Dedicated supplied C1 draft authority; preserves original typed native failure.
use super::{
    draft_create, draft_emission, draft_index, draft_links, draft_retire, draft_temporary,
    ScratchSession,
};
use crate::error::{StorageError, StorageResult};
use layerfs_content::file::edit::{
    DraftCapacity, DraftJob, DraftRecord, DraftScope, DraftState, DraftStats,
};
use layerfs_content::{ContentError, ContentResult, ObjectId};

/// One borrowed profile6 owner, with no resident/native fallback.
pub struct DraftAdapter<'a> {
    session: &'a mut ScratchSession,
    scope: DraftScope,
}
impl<'a> DraftAdapter<'a> {
    /// Capture the exact live bound native scope before C1 producer effects.
    pub fn new(session: &'a mut ScratchSession) -> StorageResult<Self> {
        let scope = session.draft_scope()?;
        Ok(Self { session, scope })
    }
    fn failure(&self, error: StorageError) -> ContentError {
        match error {
            StorageError::Content(
                error @ ContentError::InvalidOrderingRecord("draft foreign scope"),
            ) => error,
            error => self.session.keep_failure(error),
        }
    }
}
impl DraftState for DraftAdapter<'_> {
    fn capacity(&self) -> ContentResult<DraftCapacity> {
        let scope = self
            .session
            .draft_scope()
            .map_err(|error| self.failure(error))?;
        if scope != self.scope {
            return Err(ContentError::InvalidOrderingRecord("draft foreign scope"));
        }
        Ok(scope.capacity())
    }
    fn hold(&mut self, id: ObjectId, record: DraftRecord) -> ContentResult<()> {
        self.session
            .draft_run(&self.scope, |resource| {
                draft_create::hold(resource, &self.scope, id, record)
            })
            .map_err(|error| self.failure(error))
    }
    fn get(&mut self, id: ObjectId) -> ContentResult<Option<DraftRecord>> {
        self.session
            .draft_run(&self.scope, |resource| {
                draft_index::get(resource.connection.as_ref().unwrap(), &self.scope, id)
            })
            .map_err(|error| self.failure(error))
    }
    fn select_root(&mut self, before: Option<ObjectId>, after: ObjectId) -> ContentResult<()> {
        self.session
            .draft_run(&self.scope, |resource| {
                draft_links::select(resource, before, Some(after))
            })
            .map_err(|error| self.failure(error))
    }
    fn next_job(&mut self) -> ContentResult<Option<DraftJob>> {
        self.session
            .draft_run(&self.scope, |resource| {
                draft_links::next(resource.connection.as_ref().unwrap())
            })
            .map_err(|error| self.failure(error))
    }
    fn retain_temporaries(&mut self, ids: &[ObjectId]) -> ContentResult<()> {
        self.session
            .draft_run(&self.scope, |resource| {
                draft_temporary::change(resource, ids, true)
            })
            .map_err(|error| self.failure(error))
    }
    fn release_temporaries(&mut self, ids: &[ObjectId]) -> ContentResult<()> {
        self.session
            .draft_run(&self.scope, |resource| {
                draft_temporary::change(resource, ids, false)
            })
            .map_err(|error| self.failure(error))
    }
    fn supersede(&mut self, id: ObjectId, expected: Option<ObjectId>) -> ContentResult<()> {
        self.session
            .draft_run(&self.scope, |resource| {
                draft_temporary::supersede(resource, &self.scope, id, expected)
            })
            .map_err(|error| self.failure(error))
    }
    fn retire_job(&mut self, job: DraftJob) -> ContentResult<()> {
        self.session
            .draft_run(&self.scope, |resource| {
                draft_retire::retire(resource, &self.scope, job)
            })
            .map_err(|error| self.failure(error))
    }
    fn resolved(&mut self, id: ObjectId) -> ContentResult<Option<ObjectId>> {
        self.session
            .draft_run(&self.scope, |resource| {
                draft_index::resolved(resource.connection.as_ref().unwrap(), id)
            })
            .map_err(|error| self.failure(error))
    }
    fn begin_emission(&mut self, draft: ObjectId, canonical: ObjectId) -> ContentResult<bool> {
        self.session
            .draft_run(&self.scope, |resource| {
                draft_emission::begin(resource, draft, canonical)
            })
            .map_err(|error| self.failure(error))
    }
    fn accepted(&mut self, draft: ObjectId, canonical: ObjectId) -> ContentResult<()> {
        self.session
            .draft_run(&self.scope, |resource| {
                draft_emission::accepted(resource, draft, canonical)
            })
            .map_err(|error| self.failure(error))
    }
    fn finish(&mut self) -> ContentResult<()> {
        self.session
            .draft_run(&self.scope, |resource| {
                draft_retire::finish(resource, &self.scope)
            })
            .map_err(|error| self.failure(error))
    }
    fn abandon(&mut self) {
        self.session.draft_abandon();
    }
    fn stats(&self) -> DraftStats {
        self.session.draft_stats()
    }
}
