//! Conversion between service wire records and catalog values.
use crate::error::catalog as failure;
use crate::read::content::id;
use layerfs_bridge::contract::*;
use layerfs_bridge::contract::{HistoryResult, ManifestEntry};
use layerfs_history::*;

pub(crate) fn changes_kind(kind: u8) -> Result<u8, Failure> {
    match kind {
        1..=3 => Ok(kind),
        _ => Err(Code::InvalidInput.into()),
    }
}

pub(crate) fn commit_outcome(
    outcome: CommitStagedOutcome,
) -> layerfs_bridge::contract::HistoryResult {
    HistoryResult::Committed(match outcome {
        CommitStagedOutcome::Committed(record) => {
            CommitOutcomeWire::Committed(commit_wire(&record))
        }
        CommitStagedOutcome::UpToDate { head, root } => CommitOutcomeWire::UpToDate {
            head: head.map(CommitId::to_bytes),
            root: *root.as_bytes(),
        },
    })
}

pub(crate) fn cursor_option(cursor: &[u8]) -> Option<Vec<u8>> {
    (!cursor.is_empty()).then(|| cursor.to_vec())
}

pub(crate) fn name_of(name: &[u8]) -> Result<HistoryName, Failure> {
    let text = std::str::from_utf8(name).map_err(|_| Code::InvalidInput)?;
    HistoryName::new(text).map_err(failure)
}

pub(crate) fn workspace_id(bytes: &[u8; 32]) -> Result<WorkspaceId, Failure> {
    WorkspaceId::from_slice(bytes).map_err(failure)
}

pub(crate) fn stack_id(bytes: &[u8; 17]) -> Result<LayerStackId, Failure> {
    LayerStackId::from_slice(bytes).map_err(failure)
}

pub(crate) fn branch_id(bytes: &[u8; 17]) -> Result<BranchId, Failure> {
    BranchId::from_slice(bytes).map_err(failure)
}

pub(crate) fn commit_id(bytes: &[u8; 33]) -> Result<CommitId, Failure> {
    CommitId::from_slice(bytes).map_err(failure)
}

pub(crate) fn layer_id(bytes: &[u8; 33]) -> Result<LayerId, Failure> {
    LayerId::from_slice(bytes).map_err(failure)
}

pub(crate) fn optional_commit(bytes: &Option<[u8; 33]>) -> Result<Option<CommitId>, Failure> {
    bytes.as_ref().map(commit_id).transpose()
}

pub(crate) fn optional_layer(bytes: &Option<[u8; 33]>) -> Result<Option<LayerId>, Failure> {
    bytes.as_ref().map(layer_id).transpose()
}

pub(crate) fn manifest_of(entries: &[ManifestEntry]) -> Result<NamespaceManifest, Failure> {
    let entries = entries
        .iter()
        .map(|entry| {
            Ok(layerfs_history::ManifestEntry {
                parent: entry.parent,
                name: entry.name.clone(),
                kind: layerfs_history::RecordKind::from_code(entry.kind)
                    .map_err(|_| Code::InvalidInput)?,
                mode: entry.mode,
                mtime_seconds: entry.mtime_seconds,
                mtime_nanoseconds: entry.mtime_nanoseconds,
                content: entry.content.map(|root| id(&root)),
                target: (!entry.target.is_empty()).then(|| entry.target.clone()),
            })
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    Ok(NamespaceManifest { entries })
}

pub(crate) fn stack_wire(record: &LayerStackRecord) -> StackWire {
    StackWire {
        stack: record.id.to_bytes(),
        name: record.name.as_str().as_bytes().to_vec(),
        scope: *record.scope.as_bytes(),
        profile: *record.profile.as_bytes(),
        head_layer: record.head_layer.to_bytes(),
    }
}

pub(crate) fn branch_wire(record: &BranchRecord) -> BranchWire {
    BranchWire {
        branch: record.id.to_bytes(),
        stack: record.stack.to_bytes(),
        name: record.name.as_str().as_bytes().to_vec(),
        base_layer: record.base_layer.to_bytes(),
        head_commit: record.head_commit.map(CommitId::to_bytes),
    }
}

pub(crate) fn snapshot_wire(snapshot: &BranchSnapshot) -> BranchSnapshotWire {
    BranchSnapshotWire {
        branch: branch_wire(&snapshot.branch),
        head_root: snapshot.head_root.map(|root| *root.as_bytes()),
        base_root: *snapshot.base_root.as_bytes(),
        effective_root: *snapshot.effective_root.as_bytes(),
        root_serial: None,
        scope: *snapshot.scope.as_bytes(),
        profile: *snapshot.profile.as_bytes(),
    }
}

pub(crate) fn commit_wire(record: &CommitRecord) -> CommitWire {
    CommitWire {
        commit: record.id.to_bytes(),
        stack: record.stack.to_bytes(),
        root: *record.root.as_bytes(),
        parent: record.parent.map(CommitId::to_bytes),
        base_layer: record.base_layer.to_bytes(),
    }
}

pub(crate) fn layer_wire(record: &LayerRecord) -> LayerWire {
    LayerWire {
        layer: record.id.to_bytes(),
        stack: record.stack.to_bytes(),
        parent: record.parent.map(LayerId::to_bytes),
        root: *record.root.as_bytes(),
        source_branch: record.source_branch.map(BranchId::to_bytes),
        source_commit: record.source_commit.map(CommitId::to_bytes),
    }
}

pub(crate) fn stage_wire(record: &StageRecord) -> StageWire {
    StageWire {
        workspace: record.workspace.to_bytes(),
        token: record.token.value(),
        stack: record.stack.to_bytes(),
        branch: record.branch.to_bytes(),
        expected_head: record.expected_head.map(CommitId::to_bytes),
        expected_base: record.expected_base.to_bytes(),
        expected_root: *record.expected_root.as_bytes(),
        construction_base_root: *record.construction_base_root.as_bytes(),
        intended_commit_base: record.intended_commit_base.to_bytes(),
        candidate_root: *record.candidate_root.as_bytes(),
        profile: *record.profile.as_bytes(),
        scope: *record.scope.as_bytes(),
        generation: record.generation,
    }
}
