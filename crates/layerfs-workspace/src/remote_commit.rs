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
use crate::snapshot_input::{CaptureSummary, CompletionRecord, FrozenRemoteInput, SnapshotToken};
use crate::session::{WorkspaceCommitResult, WorkspaceCommitStatus};
use crate::{Workspace, WorkspaceError, WorkspaceResult};
use layerfs_content::ObjectId;
use layerfs_layerstack_store::{
    CommitOutcome, CommitId, CoreReader, LayerId, ObjectBuffer, StoreError,
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

/// Resolve a prior undelivered completion, then capture, pull, build,
/// publish and complete one generation. Called under the workspace lifecycle
/// lock (one active committer, at most one queued).
pub(crate) fn commit_remote(
    construction: &std::sync::Mutex<()>,
    worker: &crate::worker::WorkspaceWorker,
    remote: &RemoteWorkspace,
) -> WorkspaceResult<WorkspaceCommitStatus> {
    {
        let workspace = worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        if workspace.presentation_failed {
            return Err(WorkspaceError::InvalidExecution);
        }
        // A retained stage freezes further mutation until explicit discard,
        // exactly as on the materialized route.
        if workspace.pending_stage.is_some() {
            return Err(WorkspaceError::Storage(StoreError::InvalidInput(
                "workspace stage retained",
            )));
        }
        // Resolve a prior publication's undelivered completion first: its
        // outcome is authoritative and the exact attempt must finish before
        // anything new can be captured.
        if workspace.pending_completion.is_some() {
            drop(workspace);
            resolve_pending_completion(worker, remote)?;
        }
    }

    let commit_read_before = worker
        .workspace
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?
        .reader
        .read_metrics_snapshot()?;

    // Capture the frozen generation (short, sandbox-local).
    let started = Instant::now();
    let summary = remote.capture()?;
    layerfs_layerstack_store::note_workspace_commit_phase(
        layerfs_layerstack_store::WorkspaceCommitPhase::Capture,
        elapsed_ns(started),
    );

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
                layerfs_workspace_core::Data::File(
                    layerfs_workspace_core::FileData::Edited { spool_high_water, .. },
                ) => Some(*spool_high_water),
                _ => None,
            })
            .sum();
        Workspace::note_edit_state_over(&input.nodes, &input.dirty, spool, spool)?;
    }

    // An empty frontier publishes an empty candidate (UpToDate/HeadMoved
    // detection) and releases the attempt without completion.
    if summary.frontier_len == 0 {
        let outcome = publish_empty(worker)?;
        let result = match outcome {
            Outcome::Published(outcome) | Outcome::WithBase(outcome, _) => {
                let _ = remote.cancel_snapshot(summary.token);
                result_from_outcome(outcome, worker)?
            }
            Outcome::Unpublished(result) => {
                let _ = remote.cancel_snapshot(summary.token);
                return Ok(WorkspaceCommitStatus {
                    result,
                    presentation_failed: false,
                });
            }
        };
        return Ok(WorkspaceCommitStatus {
            result,
            presentation_failed: false,
        });
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
            let _ = remote.cancel_snapshot(summary.token);
            return Ok(WorkspaceCommitStatus {
                result,
                presentation_failed: false,
            });
        }
        Outcome::Published(outcome) => {
            let _ = remote.cancel_snapshot(summary.token);
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

    // Completion records: the builder's checkpoint journal plus each node's
    // captured revision. Delivery is idempotent and token-guarded.
    let started = Instant::now();
    let records = completion_records(worker, &input)?;
    let (root, head) = match &outcome {
        CommitOutcome::Committed { commit_id, root_id, .. } => {
            (*root_id, Some(commit_id.to_bytes()))
        }
        CommitOutcome::UpToDate { root_id } => {
            let expected = worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?
                .expected_head;
            (*root_id, expected.map(|head| head.to_bytes()))
        }
    };
    let delivered = remote.complete_generation(summary.token, root, head, &records);
    layerfs_layerstack_store::note_workspace_commit_phase(
        layerfs_layerstack_store::WorkspaceCommitPhase::Checkpoint,
        elapsed_ns(started),
    );
    let completion_delivered = delivered.is_ok();

    // Re-base the host workspace onto the published root.
    {
        let mut workspace = worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        rebase_host_workspace(&mut workspace, root, head, expected_base)?;
        if !completion_delivered {
            // The publication is authoritative; its exact completion is
            // retained and re-delivered by the next Commit or End.
            workspace.pending_completion = Some(PendingCompletion {
                token: summary.token,
                root,
                head,
                expected_base,
                records,
            });
        }
    }

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

    let result = result_from_outcome(outcome, worker)?;
    Ok(WorkspaceCommitStatus {
        result,
        presentation_failed: !completion_delivered || observations.is_err(),
    })
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
    rebase_host_workspace(&mut workspace, pending.root, pending.head, pending.expected_base)?;
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

fn publish_empty(worker: &crate::worker::WorkspaceWorker) -> WorkspaceResult<Outcome> {
    let workspace = worker
        .workspace
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?;
    let empty = ObjectBuffer::new(&workspace.reader)
        .and_then(|buffer| buffer.finish(workspace.base_root, 0))?;
    let admission = workspace.store.workspace_admission(workspace.workspace_id)?;
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
        .ok_or_else(|| StoreError::Integrity("completion revision"))?
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
