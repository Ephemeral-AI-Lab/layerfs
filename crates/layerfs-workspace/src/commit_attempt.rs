//! One retained attempt per Workspace. Filesystem operations use neither lock.
#[path = "commit_maintenance.rs"]
mod maintenance;
use crate::changes::{
    PreparedSnapshotCommit, PublishedCorrespondence, SnapshotCandidateDiagnostics,
};
use crate::snapshot::Snapshot;
use layerfs_layerstack_store::{
    BranchRecord, LayerStackStore, Result, StoreError, WorkspacePublicationAttempt,
    WorkspacePublicationReceipt, WorkspacePublicationResolution,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub(crate) struct PublishedContext {
    pub(crate) branch: BranchRecord,
    pub(crate) correspondence: PublishedCorrespondence,
}

pub(crate) struct Completion {
    pub(crate) receipt: WorkspacePublicationReceipt,
    pub(crate) diagnostics: Option<SnapshotCandidateDiagnostics>,
    pub(crate) cleanup_error: Option<StoreError>,
}

struct Attempt {
    expected: Arc<PublishedContext>,
    snapshot: Option<Snapshot>,
    publication: Option<WorkspacePublicationAttempt>,
    correspondence: Option<PublishedCorrespondence>,
    diagnostics: Option<SnapshotCandidateDiagnostics>,
    applied: Option<WorkspacePublicationReceipt>,
    known_not_published: bool,
}

pub(crate) struct CommitCoordinator {
    store: LayerStackStore,
    workspace: [u8; 16],
    published: Mutex<Arc<PublishedContext>>,
    // Held by Commit/explicit lifecycle only, never ordinary FS/SDK operations.
    attempt: Mutex<Option<Attempt>>,
    requests: AtomicUsize,
    max_requests: usize,
}

struct Request<'a>(&'a AtomicUsize);
impl Drop for Request<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

impl CommitCoordinator {
    pub(crate) fn new(
        store: LayerStackStore,
        workspace: [u8; 16],
        published: PublishedContext,
        max_requests: usize,
    ) -> Result<Self> {
        if max_requests == 0 {
            return Err(StoreError::InvalidInput("Commit queue capacity"));
        }
        Ok(Self {
            store,
            workspace,
            published: Mutex::new(Arc::new(published)),
            attempt: Mutex::new(None),
            requests: AtomicUsize::new(0),
            max_requests,
        })
    }

    pub(crate) fn published(&self) -> Result<Arc<PublishedContext>> {
        Ok(self
            .published
            .lock()
            .map_err(|_| StoreError::Integrity("published context lock"))?
            .clone())
    }

    pub(crate) fn has_pending(&self) -> Result<bool> {
        match self.attempt.try_lock() {
            Ok(attempt) => Ok(attempt.is_some()),
            Err(std::sync::TryLockError::WouldBlock) => Ok(true),
            Err(std::sync::TryLockError::Poisoned(_)) => {
                Err(StoreError::Integrity("Commit attempt lock"))
            }
        }
    }

    pub(crate) fn commit(
        &self,
        capture: impl FnOnce() -> Result<Snapshot>,
        mut build: impl FnMut(&Snapshot, &PublishedContext) -> Result<PreparedSnapshotCommit>,
    ) -> Result<Completion> {
        self.requests
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < self.max_requests).then_some(count + 1)
            })
            .map_err(|_| StoreError::StoreBusy)?;
        let _request = Request(&self.requests);
        let mut slot = self
            .attempt
            .lock()
            .map_err(|_| StoreError::Integrity("Commit attempt lock"))?;

        // A known completed Commit was already returned even if cleanup failed.
        // Settle that exact receipt before starting this request's new snapshot.
        if let Some(receipt) = slot.as_ref().and_then(|attempt| attempt.applied) {
            self.store
                .acknowledge_workspace_publication(&receipt.attempt)?;
            *slot = None;
        }
        if slot.is_none() {
            let expected = self.published()?;
            let snapshot = capture()?;
            if snapshot.root.sequence < expected.correspondence.covered_sequence {
                return Err(StoreError::Integrity("snapshot before published coverage"));
            }
            *slot = Some(Attempt {
                expected,
                snapshot: Some(snapshot),
                publication: None,
                correspondence: None,
                diagnostics: None,
                applied: None,
                known_not_published: false,
            });
        }

        let attempt = slot.as_mut().unwrap();
        if let Some(publication) = attempt.publication {
            match self.store.resolve_workspace_publication(&publication)? {
                WorkspacePublicationResolution::Published(receipt) => {
                    return self.finish(&mut slot, *receipt)
                }
                resolution => {
                    let stage = self.store.workspace_stage(self.workspace)?;
                    if let Some(stage) = stage {
                        if stage.branch_id != publication.branch_id
                            || stage.root_id != publication.candidate_root
                        {
                            return Err(StoreError::Integrity("retained Commit stage identity"));
                        }
                        // Canonical stage and completed correspondence now own
                        // everything retry needs; no live recapture or rebuild.
                        attempt.snapshot = None;
                        attempt.known_not_published = false;
                        let result = self.store.publish_workspace_stage(&publication);
                        let receipt = attempt.publication_result(result)?;
                        return self.finish(&mut slot, receipt);
                    }
                    if matches!(resolution, WorkspacePublicationResolution::Unknown) {
                        return Err(StoreError::Integrity(
                            "unknown Workspace publication outcome",
                        ));
                    }
                    if attempt.snapshot.is_none() {
                        return Err(StoreError::Integrity("missing retained Commit input"));
                    }
                    // Admission failed before a stage existed. Rebuild only the
                    // retained input and require the exact earlier candidate.
                }
            }
        }

        let prepared = match build(attempt.snapshot.as_ref().unwrap(), &attempt.expected) {
            Ok(prepared) => prepared,
            Err(error) => {
                if attempt.publication.is_none() {
                    *slot = None;
                }
                return Err(error);
            }
        };
        let captured = attempt.snapshot.as_ref().unwrap().root.sequence;
        if prepared.covered_sequence != captured
            || prepared.correspondence.covered_sequence != captured
            || prepared.correspondence.canonical_root != prepared.built.root_id
        {
            return Err(StoreError::Integrity(
                "incomplete snapshot candidate correspondence",
            ));
        }
        let publication = WorkspacePublicationAttempt::new(
            self.workspace,
            &attempt.expected.branch,
            attempt.expected.correspondence.canonical_root,
            prepared.built.root_id,
            attempt.expected.branch.base_layer_id,
            captured,
        );
        if attempt
            .publication
            .is_some_and(|previous| previous != publication)
        {
            return Err(StoreError::Integrity(
                "Commit retry changed the captured candidate",
            ));
        }
        attempt.publication = Some(publication);
        attempt.correspondence = Some(prepared.correspondence);
        attempt.diagnostics = Some(prepared.diagnostics);
        attempt.known_not_published = false;
        let result = self.store.commit_workspace_candidate_retained(
            &publication,
            prepared.built,
            prepared.admission,
        );
        let receipt = match attempt.publication_result(result) {
            Ok(receipt) => receipt,
            Err(error) => {
                // A complete canonical stage plus correspondence owns retry
                // input even when publication failed. Do not pin obsolete raw
                // bytes until another Commit request happens to arrive.
                if self
                    .store
                    .workspace_stage(self.workspace)
                    .is_ok_and(|stage| {
                        stage.is_some_and(|stage| {
                            stage.branch_id == publication.branch_id
                                && stage.root_id == publication.candidate_root
                        })
                    })
                {
                    attempt.snapshot = None;
                }
                return Err(error);
            }
        };
        self.finish(&mut slot, receipt)
    }

    fn finish(
        &self,
        slot: &mut Option<Attempt>,
        receipt: WorkspacePublicationReceipt,
    ) -> Result<Completion> {
        let attempt = slot
            .as_mut()
            .ok_or(StoreError::Integrity("missing publication attempt"))?;
        if attempt.publication != Some(receipt.attempt) {
            return Err(StoreError::Integrity(
                "publication receipt belongs to another attempt",
            ));
        }
        if attempt.applied.is_none() {
            let mut branch = attempt.expected.branch.clone();
            branch.head_commit_id = receipt.head_after;
            branch.base_layer_id = receipt.attempt.new_base;
            let correspondence = attempt
                .correspondence
                .as_ref()
                .ok_or(StoreError::Integrity("missing completed correspondence"))?
                .clone();
            let replacement = Arc::new(PublishedContext {
                branch,
                correspondence,
            });
            let previous = {
                let mut published = self
                    .published
                    .lock()
                    .map_err(|_| StoreError::Integrity("published context lock"))?;
                if !Arc::ptr_eq(&published, &attempt.expected) {
                    return Err(StoreError::Integrity(
                        "published context changed during attempt",
                    ));
                }
                std::mem::replace(&mut *published, replacement)
            };
            drop(previous);
            attempt.applied = Some(receipt);
            attempt.snapshot = None;
        }
        let diagnostics = attempt.diagnostics;
        // #144 D2: publication acknowledgment previously landed in unattributed.
        let acknowledge_started = std::time::Instant::now();
        let cleanup_error = self
            .store
            .acknowledge_workspace_publication(&receipt.attempt)
            .err();
        layerfs_layerstack_store::note_workspace_commit_phase(
            layerfs_layerstack_store::WorkspaceCommitPhase::Acknowledge,
            acknowledge_started.elapsed().as_nanos() as u64,
        );
        if cleanup_error.is_none() {
            *slot = None;
        }
        Ok(Completion {
            receipt,
            diagnostics,
            cleanup_error,
        })
    }

    /// Explicit lifecycle operation. Unknown publication is resolved first;
    /// discarding the live Workspace cannot erase an already published Commit.
    pub(crate) fn abandon(&self) -> Result<()> {
        let mut slot = self
            .attempt
            .lock()
            .map_err(|_| StoreError::Integrity("Commit attempt lock"))?;
        let Some(attempt) = slot.as_ref() else {
            return Ok(());
        };
        if let Some(receipt) = attempt.applied {
            self.store
                .acknowledge_workspace_publication(&receipt.attempt)?;
        } else if let Some(publication) = attempt.publication {
            let resolution = self.store.resolve_workspace_publication(&publication)?;
            match resolution {
                WorkspacePublicationResolution::Published(receipt) => {
                    let completion = self.finish(&mut slot, *receipt)?;
                    if let Some(error) = completion.cleanup_error {
                        return Err(error);
                    }
                    return Ok(());
                }
                resolution
                    if matches!(resolution, WorkspacePublicationResolution::NotPublished)
                        || attempt.known_not_published =>
                {
                    if let Some(stage) = self.store.workspace_stage(self.workspace)? {
                        if stage.branch_id != publication.branch_id
                            || stage.root_id != publication.candidate_root
                        {
                            return Err(StoreError::Integrity("abandon stage identity"));
                        }
                        self.store.discard_workspace_stage(self.workspace)?;
                    }
                }
                _ => return Err(StoreError::Integrity("unknown publication during abandon")),
            }
        }
        *slot = None;
        Ok(())
    }
}

impl Attempt {
    fn publication_result(
        &mut self,
        result: Result<WorkspacePublicationReceipt>,
    ) -> Result<WorkspacePublicationReceipt> {
        // These errors are returned before the publication transaction commits.
        // Retain that definite rollback witness even after the branch moves;
        // an I/O/database error never receives the same classification.
        self.known_not_published = matches!(
            &result,
            Err(StoreError::CommitHeadMoved { .. } | StoreError::LayerHeadMoved { .. })
        );
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::changes::SnapshotCandidateInputs;
    use crate::host_overlay::HostOverlay;
    use crate::overlay_budget::{Budget, HostAdmission, HostLimits};
    use layerfs_content::{filesystem, CanonicalPath, ObjectId};
    use layerfs_layerstack_store::{
        CoreReader, EntityName, LayerStackInitialization, LocalForkSource,
    };
    use layerfs_workspace_core::{ResourcePolicy, ROOT};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::mpsc;

    pub(super) struct Fixture {
        pub(super) directory: PathBuf,
        pub(super) host: HostOverlay,
        pub(super) coordinator: CommitCoordinator,
        scope: Option<ObjectId>,
        serials: Arc<Mutex<Option<std::ops::Range<u64>>>>,
        _runtime: layerfs_fuse::live_runtime::LiveRuntime,
    }
    impl Fixture {
        pub(super) fn new() -> Self {
            let id = crate::WorkspaceId::new();
            let directory = std::env::temp_dir().join(format!("layerfs-attempt-{id}"));
            let source = directory.join("source");
            fs::create_dir_all(&source).unwrap();
            fs::write(source.join("file"), vec![b'a'; 64 * 1024]).unwrap();
            let store = LayerStackStore::create(directory.join("store.sqlite")).unwrap();
            let genesis = store
                .initialize_layerstack(
                    EntityName::new("attempt").unwrap(),
                    LayerStackInitialization::Directory(source.clone()),
                )
                .unwrap()
                .genesis_layer_id;
            let branch = store
                .fork_branch(
                    EntityName::new("main").unwrap(),
                    LocalForkSource::Layer { layer_id: genesis },
                )
                .unwrap();
            let pinned = store.pin_branch(branch).unwrap();
            let scope = filesystem::namespace(&CoreReader(&pinned.reader), pinned.root)
                .unwrap()
                .scope;
            let policy = ResourcePolicy::default();
            let runtime = layerfs_fuse::live_runtime::LiveRuntime::new().unwrap();
            let admission = HostAdmission::new(runtime.scheduler(), HostLimits::default()).unwrap();
            let resources = Budget::open(&directory, policy, admission).unwrap();
            let comparison =
                PublishedCorrespondence::empty(&resources.index, pinned.root, 0).unwrap();
            let coordinator = CommitCoordinator::new(
                store,
                id.bytes(),
                PublishedContext {
                    branch: pinned.branch,
                    correspondence: comparison,
                },
                8,
            )
            .unwrap();
            let host = HostOverlay::new(pinned.reader, pinned.root, id.bytes(), policy, resources)
                .unwrap();
            fs::remove_dir_all(source).unwrap();
            Self {
                directory,
                host,
                coordinator,
                scope,
                serials: Default::default(),
                _runtime: runtime,
            }
        }
        pub(super) fn build(
            &self,
            snapshot: &Snapshot,
            context: &PublishedContext,
        ) -> Result<PreparedSnapshotCommit> {
            SnapshotCandidateInputs {
                snapshot,
                budget: &self.host.budget,
                ranges: &self.host.ranges,
                index: &self.host.index,
                origins: self.host.origins(),
                comparison: &context.correspondence,
                store: &self.coordinator.store,
                workspace_id: self.coordinator.workspace,
                scope: self.scope,
                serials: self.serials.clone(),
                policy: self.host.policy,
                spool: &self.directory,
            }
            .build(1)
        }
        pub(super) fn commit(&self) -> Result<Completion> {
            self.coordinator
                .commit(|| self.host.snapshot(), |s, c| self.build(s, c))
        }
        fn committed(&self, root: ObjectId) -> Vec<u8> {
            Self::read_committed(&self.coordinator.store, root)
        }
        fn read_committed(store: &LayerStackStore, root: ObjectId) -> Vec<u8> {
            let reader = store.snapshot_reader(root);
            let resolved = filesystem::resolve(
                &CoreReader(&reader),
                root,
                &CanonicalPath::new("file").unwrap(),
                &mut filesystem::LogicalCounters::default(),
            )
            .unwrap();
            let mut bytes = Vec::new();
            layerfs_content::file::content::read_range(
                &CoreReader(&reader),
                layerfs_content::file::content::FileContentRoot(resolved.record.content_root),
                0..layerfs_content::file::content::length(
                    &CoreReader(&reader),
                    layerfs_content::file::content::FileContentRoot(resolved.record.content_root),
                )
                .unwrap(),
                &mut bytes,
            )
            .unwrap();
            bytes
        }
        fn verify_reopen(self, roots: &[(ObjectId, [u8; 2])]) {
            let Self {
                directory,
                host,
                coordinator,
                ..
            } = self;
            drop((host, coordinator));
            let store = LayerStackStore::connect(directory.join("store.sqlite")).unwrap();
            for (root, expected) in roots {
                assert_eq!(&Self::read_committed(&store, *root)[..2], expected);
            }
            drop(store);
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn held_construction_keeps_live_operations_and_c1_c2_predecessor_independent() {
        let fixture = Fixture::new();
        let node = fixture.host.lookup(ROOT, b"file").unwrap().node;
        fixture.host.write(node, 0, b"X").unwrap();
        let captured = fixture.host.snapshot().unwrap().root.sequence;
        let (started, at_build) = mpsc::channel();
        let (resume, continue_build) = mpsc::channel();
        let first = std::thread::scope(|scope| {
            let shared = &fixture;
            let building = scope.spawn(move || {
                shared.coordinator.commit(
                    || shared.host.snapshot(),
                    |s, c| {
                        started.send(()).unwrap();
                        continue_build.recv().unwrap();
                        shared.build(s, c)
                    },
                )
            });
            at_build.recv().unwrap();
            fixture.host.write(node, 1, b"Y").unwrap();
            fixture.host.pin(node, false).unwrap();
            fixture.host.unpin(node).unwrap();
            let mut live = [0; 2];
            fixture.host.read_into(node, 0, &mut live).unwrap();
            assert_eq!(&live, b"XY");
            resume.send(()).unwrap();
            building.join().unwrap().unwrap()
        });
        assert_eq!(first.receipt.attempt.covered_sequence, captured);
        assert_eq!(
            &fixture.committed(first.receipt.attempt.candidate_root)[..2],
            b"Xa"
        );
        assert!(!first.receipt.up_to_date);
        let later = fixture.host.snapshot().unwrap().root.sequence;
        let second = fixture.commit().unwrap();
        assert_eq!(second.receipt.attempt.covered_sequence, later);
        assert_eq!(
            &fixture.committed(second.receipt.attempt.candidate_root)[..2],
            b"XY"
        );
        let diagnostics = second.diagnostics.unwrap();
        assert_eq!(diagnostics.full_comparisons, 0);
        assert!(diagnostics.reused_bytes >= 64 * 1024 - 2);
        // Full-attempt peaks are a distinct domain from construction-only
        // peaks: the attempt sample is taken after synchronous admission and
        // may never be below what construction alone already reserved.
        assert!(diagnostics.attempt_peak_sampled);
        assert!(
            diagnostics.attempt_peak_reserved_bytes >= diagnostics.scratch_peak_reserved_bytes,
            "attempt peak {} below construction peak {}",
            diagnostics.attempt_peak_reserved_bytes,
            diagnostics.scratch_peak_reserved_bytes
        );
        assert!(diagnostics.attempt_peak_reserved_files >= diagnostics.scratch_peak_reserved_files);
        assert!(
            diagnostics.attempt_peak_reserved_bytes <= diagnostics.construction_memory_reservation,
            "attempt peak {} exceeds the admitted construction reservation {}",
            diagnostics.attempt_peak_reserved_bytes,
            diagnostics.construction_memory_reservation
        );
        let third = fixture.commit().unwrap();
        assert!(third.receipt.up_to_date);
        assert_eq!(third.receipt.head_after, second.receipt.head_after);
        assert_eq!(third.diagnostics.unwrap().changed_keys, 0);
        fixture.verify_reopen(&[
            (first.receipt.attempt.candidate_root, *b"Xa"),
            (second.receipt.attempt.candidate_root, *b"XY"),
        ]);
        println!("held builder live write/read/pins; fresh-reopen C1=Xa C2=XY; C2 diagnostics={diagnostics:?}; C3 no-op PASS");
    }

    #[test]
    fn lost_publication_and_stage_replies_retry_exact_input_without_recapture() {
        let fixture = Fixture::new();
        let node = fixture.host.lookup(ROOT, b"file").unwrap().node;
        for (offset, fault) in [(0, u64::MAX - 2), (1, u64::MAX - 3)] {
            fixture.host.write(node, offset, b"R").unwrap();
            let sequence = fixture.host.snapshot().unwrap().root.sequence;
            layerfs_layerstack_store::set_transaction_failure_at(Some(fault));
            let failed = fixture.commit();
            layerfs_layerstack_store::set_transaction_failure_at(None);
            assert!(failed.is_err());
            if fault == u64::MAX - 2 {
                assert!(
                    fixture
                        .coordinator
                        .attempt
                        .lock()
                        .unwrap()
                        .as_ref()
                        .unwrap()
                        .snapshot
                        .is_none(),
                    "a retained canonical stage must release redundant raw snapshot ownership"
                );
            }
            fixture.host.write(node, offset + 8, b"L").unwrap();
            let completion = fixture
                .coordinator
                .commit(
                    || panic!("retry must not recapture live state"),
                    |_, _| panic!("retained stage/receipt must not rebuild"),
                )
                .unwrap();
            assert_eq!(completion.receipt.attempt.covered_sequence, sequence);
            assert!(fixture
                .coordinator
                .store
                .workspace_stage(fixture.coordinator.workspace)
                .unwrap()
                .is_none());
            assert_eq!(
                fixture.committed(completion.receipt.attempt.candidate_root)[offset as usize],
                b'R'
            );
            assert_eq!(
                fixture.committed(completion.receipt.attempt.candidate_root)[offset as usize + 8],
                b'a'
            );
        }
        // UpToDate has no unique new Commit; its exact receipt must resolve too.
        fixture.commit().unwrap();
        layerfs_layerstack_store::set_transaction_failure_at(Some(u64::MAX - 3));
        let failed = fixture.commit();
        layerfs_layerstack_store::set_transaction_failure_at(None);
        assert!(failed.is_err());
        let noop = fixture
            .coordinator
            .commit(
                || panic!("lost no-op receipt cannot acquire a new snapshot"),
                |_, _| panic!("lost no-op receipt cannot rebuild"),
            )
            .unwrap();
        assert!(noop.receipt.up_to_date);
        // Cleanup failure still returns the known publication and settles before
        // the next request captures a new boundary.
        layerfs_layerstack_store::set_transaction_failure_at(Some(u64::MAX - 4));
        let completed = fixture.commit().unwrap();
        layerfs_layerstack_store::set_transaction_failure_at(None);
        assert!(completed.cleanup_error.is_some());
        assert!(fixture.commit().unwrap().receipt.up_to_date);
        fixture.verify_reopen(&[(noop.receipt.attempt.candidate_root, *b"RR")]);
        println!("exact stage/lost Created/lost UpToDate retries and known-success cleanup; fresh reopen: PASS");
    }

    #[test]
    fn definite_head_conflict_can_abandon_stage_without_losing_live_writes() {
        let fixture = Fixture::new();
        let node = fixture.host.lookup(ROOT, b"file").unwrap().node;
        fixture.host.write(node, 0, b"S").unwrap();
        let mut winner_root = None;
        let result = fixture.coordinator.commit(
            || fixture.host.snapshot(),
            |snapshot, context| {
                let store = &fixture.coordinator.store;
                let reader = store.snapshot_reader(context.correspondence.canonical_root);
                let built = layerfs_layerstack_store::apply_changes(
                    &reader,
                    context.correspondence.canonical_root,
                    &[filesystem::ContentChange::Write {
                        path: "file".into(),
                        bytes: vec![b'W'; 64 * 1024],
                        mode: 0o600,
                    }],
                    [77; 32],
                )?;
                winner_root = Some(built.root_id);
                store.commit_candidate(
                    &context.branch,
                    context.correspondence.canonical_root,
                    context.branch.base_layer_id,
                    built,
                )?;
                fixture.build(snapshot, context)
            },
        );
        assert!(matches!(result, Err(StoreError::CommitHeadMoved { .. })));
        {
            let attempt = fixture.coordinator.attempt.lock().unwrap();
            let attempt = attempt.as_ref().unwrap();
            assert!(attempt.known_not_published);
            assert!(attempt.snapshot.is_none());
            assert!(matches!(
                fixture
                    .coordinator
                    .store
                    .resolve_workspace_publication(&attempt.publication.unwrap())
                    .unwrap(),
                WorkspacePublicationResolution::Unknown
            ));
        }
        fixture.host.write(node, 1, b"L").unwrap();
        fixture.coordinator.abandon().unwrap();
        assert!(fixture
            .coordinator
            .store
            .workspace_stage(fixture.coordinator.workspace)
            .unwrap()
            .is_none());
        assert!(fixture.coordinator.attempt.lock().unwrap().is_none());
        assert_eq!(
            fixture
                .coordinator
                .published()
                .unwrap()
                .correspondence
                .covered_sequence,
            0
        );
        let mut live = [0; 2];
        fixture.host.read_into(node, 0, &mut live).unwrap();
        assert_eq!(&live, b"SL");
        fixture.verify_reopen(&[(winner_root.unwrap(), *b"WW")]);
        println!("definite conflict rollback witness permits exact stage abandonment; live writes and winning branch remain intact: PASS");
    }
}
