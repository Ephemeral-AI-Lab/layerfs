//! The product Commit constructor: one captured namespace attempt inside the
//! unchanged driver. Its reader and operation owner are released only after
//! the driver's outcome is known; unresolved custody stays with the failure.
use super::{
    BoundWorkspace, CapturedConstruction, CommitError, CommitFailure, CommitOptions, CommitSuccess,
    PortError, ReleasedOwner, StoreOperation,
};
use crate::OwnerError;
use layerfs_content::{
    filesystem::FilesystemRootId, ContentError, ContentResult, FilesystemResult, FinalizedConsumer,
    FinalizedObject,
};
use layerfs_overlay::Capture;
use layerfs_storage::{Save, SaveSink};
use layerfs_workspace::{
    CapturedNamespace, CapturedNamespaceAttempt, ConstructedObjects, WorkspaceError,
};
use std::{cell::RefCell, sync::Arc};

/// The Save's sink, and beside it the namespace objects this attempt
/// constructed. They stay with the attempt: nothing is placed in the shared
/// cache here, and a refused object ends construction as it did.
struct Staging<'s, 'a, 'c> {
    sink: SaveSink<'s, 'a>,
    constructed: Option<&'c mut ConstructedObjects>,
}
impl FinalizedConsumer for Staging<'_, '_, '_> {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        if let Some(constructed) = &mut self.constructed {
            constructed.offer(&object);
        }
        self.sink.accept(object)
    }
}

impl BoundWorkspace {
    /// Commits the shared published frontier: every locally published change,
    /// whichever process made it. One attempt; this thread runs the
    /// synchronous captured ports, so it is never a kernel receive or
    /// request-service worker.
    pub fn commit_captured(
        &self,
        options: &CommitOptions,
    ) -> Result<CommitSuccess, Box<CommitFailure>> {
        self.commit_captured_via(options, layerfs_history::CommitVia::Workspace)
    }
    pub fn commit_captured_via(
        &self,
        options: &CommitOptions,
        via: layerfs_history::CommitVia,
    ) -> Result<CommitSuccess, Box<CommitFailure>> {
        let mut admission = self.admit_commit()?;
        let held = RefCell::new(None);
        // What construction staged for the shared cache. It is placed there
        // only by the publication callback; every other path drops it.
        let constructed = RefCell::new(None);
        let result = crate::telemetry::timed("commit", |timing| {
            self.commit_admitted(
                |save, capture, _| {
                    let mut slot = held.borrow_mut();
                    let mut staged = self.store.cache.constructed().ok();
                    let root = self.construct_captured(
                        save,
                        capture,
                        slot.insert(CapturedConstruction::default()),
                        staged.as_mut(),
                    )?;
                    *constructed.borrow_mut() = staged;
                    Ok(root)
                },
                || {
                    if let Some(staged) = constructed.borrow_mut().take() {
                        // Cache policy can lose a hit, never fail a known Commit.
                        let _ = self.store.cache.admit_constructed(staged);
                    }
                },
                options,
                via,
                timing,
            )
        });
        let mut held = held.into_inner();
        match result {
            Ok(mut success) => {
                if let Some(held) = &mut held {
                    self.release_captured(held);
                }
                admission.released = held.as_ref().is_none_or(|held| !held.retained());
                success.namespace = held;
                Ok(success)
            }
            Err(mut failure) => {
                // Only a definite, locally resolved nonpublication ends the
                // attempt's ownership. Anything else keeps every owner.
                if let (true, Some(held)) = (failure.locally_settled, &mut held) {
                    // The resolution is known done; its receipt is not kept
                    // past the releases.
                    self.release_captured(held);
                }
                admission.released =
                    failure.locally_settled && held.as_ref().is_none_or(|held| !held.retained());
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
        constructed: Option<&mut ConstructedObjects>,
    ) -> Result<FilesystemRootId, CommitError> {
        let operation = self.operation().map_err(CommitError::Workspace)?;
        // Only Commit acquires these owners, one capture at a time, so the
        // capture's own generation is unique among their live requests.
        let request = u64::try_from(capture.generation.number())
            .ok()
            .filter(|number| *number != 0)
            .ok_or(CommitError::Context("capture generation as request"))?;
        let reader = self
            .job(|job_client: &crate::OwnerClient, route| {
                job_client.acquire_captured_reader(route, capture, request)
            })
            .map_err(CommitError::Owner)?;
        held.reader = Some(reader);
        let owner = self
            .job(|job_client: &crate::OwnerClient, job_route| {
                job_client.acquire_operation(job_route, request)
            })
            .map_err(CommitError::Owner)?;
        held.operation = Some(owner);
        let policy = self.store.policy().construction();
        let mut sink = Staging {
            sink: save.sink(),
            constructed,
        };
        let digests = super::digests::StoreDigests(&*self.store.history);
        let attempt = crate::telemetry::timed("captured.namespace", |scope| {
            Ok::<_, std::convert::Infallible>(
                CapturedNamespace::new(operation.workspace(), operation.overlay(), reader, owner)
                    .with_digests(&digests)
                    .construct(
                        policy,
                        &policy.capacities(),
                        save,
                        &mut sink,
                        scope.child("construct"),
                    ),
            )
        });
        let CapturedNamespaceAttempt {
            result,
            mut custody,
        } = match attempt {
            Ok(attempt) => attempt,
            Err(never) => match never {},
        };
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
    /// The attempt's custody is dropped first, then the reader is released,
    /// then the operation owner. A release that is refused or fails stops the
    /// sequence; what it did not release stays named in the receipt. A done
    /// release's receipt is dropped at once.
    fn release_captured(&self, held: &mut CapturedConstruction) {
        held.custody = None;
        if let Some(reader) = held.reader {
            if !self.released(
                |job_client: &crate::OwnerClient, route| {
                    job_client.release_captured_reader(route, reader)
                },
                held,
            ) {
                return;
            }
            held.reader = None;
            held.released.push(ReleasedOwner::Reader);
        }
        if let Some(owner) = held.operation {
            if self.released(
                |job_client: &crate::OwnerClient, job_route| {
                    job_client.release_operation(job_route, owner)
                },
                held,
            ) {
                held.operation = None;
                held.released.push(ReleasedOwner::Operation);
            }
        }
    }
    fn released(
        &self,
        command: impl FnOnce(&crate::OwnerClient, layerfs_workspace::Route) -> Result<(), crate::OwnerError>
            + Send,
        held: &mut CapturedConstruction,
    ) -> bool {
        match self.job(command) {
            Ok(()) => true,
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
        Some(WorkspaceError::Service(service)) => match service.downcast::<OwnerError>() {
            Ok(owner) => CommitError::Owner(*owner),
            Err(other) => match other.downcast::<Arc<PortError>>() {
                Ok(port) => CommitError::Content {
                    error: label,
                    provider: Some(*port),
                },
                Err(other) => CommitError::Workspace(WorkspaceError::Service(other)),
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
