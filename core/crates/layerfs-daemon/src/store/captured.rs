//! The product Commit constructor: one captured namespace attempt inside the
//! unchanged driver. Its reader and operation owner are released only after
//! the driver's outcome is known; unresolved custody stays with the failure.
use super::{
    BoundWorkspace, CapturedConstruction, CommitError, CommitFailure, CommitSuccess, PortError,
    ReleasedOwner, StoreOperation,
};
use crate::{Command, Completion, OwnerError, Response};
use layerfs_content::{
    filesystem::FilesystemRootId, ContentError, ContentResult, FilesystemResult,
};
use layerfs_overlay::Capture;
use layerfs_storage::Save;
use layerfs_workspace::{CapturedNamespace, CapturedNamespaceAttempt, WorkspaceError};
use std::{cell::RefCell, sync::Arc};

impl BoundWorkspace {
    /// Commits the shared published frontier: every locally published change,
    /// whichever process made it. One attempt; this thread runs the
    /// synchronous captured ports, so it is never a native receive or
    /// request-service worker.
    pub fn commit_captured(&self) -> Result<CommitSuccess, Box<CommitFailure>> {
        // This thread keeps the capture's completion while it waits for
        // ordinary credits, and may keep one failed acquisition's while it
        // waits for a Lifecycle credit. With fewer than two slots of either
        // kind that wait could never end, so nothing is attempted.
        let slots = self.owner.configuration();
        if slots.jobs_per_namespace < 2 || slots.lifecycle_jobs_per_namespace < 2 {
            let mut refused = CommitFailure::new(
                CommitError::Context("owner job slots below the Commit minimum"),
                self.identity,
                self.route(),
            );
            refused.locally_settled = true;
            return Err(Box::new(refused));
        }
        let held = RefCell::new(None);
        let result = self.commit(|save, capture, _| {
            let mut slot = held.borrow_mut();
            self.construct_captured(save, capture, slot.insert(CapturedConstruction::default()))
        });
        let mut held = held.into_inner();
        match result {
            Ok(mut success) => {
                if let Some(held) = &mut held {
                    self.release_captured(held);
                }
                success.namespace = held;
                Ok(success)
            }
            Err(mut failure) => {
                // Only a definite, locally resolved nonpublication ends the
                // attempt's ownership. Anything else keeps every owner.
                if let (true, Some(held)) = (failure.locally_settled, &mut held) {
                    // The resolution is known done; its completion would keep
                    // a Lifecycle credit that the releases wait for.
                    failure.local = None;
                    self.release_captured(held);
                }
                failure.namespace = held;
                Err(failure)
            }
        }
    }
    /// The closure body. Acquired owners go to `held` before any later step
    /// can fail, and nothing here releases them.
    fn construct_captured(
        &self,
        save: &Save<'_>,
        capture: Capture,
        held: &mut CapturedConstruction,
    ) -> Result<FilesystemRootId, CommitError> {
        let operation = self.operation().map_err(CommitError::Workspace)?;
        // Only Commit acquires these owners, one capture at a time, so the
        // capture's own generation is unique among their live requests.
        let request = u64::try_from(capture.generation.number())
            .ok()
            .filter(|number| *number != 0)
            .ok_or(CommitError::Context("capture generation as request"))?;
        let reader = self.owned(
            Command::AcquireCapturedReader { capture, request },
            |response| match response {
                Response::CapturedReader(Some(reader)) => Some(*reader),
                _ => None,
            },
        )?;
        held.reader = Some(reader);
        let owner = self.owned(
            Command::AcquireOperation { request },
            |response| match response {
                Response::Operation(Some(owner)) => Some(*owner),
                _ => None,
            },
        )?;
        held.operation = Some(owner);
        let policy = self.store.policy().construction();
        let mut sink = save.sink();
        let CapturedNamespaceAttempt {
            result,
            mut custody,
        } = CapturedNamespace::new(operation.workspace(), operation.overlay(), reader, owner)
            .construct_untimed(policy, &policy.capacities(), save, &mut sink);
        held.work = Some(custody.work);
        // Exactly one slot holds the first original cause: the namespace's or
        // reader's, else the failing file's own or its record scope's, else
        // the namespace record scope's.
        let original = custody
            .failure
            .take()
            .or_else(|| {
                custody
                    .file
                    .as_mut()
                    .and_then(|file| file.failure.take().or_else(|| file.records.failure.take()))
            })
            .or_else(|| custody.records.failure.take());
        match classify(result, original, &operation) {
            Ok(built) => {
                held.counters = Some(built.counters);
                Ok(built.root)
            }
            Err(error) => {
                held.custody = Some(Box::new(custody));
                Err(error)
            }
        }
    }
    /// One owner job whose expected reply carries the acquired value.
    fn owned<T>(
        &self,
        command: Command,
        take: impl FnOnce(&Response) -> Option<T>,
    ) -> Result<T, CommitError> {
        let done = self.job(command).map_err(CommitError::Owner)?;
        match done.result().as_ref().ok().and_then(take) {
            Some(value) => Ok(value),
            None => Err(CommitError::Completion(Box::new(done))),
        }
    }
    /// The attempt's custody is dropped first, then the reader is released,
    /// then the operation owner. A release that is refused or fails stops the
    /// sequence; what it did not release stays named in the receipt. A done
    /// release's completion is dropped at once: holding it would keep a
    /// Lifecycle credit of this Workspace.
    fn release_captured(&self, held: &mut CapturedConstruction) {
        held.custody = None;
        if let Some(reader) = held.reader {
            if !self.released(Command::ReleaseCapturedReader(reader), held) {
                return;
            }
            held.reader = None;
            held.released.push(ReleasedOwner::Reader);
        }
        if let Some(owner) = held.operation {
            if self.released(Command::ReleaseOperation(owner), held) {
                held.operation = None;
                held.released.push(ReleasedOwner::Operation);
            }
        }
    }
    fn released(&self, command: Command, held: &mut CapturedConstruction) -> bool {
        match self.job(command) {
            Ok(done) if matches!(done.result(), Ok(Response::Done)) => true,
            Ok(done) => {
                held.release_failure = Some(Box::new(done));
                false
            }
            Err(error) => {
                held.release_error = Some(error);
                false
            }
        }
    }
}

/// The producer's Content result on failure is only a label. The driver must
/// classify the first original cause, so it gets that cause as what it was:
/// an owner job's completion, the refusal that never admitted it, the Store
/// provider's exact failure, or the Content or Workspace error itself.
fn classify(
    result: ContentResult<FilesystemResult>,
    original: Option<WorkspaceError>,
    operation: &StoreOperation,
) -> Result<FilesystemResult, CommitError> {
    // A base read that failed left its exact cause with the Store ports.
    let provider = || {
        operation
            .ports()
            .failure()
            .unwrap_or_else(|error| Some(Arc::new(error)))
    };
    let label = match (result, &original) {
        (Ok(built), None) => return Ok(built),
        (Ok(_), Some(_)) => ContentError::InvalidRecord("captured namespace result with a cause"),
        (Err(label), _) => label,
    };
    Err(match original {
        Some(WorkspaceError::Service(service)) => match service.downcast::<Completion>() {
            Ok(completion) => CommitError::Completion(completion),
            Err(other) => match other.downcast::<OwnerError>() {
                Ok(owner) => CommitError::Owner(*owner),
                Err(other) => match other.downcast::<Arc<PortError>>() {
                    Ok(port) => CommitError::Content {
                        error: label,
                        provider: Some(*port),
                    },
                    Err(other) => CommitError::Workspace(WorkspaceError::Service(other)),
                },
            },
        },
        Some(WorkspaceError::Content(error)) => CommitError::Content {
            error,
            provider: provider(),
        },
        Some(original) => CommitError::Workspace(original),
        None => CommitError::Content {
            error: label,
            provider: provider(),
        },
    })
}
