//! Authorized canonical reads that preserve the deciding provider cause.
use super::{Authorization, RuntimeError, RuntimeResult};
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, ObjectId};
use layerfs_history::{BranchId, WorkspaceId};
use layerfs_storage::{StorageError, StorageResult};
use std::{cell::RefCell, sync::Arc};

pub(super) enum ReadFailure {
    /// Authority refusal does not itself make the Save producer terminal.
    Authority(RuntimeError),
    /// Actual same-Save read failure is terminal under the owning Save contract.
    Storage(Arc<StorageError>),
    Content(ContentError),
}
impl ReadFailure {
    pub(super) fn into_runtime(self) -> RuntimeError {
        match self {
            Self::Authority(error) => error,
            Self::Storage(error) => RuntimeError::Storage(error),
            Self::Content(error) => RuntimeError::Content(error),
        }
    }
}

/// One operation's existing Reader or Save read method, with no new provider.
pub(super) struct AuthorizedObjects<'a, F> {
    authority: &'a dyn Authorization,
    peer: [u8; 32],
    workspace: WorkspaceId,
    branch: BranchId,
    read: F,
    failure: RefCell<Option<ReadFailure>>,
}
impl<'a, F> AuthorizedObjects<'a, F>
where
    F: Fn(&[ObjectId]) -> StorageResult<Vec<Vec<u8>>>,
{
    pub(super) fn new(
        authority: &'a dyn Authorization,
        peer: [u8; 32],
        workspace: WorkspaceId,
        branch: BranchId,
        read: F,
    ) -> Self {
        Self {
            authority,
            peer,
            workspace,
            branch,
            read,
            failure: RefCell::new(None),
        }
    }
    pub(super) fn finish<T>(self, result: ContentResult<T>) -> Result<T, ReadFailure> {
        match self.failure.into_inner() {
            Some(error) => Err(error),
            None => result.map_err(ReadFailure::Content),
        }
    }
    pub(super) fn runtime<T>(self, result: ContentResult<T>) -> RuntimeResult<T> {
        self.finish(result).map_err(ReadFailure::into_runtime)
    }
}
impl<F> AuthenticatedObjects for AuthorizedObjects<'_, F>
where
    F: Fn(&[ObjectId]) -> StorageResult<Vec<Vec<u8>>>,
{
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        if self.failure.borrow().is_some() {
            return Err(ContentError::ProviderFailure {
                what: "authorized demand already failed",
            });
        }
        let authority = self
            .authority
            .workspace(self.peer, self.workspace, self.branch)
            .and_then(|()| {
                self.authority
                    .objects(self.peer, self.workspace, self.branch, ids)
            });
        let result = match authority {
            Err(error) => Err(ReadFailure::Authority(error)),
            Ok(()) => (self.read)(ids).map_err(|error| ReadFailure::Storage(Arc::new(error))),
        };
        match result {
            Ok(values) => Ok(values),
            Err(error) => {
                *self.failure.borrow_mut() = Some(error);
                Err(ContentError::ProviderFailure {
                    what: "authorized canonical demand",
                })
            }
        }
    }
}
