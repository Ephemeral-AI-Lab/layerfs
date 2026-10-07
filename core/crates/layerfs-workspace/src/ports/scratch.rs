//! Bounded raw-record service; original result custody crosses this boundary.
use crate::{WorkspaceError, WorkspaceResult};
use layerfs_overlay::{
    indexed_changes_bytes, IndexedApply, IndexedChange, IndexedKey, IndexedScope, Overlay,
    OverlayError, SCRATCH_BYTES,
};
use std::fmt;

/// Copies made when returning an independent service reply. SQL column
/// acquisition, Content encoding and other layers are outside this observation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScratchCopies {
    pub bytes: u64,
    pub allocations: u64,
    pub retained_bytes: usize,
}
/// A value with its actual service-return copy observation.
#[derive(Debug)]
pub struct ScratchReply<T> {
    pub value: T,
    pub copies: ScratchCopies,
}
impl<T> ScratchReply<T> {
    pub fn owned(value: T) -> Self {
        Self {
            value,
            copies: ScratchCopies::default(),
        }
    }
}
/// One original guarded-application result. A refusal retains the actual
/// provider result separately from the bounded deciding body delivered to C1.
#[derive(Debug)]
pub enum ScratchApply {
    Applied,
    NotApplied {
        index: usize,
        key: IndexedKey,
        actual: Option<Vec<u8>>,
        original: WorkspaceError,
    },
}
/// Original direct-Overlay refusal; Daemon instead retains its Completion.
#[derive(Debug)]
pub struct ScratchRefusal(pub IndexedApply);
impl fmt::Display for ScratchRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ScratchRefusal {}

/// Original direct-port input when its owning descriptor capacity refuses
/// admission. The borrowed Overlay API retains its separate caller-owned scope.
#[derive(Debug)]
pub struct ScratchInputRefusal {
    pub cause: OverlayError,
    pub retained_bytes: Option<usize>,
    pub changes: Vec<IndexedChange>,
}
impl fmt::Display for ScratchInputRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "scratch input capacity refusal: {:?}, {} changes",
            self.retained_bytes,
            self.changes.len()
        )
    }
}
impl std::error::Error for ScratchInputRefusal {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

/// Independently usable short owner jobs. No Content/provider IO or operation
/// acquire/release is performed here. Each result is one original attempt.
pub trait OverlayScratch {
    fn scratch_contains(
        &self,
        scope: IndexedScope,
        key: IndexedKey,
    ) -> WorkspaceResult<ScratchReply<bool>>;
    fn scratch_get(
        &self,
        scope: IndexedScope,
        key: IndexedKey,
    ) -> WorkspaceResult<ScratchReply<Option<Vec<u8>>>>;
    fn scratch_apply(
        &self,
        scope: IndexedScope,
        changes: Vec<IndexedChange>,
    ) -> WorkspaceResult<ScratchReply<ScratchApply>>;
    fn scratch_keys(
        &self,
        scope: IndexedScope,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> WorkspaceResult<ScratchReply<Vec<[u8; 32]>>>;
    /// Non-destructive complete-key cursor, returning at most 64 sorted keys.
    /// None precedes all identities; Some excludes keys <= that full identity.
    /// Older providers refuse this separate capability explicitly.
    fn scratch_keys_after(
        &self,
        _scope: IndexedScope,
        _kind: u32,
        _after: Option<[u8; 32]>,
    ) -> WorkspaceResult<ScratchReply<Vec<[u8; 32]>>> {
        Err(layerfs_content::ContentError::ProviderFailure {
            what: "indexed construction key enumeration unavailable",
        }
        .into())
    }
}

impl OverlayScratch for Overlay {
    fn scratch_contains(
        &self,
        scope: IndexedScope,
        key: IndexedKey,
    ) -> WorkspaceResult<ScratchReply<bool>> {
        self.indexed_scratch_contains(scope, key)
            .map(ScratchReply::owned)
            .map_err(Into::into)
    }
    fn scratch_get(
        &self,
        scope: IndexedScope,
        key: IndexedKey,
    ) -> WorkspaceResult<ScratchReply<Option<Vec<u8>>>> {
        self.indexed_scratch_get(scope, key)
            .map(ScratchReply::owned)
            .map_err(Into::into)
    }
    fn scratch_apply(
        &self,
        scope: IndexedScope,
        changes: Vec<IndexedChange>,
    ) -> WorkspaceResult<ScratchReply<ScratchApply>> {
        let used = match indexed_changes_bytes(&changes) {
            Ok(bytes) => bytes,
            Err(cause) => {
                return Err(WorkspaceError::Service(Box::new(ScratchInputRefusal {
                    cause,
                    retained_bytes: None,
                    changes,
                })))
            }
        };
        let bytes = used.checked_add(
            (changes.capacity() - changes.len()) * std::mem::size_of::<IndexedChange>(),
        );
        if bytes.is_none_or(|bytes| bytes > SCRATCH_BYTES) {
            return Err(WorkspaceError::Service(Box::new(ScratchInputRefusal {
                cause: OverlayError::Invalid("owned scratch input capacity"),
                retained_bytes: bytes,
                changes,
            })));
        }
        let result = self.indexed_scratch_apply(scope, &changes)?;
        match result {
            IndexedApply::Applied => Ok(ScratchReply::owned(ScratchApply::Applied)),
            IndexedApply::NotApplied {
                index,
                key,
                ref actual,
            } => {
                let copied = actual.clone();
                let copies = ScratchCopies {
                    bytes: copied.as_ref().map_or(0, |v| v.len() as u64),
                    allocations: u64::from(copied.as_ref().is_some_and(|v| !v.is_empty())),
                    retained_bytes: copied.as_ref().map_or(0, Vec::capacity),
                };
                Ok(ScratchReply {
                    value: ScratchApply::NotApplied {
                        index,
                        key,
                        actual: copied,
                        original: WorkspaceError::Service(Box::new(ScratchRefusal(result))),
                    },
                    copies,
                })
            }
        }
    }
    fn scratch_keys(
        &self,
        scope: IndexedScope,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> WorkspaceResult<ScratchReply<Vec<[u8; 32]>>> {
        self.indexed_scratch_keys(scope, kind, excluded)
            .map(ScratchReply::owned)
            .map_err(Into::into)
    }
    fn scratch_keys_after(
        &self,
        scope: IndexedScope,
        kind: u32,
        after: Option<[u8; 32]>,
    ) -> WorkspaceResult<ScratchReply<Vec<[u8; 32]>>> {
        self.indexed_scratch_keys_after(scope, kind, after)
            .map(ScratchReply::owned)
            .map_err(Into::into)
    }
}
