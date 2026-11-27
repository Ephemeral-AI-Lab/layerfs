//! The product Commit constructor: one captured namespace attempt inside the
//! unchanged driver. Its reader and operation owner are released only after
//! the driver's outcome is known; unresolved custody stays with the failure.
use super::pipeline::Piped;
use super::{
    BoundWorkspace, CapturedConstruction, CommitError, CommitFailure, CommitOptions, CommitSuccess,
    ReleasedOwner, StoreOperation,
};
use crate::OwnerError;
use layerfs_content::{
    compare::ChangedDirectories, filesystem::statistics::LogicalChanges,
    filesystem::FilesystemRootId, AuthenticatedObjects, ContentError, ContentResult,
    FilesystemResult, FinalizedConsumer, FinalizedObject, ObjectId,
};
use layerfs_overlay::{Capture, IndexedOperationRecordScope};
use layerfs_storage::{Save, SaveSink};
use layerfs_telemetry::timer::{Timing, TimingNode, TimingReport};
use layerfs_workspace::{
    CapturedNamespace, CapturedNamespaceAttempt, ConstructedObjects, IndexedEditRecords,
    WorkspaceError,
};
use std::{cell::RefCell, convert::Infallible, sync::Arc};

pub(super) struct Candidate {
    pub(super) root: FilesystemRootId,
    pub(super) logical: Option<CapturedLogical>,
}

pub(super) struct CapturedLogical {
    changes: LogicalChanges,
    scope: IndexedOperationRecordScope,
}

impl CapturedLogical {
    pub(super) fn matches(&self, before: FilesystemRootId, after: FilesystemRootId) -> bool {
        self.changes.matches(before, after)
    }

    pub(super) fn count(
        mut self,
        owner: &crate::OwnerClient,
        reader: &layerfs_storage::Reader<'_>,
        known: u64,
    ) -> Result<
        (
            u64,
            ChangedDirectories,
            Option<super::hints::CapturedBindings>,
        ),
        CommitError,
    > {
        let bindings = self
            .changes
            .take_bindings()
            .map(|bindings| super::hints::CapturedBindings::new(bindings, self.scope));
        let mut records = IndexedEditRecords::new(owner, self.scope);
        let counted = super::commit::with_file_lengths(reader, |lengths| {
            self.changes.count(&mut records, known, lengths)
        });
        match (counted, records.into_custody().failure) {
            (Err(CommitError::Content { error, .. }), Some(original)) => {
                Err(original_cause(error, Some(original), || None))
            }
            (counted, _) => counted.map(|(size, directories)| (size, directories, bindings)),
        }
    }
}

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
    /// whichever process made it. One attempt. This thread holds the Save
    /// and publishes; the one construction producer runs the synchronous
    /// captured ports on a thread it spawns for this Commit, never on a
    /// kernel receive or request-service worker.
    pub fn commit_captured(
        &self,
        options: &CommitOptions,
    ) -> Result<CommitSuccess, Box<CommitFailure>> {
        self.commit_captured_via(options, layerfs_history::CommitVia::Workspace)
    }
    /// One call: a quarantined PostgreSQL session reopens at most once in it.
    pub fn commit_captured_via(
        &self,
        options: &CommitOptions,
        via: layerfs_history::CommitVia,
    ) -> Result<CommitSuccess, Box<CommitFailure>> {
        layerfs_persistence::one_call(|| self.commit_captured_once(options, via))
    }
    fn commit_captured_once(
        &self,
        options: &CommitOptions,
        via: layerfs_history::CommitVia,
    ) -> Result<CommitSuccess, Box<CommitFailure>> {
        let mut admission = self.admit_commit()?;
        let held = RefCell::new(None);
        // What construction staged for the shared cache. It is placed there
        // only by the publication callback; every other path drops it.
        // Before that, validation reads the candidate's own objects from it
        // (`Built`), never through the shared cache.
        let constructed: RefCell<Option<Arc<ConstructedObjects>>> = RefCell::new(None);
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
                    *constructed.borrow_mut() = staged.map(Arc::new);
                    Ok(root)
                },
                || constructed.borrow().clone(),
                || {
                    // Validation has dropped its reader of the set by now; a
                    // set still shared is not admitted. Cache policy can lose
                    // a hit, never fail a known Commit.
                    let staged = constructed.borrow_mut().take().map(Arc::try_unwrap);
                    if let Some(Ok(staged)) = staged {
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
                if !admission.released {
                    self.retain(&failure, held.as_ref());
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
        constructed: Option<&mut ConstructedObjects>,
    ) -> Result<Candidate, CommitError> {
        let planned_reads = self.store.batched_chain_reads();
        let locators = self.admitted.as_ref().map(|admitted| {
            std::sync::Arc::new(layerfs_storage::read::CapturedLocators::new(
                admitted.clone(),
            ))
        });
        let operation = self
            .captured_operation(locators.clone())
            .map_err(CommitError::Workspace)?;
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
        let capacities = policy.capacities();
        let mut sink = Staging {
            sink: save.captured_sink(),
            constructed,
        };
        let digests = super::digests::StoreDigests(&*self.store.history);
        let (workspace, overlay) = (operation.workspace(), operation.overlay());
        // The one producer runs on its own thread; this thread keeps the
        // Save and accepts what it builds (`pipeline`).
        let piped = crate::telemetry::timed("captured.namespace", |scope| {
            let recording = scope.is_recording();
            let produce = |objects: &dyn AuthenticatedObjects,
                           consumer: &mut dyn FinalizedConsumer| {
                let namespace = CapturedNamespace::new(workspace, overlay, reader, owner)
                    .with_digests(&digests)
                    .with_inode_read_ahead(planned_reads)
                    .with_logical_transitions(planned_reads);
                if !recording {
                    let attempt =
                        namespace.construct_untimed(policy, &capacities, objects, consumer);
                    return (attempt, None);
                }
                let (attempt, report) = Timing::record("producer", |root| {
                    Ok::<_, Infallible>(namespace.construct(
                        policy,
                        &capacities,
                        objects,
                        consumer,
                        root.child("construct"),
                    ))
                });
                let attempt = attempt.unwrap_or_else(|never| match never {});
                (attempt, Some(report))
            };
            // A refused PostgreSQL demand or input window can leave objects
            // in the producer's private batch. Keep any acknowledged batches
            // in Save custody and drop that unsent batch.
            let piped = super::pipeline::construct(
                save,
                &mut sink,
                locators.as_ref(),
                produce,
                |(attempt, _)| !planned_reads || attempt.result.is_ok(),
            );
            if let Ok(Piped {
                produced: (_, Some(report)),
                accept,
                ..
            }) = &piped
            {
                // The producer's tree keeps its place below this root.
                for node in report.root().map_or(&[][..], TimingNode::children) {
                    scope.attach(Some(TimingReport::from_root(node.clone())));
                }
                scope.attach(Some(TimingReport::from_root(TimingNode::new(
                    "save.accept",
                    *accept,
                ))));
            }
            Ok::<_, Infallible>(piped)
        })
        .unwrap_or_else(|never| match never {})
        .map_err(|_| CommitError::Context("commit construction thread not started"))?;
        let Piped {
            produced: (attempt, _),
            rejected,
            ..
        } = piped;
        let CapturedNamespaceAttempt {
            mut result,
            logical_changes,
            mut custody,
        } = attempt;
        // The Save refused an object the producer had already handed over:
        // its error is the first cause, as when construction stopped there.
        if rejected && result.is_ok() {
            result = Err(ContentError::OutputRejected);
        }
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
                let logical = logical_changes.map(|changes| CapturedLogical {
                    changes,
                    scope: IndexedOperationRecordScope {
                        owner,
                        file_scope: 0,
                    },
                });
                Ok(Candidate {
                    root: built.root,
                    logical,
                })
            }
            Err(error) => {
                held.custody = Some(Box::new(custody));
                Err(error)
            }
        }
    }
    /// Drains, on this thread, the overlay reclamation a Commit's releases
    /// left queued: the captured generation's retired rows and the attempt's
    /// operation records. The daemon calls it after it answered the Commit.
    /// Without it the next job of this Workspace drains them, and terminal
    /// close deletes them with the file. A failed step is retained as the
    /// Workspace's maintenance failure, as for any maintenance.
    pub fn reclaim(&self) -> Result<(), OwnerError> {
        self.job(|job_client: &crate::OwnerClient, route| job_client.reclaim(route))
    }
    /// The attempt's custody is dropped first, then the reader is released,
    /// then the operation owner. A release that is refused or fails stops the
    /// sequence; what it did not release stays named in the receipt. A done
    /// release's receipt is dropped at once. Neither release drains the
    /// reclamation it readies: that follows the answer (`reclaim`).
    pub(super) fn release_captured(&self, held: &mut CapturedConstruction) {
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
                    job_client.release_operation_deferred(job_route, owner)
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

/// A Commit's validation source: the canonical objects its construction
/// built, then the ordinary Workspace client for anything else (an inherited
/// base object, or one a merge or whiteout step built). The reading client
/// checks every identity, whichever answered.
pub(super) struct Built {
    pub(super) objects: Arc<ConstructedObjects>,
    pub(super) store: Arc<layerfs_workspace::CanonicalClient>,
}
impl AuthenticatedObjects for Built {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        let mut values: Vec<Option<Vec<u8>>> = ids
            .iter()
            .map(|id| self.objects.object(*id).map(<[u8]>::to_vec))
            .collect();
        let missing: Vec<ObjectId> = ids
            .iter()
            .zip(&values)
            .filter(|(_, value)| value.is_none())
            .map(|(id, _)| *id)
            .collect();
        if !missing.is_empty() {
            let fetched = self.store.read_canonical_batch(&missing)?;
            if fetched.len() != missing.len() {
                return Err(ContentError::BatchCardinality {
                    requested: missing.len(),
                    returned: fetched.len(),
                });
            }
            let mut fetched = fetched.into_iter();
            for value in values.iter_mut().filter(|value| value.is_none()) {
                *value = fetched.next();
            }
        }
        values
            .into_iter()
            .map(|value| {
                value.ok_or(ContentError::BatchCardinality {
                    requested: ids.len(),
                    returned: 0,
                })
            })
            .collect()
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
    Err(original_cause(label, original, provider))
}

pub(super) fn original_cause(
    label: ContentError,
    original: Option<WorkspaceError>,
    provider: impl FnOnce() -> Option<Arc<super::PortError>>,
) -> CommitError {
    match original {
        // The port's own typed cause: an owner job, the Store provider's
        // failure, or a History failure that keeps its exact class (a
        // definite pre-effect `Busy` is not an unknown outcome, #647).
        Some(WorkspaceError::Service(service)) => match super::service_cause(service, label) {
            Ok(error) => error,
            Err(other) => CommitError::Workspace(WorkspaceError::Service(other)),
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
    }
}
