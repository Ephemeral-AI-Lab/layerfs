//! Remote (sandbox-owned) workspace Commit: non-pausing snapshot capture,
//! frozen-input pull, existing single-worker canonical construction, exact
//! Store publication, and generation-safe completion.
//!
//! The flow never pauses or quiesces the live workspace: the snapshot is
//! stable by frontier ownership in the sandbox, and SDK edits, status and
//! immutable-base reads proceed throughout. The host holds no lock across
//! the build that live service needs — the frozen input is an owned copy.

use crate::changes::construction_worker_limit;
use crate::live_backing::RemoteWorkspace;
use crate::session::{WorkspaceCommitResult, WorkspaceCommitStatus};
use crate::snapshot_input::{CaptureSummary, CompletionRecord, FrozenRemoteInput, SnapshotToken};
use crate::{Workspace, WorkspaceError, WorkspaceResult};
use layerfs_content::ObjectId;
use layerfs_layerstack_store::{
    CommitId, CommitOutcome, CoreReader, LayerId, ObjectBuffer, StoreError,
};
use layerfs_workspace_core::{Attr, Kind, Node, NodeId};
use std::collections::HashMap;
use std::time::Instant;

/// An undelivered completion for a published generation: the exact attempt
/// outcome, retained until the sandbox acknowledges it. A retry re-delivers
/// these records; it never recaptures newer state.
#[derive(Clone)]
pub(crate) struct PendingCompletion {
    pub token: SnapshotToken,
    pub root: ObjectId,
    pub head: Option<[u8; 33]>,
    pub expected_base: LayerId,
    pub records: Vec<CompletionRecord>,
}

/// Resolve a prior undelivered completion, then re-drive a retained attempt or
/// capture, pull, build, publish and complete one generation. Called under the
/// workspace lifecycle lock (one active committer, at most one queued).
pub(crate) fn commit_remote(
    construction: &std::sync::Mutex<()>,
    worker: &crate::worker::WorkspaceWorker,
    remote: &RemoteWorkspace,
) -> WorkspaceResult<WorkspaceCommitStatus> {
    let (completion_pending, retained_attempt, stage_retained) = {
        let workspace = worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        if workspace.presentation_failed {
            return Err(WorkspaceError::InvalidExecution);
        }
        (
            workspace.pending_completion.is_some(),
            workspace.pending_attempt,
            workspace.pending_stage.is_some(),
        )
    };
    // Resolve a prior publication's undelivered completion first: its outcome
    // is authoritative and the exact attempt must finish before anything new
    // can be re-driven or captured.
    if completion_pending {
        resolve_pending_completion(worker, remote)?;
    }
    let summary = match retained_attempt {
        // A prior Commit failed after capture. The sandbox still owns that
        // frozen attempt, so the supported retry re-drives it exactly: it is
        // never recaptured as newer state and never refused.
        Some(summary) => summary,
        None if stage_retained => {
            // A retained stage whose attempt is gone cannot be re-driven: the
            // Store holds admitted objects for a publication this route can no
            // longer reproduce. Only an explicit Discard resolves it.
            return Err(WorkspaceError::Storage(StoreError::InvalidInput(
                "workspace stage retained",
            )));
        }
        None => {
            // Capture the frozen generation (short, sandbox-local).
            let started = Instant::now();
            let summary = remote.capture()?;
            layerfs_layerstack_store::note_workspace_commit_phase(
                layerfs_layerstack_store::WorkspaceCommitPhase::Capture,
                elapsed_ns(started),
            );
            summary
        }
    };
    // The sandbox resolves one attempt at a time and holds this one until it is
    // completed or cancelled. Retain it before the first step that can fail, so
    // a failed Commit leaves a retryable attempt instead of an unreachable
    // snapshot slot.
    {
        let mut workspace = worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        workspace.pending_attempt = Some(summary);
    }
    let commit_read_before = worker
        .workspace
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?
        .reader
        .read_metrics_snapshot()?;
    commit_attempt(construction, worker, remote, summary, commit_read_before)
}

/// Pull, build, publish and complete one retained (or freshly captured)
/// generation. Every failure before publication settles leaves the attempt
/// retained on the workspace, so the next Commit re-drives it.
fn commit_attempt(
    construction: &std::sync::Mutex<()>,
    worker: &crate::worker::WorkspaceWorker,
    remote: &RemoteWorkspace,
    summary: CaptureSummary,
    commit_read_before: layerfs_layerstack_store::WorkspaceReadReceipt,
) -> WorkspaceResult<WorkspaceCommitStatus> {
    let token = summary.token;
    // Pull the changed records. Payload bytes stream from the sandbox backing
    // during construction; nothing is staged wholesale on the host.
    let started = Instant::now();
    let input = remote.pull_frozen_input(&summary)?;
    layerfs_layerstack_store::note_workspace_commit_phase(
        layerfs_layerstack_store::WorkspaceCommitPhase::CandidatePlan,
        elapsed_ns(started),
    );

    // Edit-state telemetry over the materialized frontier.
    {
        let spool: u64 = input
            .nodes
            .values()
            .filter_map(|node| match &node.data {
                layerfs_workspace_core::Data::File(layerfs_workspace_core::FileData::Edited {
                    spool_high_water,
                    ..
                }) => Some(*spool_high_water),
                _ => None,
            })
            .sum();
        Workspace::note_edit_state_over(&input.nodes, &input.dirty, spool, spool)?;
    }

    // An empty frontier publishes an empty candidate (UpToDate/HeadMoved
    // detection) and releases the attempt without completion.
    if summary.frontier_len == 0 {
        let outcome = publish_empty(worker)?;
        return match outcome {
            Outcome::Published(outcome) | Outcome::WithBase(outcome, _) => {
                let _ = remote.cancel_snapshot(token);
                settle_attempt(worker)?;
                Ok(WorkspaceCommitStatus {
                    result: result_from_outcome(outcome, worker)?,
                    presentation_failed: false,
                })
            }
            Outcome::Unpublished(result) => {
                let _ = remote.cancel_snapshot(token);
                settle_attempt(worker)?;
                Ok(WorkspaceCommitStatus {
                    result,
                    presentation_failed: false,
                })
            }
        };
    }

    // Construction: one shared canonical compute worker across workspaces.
    // The build borrows the workspace shell immutably; live service is never
    // blocked by it.
    let prepared = {
        let _gate = construction
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        let started = Instant::now();
        let workspace = worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        // The candidate-build injection and the Store's admission-fault
        // activation belong to "this route is building a candidate", exactly as
        // they do before the materialized route's build. Keeping both here is
        // what stops the two routes from drifting apart again.
        #[cfg(feature = "test-instrumentation")]
        {
            if crate::lifecycle::consume_verification_fault(
                worker.request.branch_id,
                crate::lifecycle::VerificationFault::Candidate,
                0,
            ) {
                crate::changes::inject_candidate_failure_once();
            }
            layerfs_layerstack_store::verification_candidate(worker.request.branch_id, 0);
        }
        let prepared = workspace
            .build_remote_candidate(&input, construction_worker_limit())
            .map_err(WorkspaceError::from)?;
        drop(workspace);
        layerfs_layerstack_store::note_workspace_commit_phase(
            layerfs_layerstack_store::WorkspaceCommitPhase::CandidateFinish,
            elapsed_ns(started),
        );
        prepared
    };

    // Publication under the existing conditional Store transaction. Busy and
    // HeadMoved surface as commit results; the attempt is released.
    let started = Instant::now();
    let published = publish_prepared(worker, prepared)?;
    let (outcome, expected_base) = match published {
        Outcome::WithBase(outcome, expected_base) => (outcome, expected_base),
        Outcome::Unpublished(result) => {
            let _ = remote.cancel_snapshot(token);
            settle_attempt(worker)?;
            return Ok(WorkspaceCommitStatus {
                result,
                presentation_failed: false,
            });
        }
        Outcome::Published(outcome) => {
            let _ = remote.cancel_snapshot(token);
            settle_attempt(worker)?;
            return Ok(WorkspaceCommitStatus {
                result: result_from_outcome(outcome, worker)?,
                presentation_failed: false,
            });
        }
    };
    layerfs_layerstack_store::note_workspace_commit_phase(
        layerfs_layerstack_store::WorkspaceCommitPhase::Publication,
        elapsed_ns(started),
    );
    // Publication is authoritative: this attempt's transfer is settled. The
    // completion below is either delivered now or retained exactly.
    settle_attempt(worker)?;

    // Completion records: the builder's checkpoint journal plus each node's
    // captured revision. Delivery is idempotent and token-guarded.
    let started = Instant::now();
    let records = completion_records(worker, &input)?;
    let (root, head) = match &outcome {
        CommitOutcome::Committed {
            commit_id, root_id, ..
        } => (*root_id, Some(commit_id.to_bytes())),
        CommitOutcome::UpToDate { root_id } => {
            let expected = worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?
                .expected_head;
            (*root_id, expected.map(|head| head.to_bytes()))
        }
    };
    let delivered = remote.complete_generation(token, root, head, &records);
    layerfs_layerstack_store::note_workspace_commit_phase(
        layerfs_layerstack_store::WorkspaceCommitPhase::Checkpoint,
        elapsed_ns(started),
    );
    let completion_delivered = delivered.is_ok();

    // Re-base the host workspace onto the published root. The Commit result
    // must report the head this route observed *before* publication, so it is
    // sampled here, under the lock this block already holds.
    let previous_head = {
        let mut workspace = worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        let previous_head = workspace.expected_head;
        rebase_host_workspace(&mut workspace, root, head, expected_base)?;
        if !completion_delivered {
            // The publication is authoritative; its exact completion is
            // retained and re-delivered by the next Commit or End.
            workspace.pending_completion = Some(PendingCompletion {
                token,
                root,
                head,
                expected_base,
                records,
            });
        }
        previous_head
    };

    // Read-metrics observation failure does not roll back publication but
    // marks presentation as failed, exactly as on the materialized route.
    let observations = worker
        .workspace
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?
        .reader
        .read_metrics_snapshot()
        .and_then(|after| {
            layerfs_layerstack_store::note_workspace_commit_reads(commit_read_before, after)
        });

    let result = result_from_pre_publication_head(outcome, previous_head);
    #[allow(unused_mut)]
    let mut presentation_failed = !completion_delivered || observations.is_err();
    #[cfg(feature = "test-instrumentation")]
    {
        // The sandbox-owned route has no freeze/resume; `projection::resume`
        // only owns the recorded failure state. Consume the one-shot fault at
        // the same transition the materialized route does: after a Commit
        // published a new generation.
        let injected = matches!(&result, WorkspaceCommitResult::Created { .. })
            && crate::lifecycle::consume_verification_fault(
                worker.request.branch_id,
                crate::lifecycle::VerificationFault::PresentationResume,
                0,
            )
            && {
                crate::projection::inject_resume_failure_once();
                crate::projection::resume(worker).is_err()
            };
        presentation_failed |= injected;
        if injected {
            // Record the failure on the workspace so it is recoverable, exactly
            // as the materialized transition does. A retained completion is not
            // recorded here: it is re-delivered by the next Commit or End.
            worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?
                .presentation_failed = true;
        }
    }
    Ok(WorkspaceCommitStatus {
        result,
        presentation_failed,
    })
}

/// Mark this attempt settled: the sandbox attempt was completed or cancelled,
/// so no retry re-drives it.
fn settle_attempt(worker: &crate::worker::WorkspaceWorker) -> WorkspaceResult<()> {
    worker
        .workspace
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?
        .pending_attempt = None;
    Ok(())
}

/// Re-deliver a pending completion and settle the prior publication's
/// bookkeeping. The publication outcome is never re-executed.
pub(crate) fn resolve_pending_completion(
    worker: &crate::worker::WorkspaceWorker,
    remote: &RemoteWorkspace,
) -> WorkspaceResult<()> {
    let pending = {
        let workspace = worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        workspace.pending_completion.clone()
    };
    let Some(pending) = pending else {
        return Ok(());
    };
    remote.complete_generation(pending.token, pending.root, pending.head, &pending.records)?;
    let mut workspace = worker
        .workspace
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?;
    workspace.pending_completion = None;
    workspace.pending_publication = None;
    rebase_host_workspace(
        &mut workspace,
        pending.root,
        pending.head,
        pending.expected_base,
    )?;
    Ok(())
}

enum Outcome {
    Published(CommitOutcome),
    Unpublished(WorkspaceCommitResult),
    WithBase(CommitOutcome, LayerId),
}

fn result_from_outcome(
    outcome: CommitOutcome,
    worker: &crate::worker::WorkspaceWorker,
) -> WorkspaceResult<WorkspaceCommitResult> {
    let previous_head = worker
        .workspace
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?
        .expected_head;
    Ok(WorkspaceCommitResult::from_outcome(outcome, previous_head))
}

/// Assemble the Commit result from the head observed *before* publication.
/// Re-basing the host shell advances `expected_head` to the newly published
/// head, so reading it afterwards would report a Commit as its own
/// predecessor on the host-continuation route.
fn result_from_pre_publication_head(
    outcome: CommitOutcome,
    previous_head: Option<CommitId>,
) -> WorkspaceCommitResult {
    WorkspaceCommitResult::from_outcome(outcome, previous_head)
}

fn publish_empty(worker: &crate::worker::WorkspaceWorker) -> WorkspaceResult<Outcome> {
    let workspace = worker
        .workspace
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?;
    let empty = ObjectBuffer::new(&workspace.reader)
        .and_then(|buffer| buffer.finish(workspace.base_root, 0))?;
    let admission = workspace
        .store
        .workspace_admission(workspace.workspace_id)?;
    let mut branch = workspace
        .store
        .branch(workspace.branch_id)?
        .ok_or(StoreError::NotFound("Branch"))?;
    branch.head_commit_id = workspace.expected_head;
    branch.base_layer_id = workspace.expected_base;
    let outcome = workspace.store.commit_workspace_candidate(
        workspace.workspace_id,
        &branch,
        workspace.base_root,
        workspace.expected_base,
        empty,
        admission,
    );
    match outcome {
        Ok(outcome) => Ok(Outcome::Published(outcome)),
        Err(error) => match WorkspaceError::from_commit(error) {
            Ok(result) => Ok(Outcome::Unpublished(result)),
            Err(error) => Err(error),
        },
    }
}

fn publish_prepared(
    worker: &crate::worker::WorkspaceWorker,
    prepared: crate::changes::PreparedCommit,
) -> WorkspaceResult<Outcome> {
    let mut workspace = worker
        .workspace
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?;
    let crate::changes::PreparedCommit {
        built,
        checkpoint,
        admission,
    } = prepared;
    let admission = admission.ok_or(StoreError::Integrity("Commit admission handoff"))?;
    let mut branch = workspace
        .store
        .branch(workspace.branch_id)?
        .ok_or(StoreError::NotFound("Branch"))?;
    branch.head_commit_id = workspace.expected_head;
    branch.base_layer_id = workspace.expected_base;
    let candidate_root = built.root_id;
    let outcome = workspace.store.commit_workspace_candidate(
        workspace.workspace_id,
        &branch,
        workspace.base_root,
        workspace.expected_base,
        built,
        admission,
    );
    if outcome.is_err() {
        workspace.note_retained_stage(candidate_root);
    }
    match outcome {
        Ok(outcome) => {
            workspace.pending_stage = None;
            workspace.pending_checkpoint = Some(checkpoint);
            Ok(Outcome::WithBase(outcome, workspace.expected_base))
        }
        Err(error) => match WorkspaceError::from_commit(error) {
            Ok(result) => Ok(Outcome::Unpublished(result)),
            Err(error) => Err(error),
        },
    }
}

fn completion_records(
    worker: &crate::worker::WorkspaceWorker,
    input: &FrozenRemoteInput,
) -> WorkspaceResult<Vec<CompletionRecord>> {
    let workspace = worker
        .workspace
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?;
    let checkpoint = workspace
        .pending_checkpoint
        .as_ref()
        .ok_or(StoreError::Integrity("missing published checkpoint"))?;
    let mut records = Vec::new();
    let nodes: &HashMap<NodeId, Node> = &input.nodes;
    checkpoint.visit(|id, inode, content, attr| {
        completion_record(id, inode, content, attr, nodes, &mut records)
    })?;
    Ok(records)
}

fn completion_record(
    id: NodeId,
    inode: layerfs_content::tree::inode::InodeId,
    content: ObjectId,
    attr: Attr,
    nodes: &HashMap<NodeId, Node>,
    records: &mut Vec<CompletionRecord>,
) -> Result<(), StoreError> {
    let revision = nodes
        .get(&id)
        .ok_or(StoreError::Integrity("completion revision"))?
        .revision;
    records.push(CompletionRecord {
        node: id,
        revision,
        inode: *inode.as_bytes(),
        content: content.to_bytes(),
        size: attr.size,
        mode: attr.mode,
        links: attr.links,
        mtime_seconds: attr.mtime_seconds,
        mtime_nanoseconds: attr.mtime_nanoseconds,
        kind: match attr.kind {
            Kind::File => 1,
            Kind::Directory => 2,
            Kind::Symlink => 3,
        },
    });
    Ok(())
}

/// Re-base the host workspace shell onto the published root and refresh the
/// immutable-base service context.
fn rebase_host_workspace(
    workspace: &mut Workspace,
    root: ObjectId,
    head: Option<[u8; 33]>,
    expected_base: LayerId,
) -> Result<(), StoreError> {
    let reader = workspace
        .store
        .snapshot_reader(root)
        .with_read_metrics_from(&workspace.reader);
    let namespace = layerfs_content::filesystem::namespace(&CoreReader(&reader), root)?;
    workspace.reader = reader;
    workspace.expected_head = head.map(CommitId::from_bytes).transpose()?;
    workspace.expected_base = expected_base;
    workspace.base_root = root;
    workspace.base_inodes =
        layerfs_content::tree::inode::InodeTableRoot(namespace.inode_table_root);
    workspace.directory_lookup_cache = Default::default();
    workspace.resolution = None;
    // The immutable-base service reads against the new root.
    if let Some(remote) = &workspace.remote {
        let mut backing = remote
            .backing
            .lock()
            .map_err(|_| StoreError::Integrity("live backing lock"))?;
        backing.snapshot.reader = workspace.reader.clone();
        backing.snapshot.root = root;
    }
    Ok(())
}

fn elapsed_ns(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(all(test, feature = "test-instrumentation"))]
mod tests {
    use super::*;
    use crate::worker::{WorkspaceIdentity, WorkspaceWorker};
    use crate::{
        CreateWorkspaceSession, WorkspaceFileRangeEdit, WorkspaceFileReplacement, WorkspaceId,
        WorkspacePlacement, WorkspaceProjection,
    };
    use layerfs_layerstack_store::{
        arm_verification_store_fault, begin_workspace_commit,
        take_verification_store_fault_receipt, CaptureMode, EntityName, LayerStackInitialization,
        LayerStackStore, LocalForkSource, VerificationStoreFault,
    };

    /// A publication failure must leave a *retryable* attempt, not a workspace
    /// that can never capture again. The sandbox resolves one attempt at a
    /// time, so before this behaviour a failed final publication (the injected
    /// fault is exactly `FinalPublication`) left the snapshot slot occupied and
    /// the only recovery was `EndWorkspaceMode::Discard` — throwing the
    /// retained publication away. This test fails without the retained attempt:
    /// the retry returns `InvalidInput("workspace stage retained")`.
    #[test]
    fn failed_publication_is_re_driven_by_the_supported_commit_retry() {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-remote-commit-retry-{}",
            WorkspaceId::new()
        ));
        let fixture = directory.join("fixture");
        std::fs::create_dir_all(&fixture).unwrap();
        std::fs::write(fixture.join("file"), vec![0u8; 4096]).unwrap();
        let store = LayerStackStore::create(directory.join("store.sqlite")).unwrap();
        let layer = store
            .initialize_layerstack(
                EntityName::new("project").unwrap(),
                LayerStackInitialization::Directory(fixture),
            )
            .unwrap()
            .genesis_layer_id;
        let branch = store
            .fork_branch(
                EntityName::new("main").unwrap(),
                LocalForkSource::Layer { layer_id: layer },
            )
            .unwrap();
        let mut workspace =
            crate::Workspace::open(store.clone(), branch, directory.join("spool")).unwrap();
        let workspace_id = crate::WorkspaceId::new();
        let pinned = store.pin_branch(branch).unwrap();
        let identity = WorkspaceIdentity {
            layer_stack_id: pinned.layer_stack.id,
            layer_stack_name: pinned.layer_stack.name.clone(),
            branch_name: pinned.branch.name.clone(),
        };
        let remote = RemoteWorkspace::start_local(&workspace).unwrap();
        workspace.remote = Some(remote.clone());
        let request = CreateWorkspaceSession {
            branch_id: branch,
            placement: WorkspacePlacement::Host {
                root: directory.join("mount"),
            },
            projection: Some(WorkspaceProjection::Fuse),
        };
        let worker = WorkspaceWorker::new(
            workspace_id,
            request,
            WorkspaceProjection::Fuse,
            identity,
            workspace,
        );
        // One changed byte in the live sandbox owner: the frozen generation
        // this Commit will publish and the retry must not silently drop.
        remote
            .edit(
                "file",
                vec![WorkspaceFileRangeEdit {
                    workspace_id,
                    path: "file".into(),
                    start: 0,
                    delete_len: 1,
                    replacement: WorkspaceFileReplacement::Inline(vec![0xa5]),
                }],
            )
            .unwrap();

        arm_verification_store_fault(branch, VerificationStoreFault::FinalPublication).unwrap();
        let construction = crate::changes::construction_gate();
        // The lifecycle entry opens the Commit timing scope; the read-metric
        // observation inside the route requires it.
        let timing = begin_workspace_commit(CaptureMode::Live).unwrap();
        let error = commit_remote(construction, &worker, &remote).unwrap_err();
        drop(timing);
        assert!(
            matches!(
                &error,
                WorkspaceError::Storage(StoreError::Integrity(message))
                    if *message == "injected qualified Workspace transaction failure"
            ),
            "unexpected faulted Commit outcome: {error:?}"
        );
        let receipt = take_verification_store_fault_receipt().unwrap();
        assert_eq!((receipt.hit_count, receipt.active), (1, true));
        {
            let workspace = worker.workspace.lock().unwrap();
            assert!(
                workspace.pending_stage.is_some(),
                "a failed publication retains the candidate stage"
            );
            assert!(
                workspace.pending_attempt.is_some(),
                "a failed publication retains its sandbox attempt"
            );
        }
        assert_eq!(
            store.branch(branch).unwrap().unwrap().head_commit_id,
            pinned.branch.head_commit_id,
            "the faulted Commit published nothing"
        );

        let timing = begin_workspace_commit(CaptureMode::Live).unwrap();
        let status = commit_remote(construction, &worker, &remote).unwrap();
        drop(timing);
        assert!(
            matches!(status.result, WorkspaceCommitResult::Created { .. }),
            "retry did not publish the retained generation: {:?}",
            status.result
        );
        assert!(!status.presentation_failed);
        {
            let workspace = worker.workspace.lock().unwrap();
            assert!(workspace.pending_stage.is_none());
            assert!(workspace.pending_attempt.is_none());
        }
        let head = store
            .branch(branch)
            .unwrap()
            .unwrap()
            .head_commit_id
            .unwrap();
        let record = store.commit(head).unwrap().unwrap();
        let mut bytes = Vec::new();
        layerfs_content::filesystem::read_range(
            &layerfs_layerstack_store::CoreReader(&store.snapshot_reader(record.root_id)),
            record.root_id,
            &layerfs_content::CanonicalPath::new("file").unwrap(),
            0..4,
            &mut bytes,
        )
        .unwrap();
        assert_eq!(
            bytes,
            vec![0xa5, 0, 0, 0],
            "the retry must publish the retained generation's bytes"
        );
        remote.server.control("shutdown").unwrap();
        drop(remote);
        drop(worker);
        drop(store);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
