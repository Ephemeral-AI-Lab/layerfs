//! Complete typed deciding-transaction records, without a later history reread.
use super::reader::Reader;
use layerfs_bridge::contract::{FrameError, FrameResult};
use layerfs_history::{
    BranchId, BranchRecord, BranchSnapshot, CatalogId, CommitId, CommitRecord, CommitStagedOutcome,
    HistoryName, LayerId, LayerStackId, StageRecord, StageToken, WorkspaceId,
};
use layerfs_storage::save::{PoolCounters, WriteOutcome};
pub(crate) fn stage(r: &mut Reader<'_>) -> FrameResult<StageRecord> {
    let stage = StageRecord {
        workspace: WorkspaceId::from_slice(r.take(32)?)
            .map_err(|_| FrameError::Invalid("stage Workspace"))?,
        token: StageToken::new(r.u64()?).map_err(|_| FrameError::Invalid("stage token"))?,
        stack: LayerStackId::from_slice(r.take(17)?)
            .map_err(|_| FrameError::Invalid("stage stack"))?,
        branch: BranchId::from_slice(r.take(17)?)
            .map_err(|_| FrameError::Invalid("stage Branch"))?,
        expected_head: head(r)?,
        expected_base: layer(r)?,
        expected_root: r.object()?,
        construction_base_root: r.object()?,
        intended_commit_base: layer(r)?,
        candidate_root: r.object()?,
        profile: r.object()?,
        scope: r.object()?,
        generation: r.u64()?,
    };
    if stage.generation > i64::MAX as u64 {
        return Err(FrameError::Invalid("stage generation"));
    }
    Ok(stage)
}
/// Decodes the complete StageRecord carried as a remote failure field.
pub fn decode_stage(bytes: &[u8]) -> FrameResult<StageRecord> {
    let mut r = Reader::new(bytes);
    let stage = stage(&mut r)?;
    r.end()?;
    if stage.generation > i64::MAX as u64 {
        return Err(FrameError::Invalid("stage generation"));
    }
    Ok(stage)
}
fn layer(r: &mut Reader<'_>) -> FrameResult<LayerId> {
    LayerId::from_slice(r.take(33)?).map_err(|_| FrameError::Invalid("reply Layer"))
}
fn head(r: &mut Reader<'_>) -> FrameResult<Option<CommitId>> {
    if r.flag()? {
        Ok(Some(
            CommitId::from_slice(r.take(33)?).map_err(|_| FrameError::Invalid("reply Commit"))?,
        ))
    } else {
        Ok(None)
    }
}
/// Captured host binding data. This is not a locally constructible Binding
/// capability; the authenticated host retains authority and original expectations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteBinding {
    /// Exact authority-assigned runtime incarnation.
    pub runtime: [u8; 32],
    /// Authenticated client static identity authorized by the host.
    pub peer: [u8; 32],
    /// Exact Workspace incarnation.
    pub workspace: WorkspaceId,
    /// Authority-bound catalog identity.
    pub catalog: CatalogId,
    /// Owning provider continuity incarnation.
    pub incarnation: u64,
    /// Checked original root inode serial.
    pub root_serial: u64,
    /// Coherent original Branch expectations, never refreshed by this reply.
    pub snapshot: BranchSnapshot,
}
pub(crate) fn binding(r: &mut Reader<'_>) -> FrameResult<RemoteBinding> {
    let runtime = r.take(32)?.try_into().expect("fixed runtime identity");
    let peer = r.take(32)?.try_into().expect("fixed peer identity");
    let workspace = WorkspaceId::from_slice(r.take(32)?)
        .map_err(|_| FrameError::Invalid("binding Workspace"))?;
    let catalog = CatalogId::from_bytes(r.take(32)?.try_into().expect("fixed catalog"));
    let incarnation = r.u64()?;
    let root_serial = r.u64()?;
    let id =
        BranchId::from_slice(r.take(17)?).map_err(|_| FrameError::Invalid("binding Branch"))?;
    let stack =
        LayerStackId::from_slice(r.take(17)?).map_err(|_| FrameError::Invalid("binding stack"))?;
    let name = r.blob()?;
    if name.len() > 63 {
        return Err(FrameError::Invalid("binding name window"));
    }
    let name = HistoryName::new(
        std::str::from_utf8(name).map_err(|_| FrameError::Invalid("binding name UTF-8"))?,
    )
    .map_err(|_| FrameError::Invalid("binding name grammar"))?;
    let base_layer = layer(r)?;
    let head_commit = head(r)?;
    let head_root = if r.flag()? { Some(r.object()?) } else { None };
    let base_root = r.object()?;
    let effective_root = r.object()?;
    let scope = r.object()?;
    let profile = r.object()?;
    if runtime == [0; 32]
        || peer == [0; 32]
        || incarnation == 0
        || root_serial == 0
        || root_serial > i64::MAX as u64
        || head_commit.is_some() != head_root.is_some()
        || effective_root != head_root.unwrap_or(base_root)
    {
        return Err(FrameError::Invalid("binding coherent snapshot"));
    }
    Ok(RemoteBinding {
        runtime,
        peer,
        workspace,
        catalog,
        incarnation,
        root_serial,
        snapshot: BranchSnapshot {
            branch: BranchRecord {
                id,
                stack,
                name,
                base_layer,
                head_commit,
            },
            head_root,
            base_root,
            effective_root,
            scope,
            profile,
        },
    })
}
pub(crate) fn commit(r: &mut Reader<'_>) -> FrameResult<CommitStagedOutcome> {
    Ok(match r.byte()? {
        1 => {
            let record = CommitRecord {
                id: CommitId::from_slice(r.take(33)?)
                    .map_err(|_| FrameError::Invalid("reply Commit ID"))?,
                stack: LayerStackId::from_slice(r.take(17)?)
                    .map_err(|_| FrameError::Invalid("reply Commit stack"))?,
                root: r.object()?,
                parent: head(r)?,
                base_layer: layer(r)?,
            };
            if record.id != CommitId::derive(record.root, record.parent, record.base_layer) {
                return Err(FrameError::Invalid("reply Commit identity"));
            }
            CommitStagedOutcome::Committed(record)
        }
        2 => CommitStagedOutcome::UpToDate {
            head: head(r)?,
            root: r.object()?,
        },
        _ => return Err(FrameError::Invalid("commit result kind")),
    })
}
pub(crate) fn write(r: &mut Reader<'_>) -> FrameResult<WriteOutcome> {
    Ok(WriteOutcome {
        inserted: r.u64()?,
        reused: r.u64()?,
        full_records: r.u64()?,
        prefix_records: r.u64()?,
        packs: r.u64()?,
        canonical_bytes: r.u64()?,
        pool: PoolCounters {
            leaves: r.u64()?,
            reused_values: r.u64()?,
            new_values: r.u64()?,
            groups: r.u64()?,
            delta_leaves: r.u64()?,
            full_leaves: r.u64()?,
            trials: r.u64()?,
            work_exceeded: r.u64()?,
        },
    })
}
