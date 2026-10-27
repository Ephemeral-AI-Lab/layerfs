//! Typed bounded record calls under the Workspace lock.
use crate::{OriginalInput, OwnerClient, OwnerError, ServiceClass};
use layerfs_overlay::{
    IndexedOperationRecordChange, IndexedOperationRecordKey, IndexedOperationRecordScope, Overlay,
};
use layerfs_workspace::{
    OperationRecordApply, OperationRecordReply, OperationRecordWindow, OverlayOperationRecords,
    WorkspaceError, WorkspaceResult,
};

impl OwnerClient {
    fn record<T, I: Send + 'static>(
        &self,
        input: I,
        action: impl FnOnce(&Overlay, I) -> WorkspaceResult<T> + Send + 'static,
    ) -> WorkspaceResult<T> {
        let route = match self.bound_route() {
            Ok(route) => route,
            Err(cause) => {
                return Err(WorkspaceError::Service(Box::new(OwnerError::Unattempted {
                    cause: Box::new(cause),
                    input: OriginalInput::new(input),
                })))
            }
        };
        self.call_observed(Some(route), ServiceClass::General, None, move |db, _| {
            action(db, input).map_err(|cause| OwnerError::Workspace(Box::new(cause)))
        })
        .map(|(value, _)| value)
        .map_err(|cause| WorkspaceError::Service(Box::new(cause)))
    }
}
impl OverlayOperationRecords for OwnerClient {
    fn operation_record_contains(
        &self,
        scope: IndexedOperationRecordScope,
        key: IndexedOperationRecordKey,
    ) -> WorkspaceResult<OperationRecordReply<bool>> {
        self.record((scope, key), |db, (scope, key)| {
            db.indexed_operation_record_contains(scope, key)
                .map_err(Into::into)
        })
        .map(OperationRecordReply::owned)
    }
    fn operation_record_get(
        &self,
        scope: IndexedOperationRecordScope,
        key: IndexedOperationRecordKey,
    ) -> WorkspaceResult<OperationRecordReply<Option<Vec<u8>>>> {
        self.record((scope, key), |db, (scope, key)| {
            db.indexed_operation_record_get(scope, key)
                .map_err(Into::into)
        })
        .map(OperationRecordReply::owned)
    }
    fn operation_record_get_many(
        &self,
        scope: IndexedOperationRecordScope,
        keys: &[IndexedOperationRecordKey],
    ) -> WorkspaceResult<OperationRecordReply<Vec<Option<Vec<u8>>>>> {
        self.record((scope, keys.to_vec()), |db, (scope, keys)| {
            db.indexed_operation_record_get_many(scope, &keys)
                .map_err(Into::into)
        })
        .map(OperationRecordReply::owned)
    }
    fn operation_record_apply(
        &self,
        scope: IndexedOperationRecordScope,
        changes: Vec<IndexedOperationRecordChange>,
    ) -> WorkspaceResult<OperationRecordReply<OperationRecordApply>> {
        // The Workspace port owns the original guarded refusal and counts the
        // one deciding-value copy. The call transfers that typed result intact.
        self.record((scope, changes), |db, (scope, changes)| {
            OverlayOperationRecords::operation_record_apply(db, scope, changes)
        })
    }
    fn operation_record_keys(
        &self,
        scope: IndexedOperationRecordScope,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> WorkspaceResult<OperationRecordReply<Vec<[u8; 32]>>> {
        self.record(
            (scope, kind, excluded),
            |db, (scope, kind, excluded)| match excluded {
                Some(root) => db
                    .indexed_operation_record_first_keys(scope, kind, root)
                    .map_err(Into::into),
                None => db
                    .indexed_operation_record_keys(scope, kind, None)
                    .map_err(Into::into),
            },
        )
        .map(OperationRecordReply::owned)
    }
    fn operation_record_keys_after(
        &self,
        scope: IndexedOperationRecordScope,
        kind: u32,
        after: Option<[u8; 32]>,
    ) -> WorkspaceResult<OperationRecordReply<Vec<[u8; 32]>>> {
        self.record((scope, kind, after), |db, (scope, kind, after)| {
            db.indexed_operation_record_keys_after(scope, kind, after)
                .map_err(Into::into)
        })
        .map(OperationRecordReply::owned)
    }
    fn operation_record_entries_after(
        &self,
        scope: IndexedOperationRecordScope,
        kind: u32,
        after: Option<[u8; 32]>,
    ) -> WorkspaceResult<OperationRecordReply<OperationRecordWindow>> {
        self.record((scope, kind, after), |db, (scope, kind, after)| {
            db.indexed_operation_record_entries_after(scope, kind, after)
                .map_err(Into::into)
        })
        .map(OperationRecordReply::owned)
    }
}
