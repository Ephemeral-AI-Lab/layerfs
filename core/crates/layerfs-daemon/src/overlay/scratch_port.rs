//! Workspace raw-record port over original short credited Scratch jobs.
use crate::{
    Command, Completion, IndexedScratchJob, IndexedScratchReply, OwnerClient, OwnerError, Response,
};
use layerfs_overlay::{IndexedApply, IndexedChange, IndexedKey, IndexedScope};
use layerfs_workspace::{
    OverlayScratch, ScratchApply, ScratchCopies, ScratchReply, WorkspaceError, WorkspaceResult,
};

fn original_job(
    client: &OwnerClient,
    scope: IndexedScope,
    job: IndexedScratchJob,
) -> WorkspaceResult<Completion> {
    let command = Command::IndexedScratch(Box::new(job));
    let pending = client
        .try_submit(Some(scope.owner.route()), command)
        .map_err(|(cause, command)| {
            WorkspaceError::Service(Box::new(OwnerError::Unattempted {
                cause: Box::new(cause),
                command: Box::new(command),
            }))
        })?;
    pending
        .wait()
        .map_err(|cause| WorkspaceError::Service(Box::new(cause)))
}
fn copies<T>(value: &Vec<T>) -> ScratchCopies {
    ScratchCopies {
        bytes: (value.len() * std::mem::size_of::<T>()) as u64,
        allocations: u64::from(!value.is_empty()),
        retained_bytes: value.capacity() * std::mem::size_of::<T>(),
    }
}
impl OverlayScratch for OwnerClient {
    fn scratch_contains(
        &self,
        scope: IndexedScope,
        key: IndexedKey,
    ) -> WorkspaceResult<ScratchReply<bool>> {
        let done = original_job(self, scope, IndexedScratchJob::Contains { scope, key })?;
        match done.result() {
            Ok(Response::IndexedScratch(IndexedScratchReply::Contains(value))) => {
                Ok(ScratchReply::owned(*value))
            }
            _ => Err(WorkspaceError::Service(Box::new(done))),
        }
    }
    fn scratch_get(
        &self,
        scope: IndexedScope,
        key: IndexedKey,
    ) -> WorkspaceResult<ScratchReply<Option<Vec<u8>>>> {
        let done = original_job(self, scope, IndexedScratchJob::Get { scope, key })?;
        match done.result() {
            Ok(Response::IndexedScratch(IndexedScratchReply::Value(value))) => {
                let value = value.clone();
                let copies = value.as_ref().map_or(ScratchCopies::default(), copies);
                Ok(ScratchReply { value, copies })
            }
            _ => Err(WorkspaceError::Service(Box::new(done))),
        }
    }
    fn scratch_apply(
        &self,
        scope: IndexedScope,
        changes: Vec<IndexedChange>,
    ) -> WorkspaceResult<ScratchReply<ScratchApply>> {
        let done = original_job(self, scope, IndexedScratchJob::Apply { scope, changes })?;
        match done.result() {
            Ok(Response::IndexedScratch(IndexedScratchReply::Applied(IndexedApply::Applied))) => {
                Ok(ScratchReply::owned(ScratchApply::Applied))
            }
            Ok(Response::IndexedScratch(IndexedScratchReply::Applied(
                IndexedApply::NotApplied { index, key, actual },
            ))) => {
                let index = *index;
                let key = *key;
                let actual = actual.clone();
                let copies = actual.as_ref().map_or(ScratchCopies::default(), copies);
                Ok(ScratchReply {
                    value: ScratchApply::NotApplied {
                        index,
                        key,
                        actual,
                        original: WorkspaceError::Service(Box::new(done)),
                    },
                    copies,
                })
            }
            _ => Err(WorkspaceError::Service(Box::new(done))),
        }
    }
    fn scratch_keys(
        &self,
        scope: IndexedScope,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> WorkspaceResult<ScratchReply<Vec<[u8; 32]>>> {
        let job = match excluded {
            Some(excluded_root) => IndexedScratchJob::FirstKeys {
                scope,
                kind,
                excluded_root,
            },
            None => IndexedScratchJob::FirstKeysAll { scope, kind },
        };
        let done = original_job(self, scope, job)?;
        match done.result() {
            Ok(Response::IndexedScratch(IndexedScratchReply::Keys(value))) => {
                let value = value.clone();
                let copies = copies(&value);
                Ok(ScratchReply { value, copies })
            }
            _ => Err(WorkspaceError::Service(Box::new(done))),
        }
    }
}
