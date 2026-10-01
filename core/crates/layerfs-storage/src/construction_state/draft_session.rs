//! Selected draft lifetime and one bounded provider operation at a time.
use super::{session::Resource, ScratchSession};
use crate::error::{StorageError, StorageResult};
use layerfs_content::file::edit::{DraftScope, DraftStats};
use layerfs_content::ContentError;
impl ScratchSession {
    /// Exact bound profile6 scope; never constructs/adopts a scope from raw tokens.
    pub fn draft_scope(&self) -> StorageResult<DraftScope> {
        let resource = self.resource.as_ref().ok_or(StorageError::Integrity(
            "draft selected profile unavailable",
        ))?;
        let drafts = resource.draft.as_ref().ok_or(StorageError::Integrity(
            "draft selected profile unavailable",
        ))?;
        if resource.native.reserved_bytes != drafts.scope.capacity().scratch_bytes()
            || resource.release_attempted
            || resource.unknown.get()
            || resource.native.quarantined
            || drafts.failed.get()
            || drafts.ledger.ended
            || drafts.attempt.is_some()
        {
            return Err(StorageError::Integrity("draft selected profile ended"));
        }
        resource.check_live()?;
        resource.verify()?;
        Ok(drafts.scope.clone())
    }
    pub(crate) fn draft_run<T>(
        &mut self,
        scope: &DraftScope,
        operation: impl FnOnce(&mut Resource) -> StorageResult<T>,
    ) -> StorageResult<T> {
        if self
            .resource
            .as_ref()
            .and_then(|resource| resource.draft.as_ref())
            .is_none_or(|drafts| &drafts.scope != scope)
        {
            return Err(StorageError::Content(ContentError::InvalidOrderingRecord(
                "draft foreign scope",
            )));
        }
        let result = (|| {
            let resource = self.resource.as_mut().unwrap();
            let drafts = resource.draft.as_ref().unwrap();
            if resource.native.reserved_bytes != drafts.scope.capacity().scratch_bytes()
                || resource.release_attempted
                || resource.unknown.get()
                || resource.native.quarantined
                || drafts.failed.get()
                || drafts.ledger.ended
                || drafts.attempt.is_some()
            {
                return Err(StorageError::Integrity("draft owner unavailable"));
            }
            resource.check_live()?;
            resource.verify()?;
            operation(resource)
        })();
        self.finish(result)
    }
    pub(crate) fn draft_stats(&self) -> DraftStats {
        self.resource
            .as_ref()
            .unwrap()
            .draft
            .as_ref()
            .unwrap()
            .stats
    }
    pub(crate) fn draft_abandon(&mut self) {
        self.resource
            .as_ref()
            .unwrap()
            .draft
            .as_ref()
            .unwrap()
            .failed
            .set(true);
    }
}
