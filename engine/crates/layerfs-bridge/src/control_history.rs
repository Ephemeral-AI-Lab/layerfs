//! Reuse the typed History vocabulary for bounded control results.
use crate::{
    control::{ControlError, WorkspaceToken},
    wire::{Reader, Writer},
};
use layerfs_content::ObjectId;
use layerfs_history::{
    error::MovedState, BranchId, BranchRecord, BranchSnapshot, CommitId, CommitMeta, CommitOutcome,
    CommitRecord, CommitVia, HistoryName, LayerId, LayerStackId, NodeId, WorkspaceId,
    MAXIMUM_MESSAGE_BYTES,
};
pub(crate) fn history<T>(value: layerfs_history::HistoryResult<T>) -> Result<T, ControlError> {
    value.map_err(|_| ControlError("history control value"))
}
pub(crate) fn object(input: &mut Reader<'_>) -> Result<ObjectId, ControlError> {
    ObjectId::from_bytes(&input.array::<32>()?).map_err(|_| ControlError("object identity"))
}
pub(crate) fn boolean(input: &mut Reader<'_>) -> Result<bool, ControlError> {
    match input.byte()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(ControlError("control boolean")),
    }
}
pub(crate) fn put_head(out: &mut Writer, value: Option<CommitId>) -> Result<(), ControlError> {
    out.byte(u8::from(value.is_some()))?;
    if let Some(id) = value {
        out.put(&id.to_bytes())?;
    }
    Ok(())
}
pub(crate) fn head(input: &mut Reader<'_>) -> Result<Option<CommitId>, ControlError> {
    if boolean(input)? {
        Ok(Some(history(CommitId::from_bytes(input.array()?))?))
    } else {
        Ok(None)
    }
}
pub(crate) fn put_token(out: &mut Writer, value: WorkspaceToken) -> Result<(), ControlError> {
    if value.namespace <= 0 {
        return Err(ControlError("namespace token"));
    }
    out.put(&value.workspace.to_bytes())?;
    out.put(&value.namespace.to_be_bytes())
}
pub(crate) fn token(input: &mut Reader<'_>) -> Result<WorkspaceToken, ControlError> {
    let workspace = history(WorkspaceId::from_authority(input.array()?))?;
    let namespace = i64::from_be_bytes(input.array()?);
    if namespace <= 0 {
        return Err(ControlError("namespace token"));
    }
    Ok(WorkspaceToken {
        workspace,
        namespace,
    })
}
pub(crate) fn put_cursor(out: &mut Writer, value: &Option<Vec<u8>>) -> Result<(), ControlError> {
    out.byte(u8::from(value.is_some()))?;
    if let Some(bytes) = value {
        if bytes.len() != layerfs_history::query::CURSOR_BYTES {
            return Err(ControlError("history cursor width"));
        }
        out.blob(bytes)?;
    }
    Ok(())
}
pub(crate) fn cursor(input: &mut Reader<'_>) -> Result<Option<Vec<u8>>, ControlError> {
    if !boolean(input)? {
        return Ok(None);
    }
    let bytes = input.blob(layerfs_history::query::CURSOR_BYTES)?;
    if bytes.len() != layerfs_history::query::CURSOR_BYTES {
        return Err(ControlError("history cursor width"));
    }
    Ok(Some(bytes.to_vec()))
}
pub(crate) fn put_branch(out: &mut Writer, value: &BranchRecord) -> Result<(), ControlError> {
    out.put(&value.id.to_bytes())?;
    out.put(&value.stack.to_bytes())?;
    out.blob(value.name.as_str().as_bytes())?;
    out.put(&value.base_layer.to_bytes())?;
    put_head(out, value.head_commit)?;
    put_node(out, value.forked_from)
}
pub(crate) fn branch(input: &mut Reader<'_>) -> Result<BranchRecord, ControlError> {
    Ok(BranchRecord {
        id: history(BranchId::from_bytes(input.array()?))?,
        stack: history(LayerStackId::from_bytes(input.array()?))?,
        name: history(HistoryName::new(&input.text(63)?))?,
        base_layer: history(LayerId::from_bytes(input.array()?))?,
        head_commit: head(input)?,
        forked_from: node(input)?,
    })
}
pub(crate) fn put_binding(out: &mut Writer, value: &BranchSnapshot) -> Result<(), ControlError> {
    let b = &value.branch;
    out.put(&b.id.to_bytes())?;
    out.put(&b.stack.to_bytes())?;
    out.blob(b.name.as_str().as_bytes())?;
    out.put(&b.base_layer.to_bytes())?;
    put_head(out, b.head_commit)?;
    put_node(out, b.forked_from)?;
    out.byte(u8::from(value.head_root.is_some()))?;
    if let Some(root) = value.head_root {
        out.put(&root.to_bytes())?;
    }
    for id in [
        value.base_root,
        value.effective_root,
        value.scope,
        value.profile,
    ] {
        out.put(&id.to_bytes())?;
    }
    Ok(())
}
pub(crate) fn binding(input: &mut Reader<'_>) -> Result<BranchSnapshot, ControlError> {
    let id = history(BranchId::from_bytes(input.array()?))?;
    let stack = history(LayerStackId::from_bytes(input.array()?))?;
    let name = history(HistoryName::new(&input.text(63)?))?;
    let base_layer = history(LayerId::from_bytes(input.array()?))?;
    let head_commit = head(input)?;
    let forked_from = node(input)?;
    let head_root = if boolean(input)? {
        Some(object(input)?)
    } else {
        None
    };
    Ok(BranchSnapshot {
        branch: BranchRecord {
            id,
            stack,
            name,
            base_layer,
            head_commit,
            forked_from,
        },
        head_root,
        base_root: object(input)?,
        effective_root: object(input)?,
        scope: object(input)?,
        profile: object(input)?,
    })
}
pub(crate) fn put_commit(out: &mut Writer, value: &CommitRecord) -> Result<(), ControlError> {
    out.put(&value.id.to_bytes())?;
    out.put(&value.stack.to_bytes())?;
    out.put(&value.root.to_bytes())?;
    put_head(out, value.parent)?;
    out.put(&value.base_layer.to_bytes())?;
    put_node(out, value.second_parent)?;
    put_meta(out, &value.meta)
}
/// Commit metadata: Branch, via, agent, call, merge origin and time.
pub(crate) fn put_meta(out: &mut Writer, value: &CommitMeta) -> Result<(), ControlError> {
    out.byte(u8::from(value.branch.is_some()))?;
    if let Some(branch) = value.branch {
        out.put(&branch.to_bytes())?;
    }
    out.byte(value.via.code())?;
    out.byte(u8::from(value.message.is_some()))?;
    if let Some(text) = &value.message {
        if text.len() > MAXIMUM_MESSAGE_BYTES {
            return Err(ControlError("Commit message"));
        }
        out.blob(text.as_bytes())?;
    }
    put_node(out, value.merged_from)?;
    out.put(&value.time_ms.to_be_bytes())?;
    for count in [
        value.statistics.logical,
        value.statistics.new_canonical,
        value.statistics.new_stored,
        value.statistics.new_metadata,
    ] {
        out.put(&count.to_be_bytes())?;
    }
    put_head(out, value.displaced)?;
    // Full decisions remain in the Store and the separate merge report. A
    // bounded typed history record discloses their omission explicitly.
    out.byte(u8::from(value.lww_truncated || !value.lww.is_empty()))
}
pub(crate) fn meta(input: &mut Reader<'_>) -> Result<CommitMeta, ControlError> {
    let branch = if boolean(input)? {
        Some(history(BranchId::from_bytes(input.array()?))?)
    } else {
        None
    };
    let via = history(CommitVia::from_code(input.byte()?))?;
    let mut text = || -> Result<Option<String>, ControlError> {
        if boolean(input)? {
            Ok(Some(input.text(MAXIMUM_MESSAGE_BYTES)?))
        } else {
            Ok(None)
        }
    };
    let message = text()?;
    Ok(CommitMeta {
        branch,
        via,
        message,
        merged_from: node(input)?,
        time_ms: i64::from_be_bytes(input.array()?),
        statistics: layerfs_history::CommitStatistics {
            logical: u64::from_be_bytes(input.array()?),
            new_canonical: u64::from_be_bytes(input.array()?),
            new_stored: u64::from_be_bytes(input.array()?),
            new_metadata: u64::from_be_bytes(input.array()?),
        },
        displaced: head(input)?,
        lww: Vec::new(),
        lww_truncated: boolean(input)?,
        // Like the decisions, the unit list stays in the Store.
        units: Vec::new(),
    })
}
pub(crate) fn put_node(out: &mut Writer, value: Option<NodeId>) -> Result<(), ControlError> {
    out.byte(u8::from(value.is_some()))?;
    if let Some(id) = value {
        out.put(id.as_slice())?;
    }
    Ok(())
}
pub(crate) fn node(input: &mut Reader<'_>) -> Result<Option<NodeId>, ControlError> {
    if boolean(input)? {
        Ok(Some(history(NodeId::from_slice(&input.array::<33>()?))?))
    } else {
        Ok(None)
    }
}
pub(crate) fn commit(input: &mut Reader<'_>) -> Result<CommitRecord, ControlError> {
    Ok(CommitRecord {
        id: history(CommitId::from_bytes(input.array()?))?,
        stack: history(LayerStackId::from_bytes(input.array()?))?,
        root: object(input)?,
        parent: head(input)?,
        base_layer: history(LayerId::from_bytes(input.array()?))?,
        second_parent: node(input)?,
        meta: meta(input)?,
    })
}
pub(crate) fn put_outcome(out: &mut Writer, value: &CommitOutcome) -> Result<(), ControlError> {
    match value {
        CommitOutcome::Committed { record, displaced } => {
            out.byte(1)?;
            put_commit(out, record)?;
            put_head(out, *displaced)
        }
        CommitOutcome::UpToDate { head, root } => {
            out.byte(2)?;
            put_head(out, *head)?;
            out.put(&root.to_bytes())
        }
    }
}
pub(crate) fn outcome(input: &mut Reader<'_>) -> Result<CommitOutcome, ControlError> {
    match input.byte()? {
        1 => Ok(CommitOutcome::Committed {
            record: Box::new(commit(input)?),
            displaced: head(input)?,
        }),
        2 => Ok(CommitOutcome::UpToDate {
            head: head(input)?,
            root: object(input)?,
        }),
        _ => Err(ControlError("Commit outcome")),
    }
}
pub(crate) fn put_publication(
    out: &mut Writer,
    value: &Option<CommitOutcome>,
) -> Result<(), ControlError> {
    out.byte(u8::from(value.is_some()))?;
    if let Some(value) = value {
        put_outcome(out, value)?;
    }
    Ok(())
}
pub(crate) fn publication(input: &mut Reader<'_>) -> Result<Option<CommitOutcome>, ControlError> {
    if boolean(input)? {
        Ok(Some(outcome(input)?))
    } else {
        Ok(None)
    }
}
pub(crate) fn put_moved(out: &mut Writer, value: &MovedState) -> Result<(), ControlError> {
    put_head(out, value.expected_head)?;
    put_head(out, value.actual_head)?;
    out.put(&value.expected_base.to_bytes())?;
    out.put(&value.actual_base.to_bytes())
}
pub(crate) fn moved(input: &mut Reader<'_>) -> Result<MovedState, ControlError> {
    Ok(MovedState {
        expected_head: head(input)?,
        actual_head: head(input)?,
        expected_base: history(LayerId::from_bytes(input.array()?))?,
        actual_base: history(LayerId::from_bytes(input.array()?))?,
    })
}
