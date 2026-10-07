//! Bounded raw-record service; original result custody crosses this boundary.
use crate::{WorkspaceError, WorkspaceResult};
use layerfs_overlay::{
    indexed_operation_record_changes_bytes, IndexedOperationRecordApply,
    IndexedOperationRecordChange, IndexedOperationRecordKey, IndexedOperationRecordScope, Overlay,
    OverlayError, OPERATION_RECORD_BYTES,
};
use std::fmt;

/// Copies made when returning an independent service reply. SQL column
/// acquisition, Content encoding and other layers are outside this observation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OperationRecordCopies {
    pub bytes: u64,
    pub allocations: u64,
    pub retained_bytes: usize,
}
/// A value with its actual service-return copy observation.
#[derive(Debug)]
pub struct OperationRecordReply<T> {
    pub value: T,
    pub copies: OperationRecordCopies,
}
impl<T> OperationRecordReply<T> {
    pub fn owned(value: T) -> Self {
        Self {
            value,
            copies: OperationRecordCopies::default(),
        }
    }
}
/// One original guarded-application result. A refusal retains the actual
/// provider result separately from the bounded deciding body delivered to C1.
#[derive(Debug)]
pub enum OperationRecordApply {
    Applied,
    NotApplied {
        index: usize,
        key: IndexedOperationRecordKey,
        actual: Option<Vec<u8>>,
        original: WorkspaceError,
    },
}
/// Original direct-Overlay refusal; Daemon instead retains its Completion.
#[derive(Debug)]
pub struct OperationRecordRefusal(pub IndexedOperationRecordApply);
impl fmt::Display for OperationRecordRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for OperationRecordRefusal {}

/// Original direct-port input when its owning descriptor capacity refuses
/// admission. The borrowed Overlay API retains its separate caller-owned scope.
#[derive(Debug)]
pub struct OperationRecordInputRefusal {
    pub cause: OverlayError,
    pub retained_bytes: Option<usize>,
    pub changes: Vec<IndexedOperationRecordChange>,
}
impl fmt::Display for OperationRecordInputRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "operation_record input capacity refusal: {:?}, {} changes",
            self.retained_bytes,
            self.changes.len()
        )
    }
}
impl std::error::Error for OperationRecordInputRefusal {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

/// Independently usable short owner jobs. No Content/provider IO or operation
/// acquire/release is performed here. Each result is one original attempt.
pub trait OverlayOperationRecords {
    fn operation_record_contains(
        &self,
        scope: IndexedOperationRecordScope,
        key: IndexedOperationRecordKey,
    ) -> WorkspaceResult<OperationRecordReply<bool>>;
    fn operation_record_get(
        &self,
        scope: IndexedOperationRecordScope,
        key: IndexedOperationRecordKey,
    ) -> WorkspaceResult<OperationRecordReply<Option<Vec<u8>>>>;
    fn operation_record_apply(
        &self,
        scope: IndexedOperationRecordScope,
        changes: Vec<IndexedOperationRecordChange>,
    ) -> WorkspaceResult<OperationRecordReply<OperationRecordApply>>;
    fn operation_record_keys(
        &self,
        scope: IndexedOperationRecordScope,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> WorkspaceResult<OperationRecordReply<Vec<[u8; 32]>>>;
    /// Non-destructive complete-key cursor, returning at most 64 sorted keys.
    /// None precedes all identities; Some excludes keys <= that full identity.
    /// Older providers refuse this separate capability explicitly.
    fn operation_record_keys_after(
        &self,
        _scope: IndexedOperationRecordScope,
        _kind: u32,
        _after: Option<[u8; 32]>,
    ) -> WorkspaceResult<OperationRecordReply<Vec<[u8; 32]>>> {
        Err(layerfs_content::ContentError::ProviderFailure {
            what: "indexed construction key enumeration unavailable",
        }
        .into())
    }
}

impl OverlayOperationRecords for Overlay {
    fn operation_record_contains(
        &self,
        scope: IndexedOperationRecordScope,
        key: IndexedOperationRecordKey,
    ) -> WorkspaceResult<OperationRecordReply<bool>> {
        self.indexed_operation_record_contains(scope, key)
            .map(OperationRecordReply::owned)
            .map_err(Into::into)
    }
    fn operation_record_get(
        &self,
        scope: IndexedOperationRecordScope,
        key: IndexedOperationRecordKey,
    ) -> WorkspaceResult<OperationRecordReply<Option<Vec<u8>>>> {
        self.indexed_operation_record_get(scope, key)
            .map(OperationRecordReply::owned)
            .map_err(Into::into)
    }
    fn operation_record_apply(
        &self,
        scope: IndexedOperationRecordScope,
        changes: Vec<IndexedOperationRecordChange>,
    ) -> WorkspaceResult<OperationRecordReply<OperationRecordApply>> {
        let used = match indexed_operation_record_changes_bytes(&changes) {
            Ok(bytes) => bytes,
            Err(cause) => {
                return Err(WorkspaceError::Service(Box::new(
                    OperationRecordInputRefusal {
                        cause,
                        retained_bytes: None,
                        changes,
                    },
                )))
            }
        };
        let bytes = used.checked_add(
            (changes.capacity() - changes.len())
                * std::mem::size_of::<IndexedOperationRecordChange>(),
        );
        if bytes.is_none_or(|bytes| bytes > OPERATION_RECORD_BYTES) {
            return Err(WorkspaceError::Service(Box::new(
                OperationRecordInputRefusal {
                    cause: OverlayError::Invalid("owned operation record input capacity"),
                    retained_bytes: bytes,
                    changes,
                },
            )));
        }
        let result = self.indexed_operation_record_apply(scope, &changes)?;
        match result {
            IndexedOperationRecordApply::Applied => {
                Ok(OperationRecordReply::owned(OperationRecordApply::Applied))
            }
            IndexedOperationRecordApply::NotApplied {
                index,
                key,
                ref actual,
            } => {
                let copied = actual.clone();
                let copies = OperationRecordCopies {
                    bytes: copied.as_ref().map_or(0, |v| v.len() as u64),
                    allocations: u64::from(copied.as_ref().is_some_and(|v| !v.is_empty())),
                    retained_bytes: copied.as_ref().map_or(0, Vec::capacity),
                };
                Ok(OperationRecordReply {
                    value: OperationRecordApply::NotApplied {
                        index,
                        key,
                        actual: copied,
                        original: WorkspaceError::Service(Box::new(OperationRecordRefusal(result))),
                    },
                    copies,
                })
            }
        }
    }
    fn operation_record_keys(
        &self,
        scope: IndexedOperationRecordScope,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> WorkspaceResult<OperationRecordReply<Vec<[u8; 32]>>> {
        self.indexed_operation_record_keys(scope, kind, excluded)
            .map(OperationRecordReply::owned)
            .map_err(Into::into)
    }
    fn operation_record_keys_after(
        &self,
        scope: IndexedOperationRecordScope,
        kind: u32,
        after: Option<[u8; 32]>,
    ) -> WorkspaceResult<OperationRecordReply<Vec<[u8; 32]>>> {
        self.indexed_operation_record_keys_after(scope, kind, after)
            .map(OperationRecordReply::owned)
            .map_err(Into::into)
    }
}
