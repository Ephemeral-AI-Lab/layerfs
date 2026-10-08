//! Workspace raw-record port over original short credited OperationRecord jobs.
//! Constructor-thread port: each job waits for a credit before its one attempt.
use crate::{
    Command, Completion, IndexedOperationRecordJob, IndexedOperationRecordReply, OwnerClient,
    OwnerError, Response,
};
use layerfs_overlay::{
    IndexedOperationRecordApply, IndexedOperationRecordChange, IndexedOperationRecordKey,
    IndexedOperationRecordScope,
};
use layerfs_workspace::{
    OperationRecordApply, OperationRecordCopies, OperationRecordReply, OverlayOperationRecords,
    WorkspaceError, WorkspaceResult,
};

fn original_job(
    client: &OwnerClient,
    scope: IndexedOperationRecordScope,
    job: IndexedOperationRecordJob,
) -> WorkspaceResult<Completion> {
    let command = Command::IndexedOperationRecord(Box::new(job));
    let pending = client
        .submit_waiting(Some(scope.owner.route()), command)
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
fn copies<T>(value: &Vec<T>) -> OperationRecordCopies {
    OperationRecordCopies {
        bytes: (value.len() * std::mem::size_of::<T>()) as u64,
        allocations: u64::from(!value.is_empty()),
        retained_bytes: value.capacity() * std::mem::size_of::<T>(),
    }
}
impl OverlayOperationRecords for OwnerClient {
    fn operation_record_contains(
        &self,
        scope: IndexedOperationRecordScope,
        key: IndexedOperationRecordKey,
    ) -> WorkspaceResult<OperationRecordReply<bool>> {
        let done = original_job(
            self,
            scope,
            IndexedOperationRecordJob::Contains { scope, key },
        )?;
        match done.result() {
            Ok(Response::IndexedOperationRecord(IndexedOperationRecordReply::Contains(value))) => {
                Ok(OperationRecordReply::owned(*value))
            }
            _ => Err(WorkspaceError::Service(Box::new(done))),
        }
    }
    fn operation_record_get(
        &self,
        scope: IndexedOperationRecordScope,
        key: IndexedOperationRecordKey,
    ) -> WorkspaceResult<OperationRecordReply<Option<Vec<u8>>>> {
        let done = original_job(self, scope, IndexedOperationRecordJob::Get { scope, key })?;
        match done.result() {
            Ok(Response::IndexedOperationRecord(IndexedOperationRecordReply::Value(value))) => {
                let value = value.clone();
                let copies = value
                    .as_ref()
                    .map_or(OperationRecordCopies::default(), copies);
                Ok(OperationRecordReply { value, copies })
            }
            _ => Err(WorkspaceError::Service(Box::new(done))),
        }
    }
    fn operation_record_apply(
        &self,
        scope: IndexedOperationRecordScope,
        changes: Vec<IndexedOperationRecordChange>,
    ) -> WorkspaceResult<OperationRecordReply<OperationRecordApply>> {
        let done = original_job(
            self,
            scope,
            IndexedOperationRecordJob::Apply { scope, changes },
        )?;
        match done.result() {
            Ok(Response::IndexedOperationRecord(IndexedOperationRecordReply::Applied(
                IndexedOperationRecordApply::Applied,
            ))) => Ok(OperationRecordReply::owned(OperationRecordApply::Applied)),
            Ok(Response::IndexedOperationRecord(IndexedOperationRecordReply::Applied(
                IndexedOperationRecordApply::NotApplied { index, key, actual },
            ))) => {
                let index = *index;
                let key = *key;
                let actual = actual.clone();
                let copies = actual
                    .as_ref()
                    .map_or(OperationRecordCopies::default(), copies);
                Ok(OperationRecordReply {
                    value: OperationRecordApply::NotApplied {
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
    fn operation_record_keys(
        &self,
        scope: IndexedOperationRecordScope,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> WorkspaceResult<OperationRecordReply<Vec<[u8; 32]>>> {
        let job = match excluded {
            Some(excluded_root) => IndexedOperationRecordJob::FirstKeys {
                scope,
                kind,
                excluded_root,
            },
            None => IndexedOperationRecordJob::FirstKeysAll { scope, kind },
        };
        let done = original_job(self, scope, job)?;
        match done.result() {
            Ok(Response::IndexedOperationRecord(IndexedOperationRecordReply::Keys(value))) => {
                let value = value.clone();
                let copies = copies(&value);
                Ok(OperationRecordReply { value, copies })
            }
            _ => Err(WorkspaceError::Service(Box::new(done))),
        }
    }
    fn operation_record_keys_after(
        &self,
        scope: IndexedOperationRecordScope,
        kind: u32,
        after: Option<[u8; 32]>,
    ) -> WorkspaceResult<OperationRecordReply<Vec<[u8; 32]>>> {
        let done = original_job(
            self,
            scope,
            IndexedOperationRecordJob::KeysAfter { scope, kind, after },
        )?;
        match done.result() {
            Ok(Response::IndexedOperationRecord(IndexedOperationRecordReply::Keys(value))) => {
                let value = value.clone();
                let copies = copies(&value);
                Ok(OperationRecordReply { value, copies })
            }
            _ => Err(WorkspaceError::Service(Box::new(done))),
        }
    }
}
