//! Commit insertion, atomic Branch overwrite, ancestry and history pages.
//!
//! One transaction checks the expected head and captured provenance, inserts
//! or verifies the immutable Commit and advances the Branch. A moved head is
//! refused unless the request forces an overwrite; a forced candidate keeps its
//! captured parent and reports the head it displaced. There is no stage row.

use super::params;
use super::transaction::Transaction;
use layerfs_content::ObjectId;
use layerfs_history::error::{HistoryError, HistoryResult, Missing, MovedState};
use layerfs_history::identity::{BranchId, CatalogId, CommitId, LayerId, LayerStackId, NodeId};
use layerfs_history::records::{
    CommitHistoryRequest, CommitMeta, CommitOutcome, CommitRecord, CommitRequest, PageResult,
};

use super::query::{self, Context, Cursor, Range};
use super::rows::{commit_row, one, sql};

const COMMIT_BY_ID: &str = "commit_commit_by_id";

const INSERT_COMMIT: &str = "commit_insert_commit";

const ADVANCE_BRANCH: &str = "commit_advance_branch";

/// Reads one Commit by identity.
pub(crate) fn commit(tx: &Transaction<'_>, id: CommitId) -> HistoryResult<Option<CommitRecord>> {
    one(tx, COMMIT_BY_ID, [id.as_slice()], commit_row)
}

/// True when an existing row for this identity carries the same immutable fields.
pub(crate) fn verify_commit(tx: &Transaction<'_>, record: &CommitRecord) -> HistoryResult<bool> {
    match commit(tx, record.id)? {
        None => Ok(false),
        Some(stored) => {
            if stored.root == record.root
                && stored.parent == record.parent
                && stored.base_layer == record.base_layer
                && stored.second_parent == record.second_parent
                && stored.stack == record.stack
                && (CommitMeta {
                    time_ms: record.meta.time_ms,
                    ..stored.meta
                }) == record.meta
            {
                Ok(true)
            } else {
                Err(HistoryError::Integrity("Commit identity"))
            }
        }
    }
}

/// True when `target` is on `head`'s first-parent chain.
///
/// A run is a first-parent path, so `target` is on the chain when it is in
/// one of the runs the chain passes through, no deeper than where the chain
/// leaves that run. Work follows the runs (the forks from Commits on the
/// chain), not its length.
pub(crate) fn ancestry_contains(
    tx: &Transaction<'_>,
    head: Option<CommitId>,
    target: CommitId,
) -> HistoryResult<bool> {
    let Some(head) = head else {
        return Ok(false);
    };
    let target = super::lineage::commit_line(tx, target)?;
    let mut current = super::lineage::commit_line(tx, head)?;
    loop {
        if current.depth < target.depth {
            return Ok(false);
        }
        if current.run == target.run {
            return Ok(true);
        }
        match super::lineage::commit_line(tx, current.run)?.parent {
            Some(parent) => current = super::lineage::commit_line(tx, parent)?,
            None => return Ok(false),
        }
    }
}

/// Publishes one candidate against the expected Branch head.
pub(crate) fn commit_branch(
    tx: &Transaction<'_>,
    request: &CommitRequest,
) -> HistoryResult<CommitOutcome> {
    let branch =
        super::branch::branch(tx, request.branch)?.ok_or(HistoryError::Missing(Missing::Branch))?;
    let stack = super::layerstack::layer_stack(tx, branch.stack)?
        .ok_or(HistoryError::Integrity("LayerStack"))?;
    if stack.profile != request.profile || stack.scope != request.scope {
        return Err(HistoryError::Integrity("Commit context"));
    }
    let moved =
        branch.base_layer != request.expected_base || branch.head_commit != request.expected_head;
    if moved && (!request.force || branch.base_layer != request.expected_base) {
        return Err(HistoryError::HeadMoved(Box::new(MovedState {
            expected_head: request.expected_head,
            actual_head: branch.head_commit,
            expected_base: request.expected_base,
            actual_base: branch.base_layer,
        })));
    }
    let base_root = super::layerstack::layer_root(tx, request.expected_base)?;
    let root_of = |head: Option<CommitId>| -> HistoryResult<_> {
        match head {
            Some(head) => {
                let record = commit(tx, head)?.ok_or(HistoryError::Integrity("Branch Commit"))?;
                if record.stack != branch.stack || record.base_layer != branch.base_layer {
                    return Err(HistoryError::Integrity("Branch Commit ownership"));
                }
                Ok(record.root)
            }
            None => Ok(base_root),
        }
    };
    if root_of(request.expected_head)? != request.expected_root {
        return Err(HistoryError::Integrity("Commit context"));
    }
    let current_root = root_of(branch.head_commit)?;
    if request.candidate_root == current_root {
        return Ok(CommitOutcome::UpToDate {
            head: branch.head_commit,
            root: current_root,
        });
    }
    let record = new_commit(
        tx,
        branch.stack,
        request.branch,
        request.candidate_root,
        request.expected_head,
        request.expected_base,
        request.second_parent,
        &CommitMeta {
            displaced: branch.head_commit.filter(|_| moved),
            ..request.meta.clone()
        },
    )?;
    let advanced = tx
        .execute(
            ADVANCE_BRANCH,
            params![
                record.id.as_slice(),
                branch.id.as_slice(),
                request.expected_base.as_slice(),
            ],
        )
        .map_err(sql)?;
    if advanced != 1 {
        return Err(HistoryError::Integrity("Branch advance"));
    }
    let displaced = branch.head_commit.filter(|_| moved);
    Ok(CommitOutcome::Committed {
        record: Box::new(record),
        displaced,
    })
}

/// Derives, checks and inserts one Commit made on `branch` now, or verifies
/// the identical record that already exists.
#[allow(clippy::too_many_arguments)]
pub(crate) fn new_commit(
    tx: &Transaction<'_>,
    stack: LayerStackId,
    branch: BranchId,
    root: ObjectId,
    parent: Option<CommitId>,
    base_layer: LayerId,
    second_parent: Option<NodeId>,
    meta: &CommitMeta,
) -> HistoryResult<CommitRecord> {
    meta.check()?;
    let meta = CommitMeta {
        message: Some(
            meta.message
                .clone()
                .unwrap_or_else(|| format!("{:?} filesystem change", meta.via)),
        ),
        branch: Some(branch),
        time_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| {
                i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
            }),
        ..meta.clone()
    };
    let record = CommitRecord {
        id: CommitId::derive(root, parent, base_layer, second_parent, &meta),
        stack,
        root,
        parent,
        base_layer,
        second_parent,
        meta,
    };
    if !verify_commit(tx, &record)? {
        tx.execute(
            INSERT_COMMIT,
            params![
                record.id.as_slice(),
                record.stack.as_slice(),
                record.root.as_bytes().as_slice(),
                record.parent.map(|id| id.to_bytes().to_vec()),
                record.base_layer.as_slice(),
                record.second_parent.map(|id| id.as_slice().to_vec()),
                record.meta.branch.map(|id| id.to_bytes().to_vec()),
                i64::from(record.meta.via.code()),
                record.meta.message.clone(),
                record.meta.merged_from.map(|id| id.as_slice().to_vec()),
                record.meta.time_ms,
                i64::try_from(record.meta.statistics.logical)
                    .map_err(|_| HistoryError::Capacity("logical bytes"))?,
                i64::try_from(record.meta.statistics.new_canonical)
                    .map_err(|_| HistoryError::Capacity("canonical bytes"))?,
                i64::try_from(record.meta.statistics.new_stored)
                    .map_err(|_| HistoryError::Capacity("stored bytes"))?,
                i64::try_from(record.meta.statistics.new_metadata)
                    .map_err(|_| HistoryError::Capacity("metadata bytes"))?,
                super::facts::encode(&record.meta.lww, &record.meta.units)?,
                record.meta.displaced.map(|id| id.to_bytes().to_vec()),
            ],
        )
        .map_err(sql)?;
        super::lineage::record_commit(tx, &record)?;
        return Ok(record);
    }
    // An identical Commit already exists: it keeps its own recorded time.
    commit(tx, record.id)?.ok_or(HistoryError::Integrity("Commit identity"))
}

/// Walks one Branch's ancestry from an immutable start, newest first.
pub(crate) fn commit_history(
    tx: &Transaction<'_>,
    catalog: CatalogId,
    incarnation: u64,
    key: &[u8; 32],
    request: &CommitHistoryRequest,
) -> HistoryResult<PageResult<CommitRecord>> {
    let branch =
        super::branch::branch(tx, request.branch)?.ok_or(HistoryError::Missing(Missing::Branch))?;
    let capacity = query::capacity(request.limit, CommitRecord::MAXIMUM_ENCODED_BYTES)?;
    let context = Context {
        key,
        catalog,
        incarnation,
        range: Range::CommitAncestry,
        subject: request.branch.as_slice(),
    };
    let cursor = request
        .cursor
        .as_ref()
        .map(|bytes| query::decode(context, bytes))
        .transpose()?;
    let (start, mut current) = if let Some(cursor) = cursor {
        let anchor = CommitId::from_slice(&cursor.anchor)?;
        if request.start.is_some_and(|start| start != anchor) {
            return Err(HistoryError::InvalidInput("page cursor anchor"));
        }
        let position = CommitId::from_slice(&cursor.position)?;
        let anchor_record = commit(tx, anchor)?.ok_or(HistoryError::Integrity("Commit anchor"))?;
        let last = commit(tx, position)?.ok_or(HistoryError::Integrity("Commit chain"))?;
        // The keyed cursor attests that traversal from this anchor delivered
        // this position. Rewalking it would impose a lifetime listing ceiling.
        if anchor_record.stack != branch.stack || last.stack != branch.stack {
            return Err(HistoryError::Integrity("page cursor ancestry"));
        }
        (Some(anchor), last.parent)
    } else {
        let start = request.start.or(branch.head_commit);
        if let Some(start) = start {
            let record = commit(tx, start)?.ok_or(HistoryError::Missing(Missing::Commit))?;
            if record.stack != branch.stack {
                return Err(HistoryError::Integrity("Commit ownership"));
            }
            if request.start.is_some() && !ancestry_contains(tx, branch.head_commit, start)? {
                return Err(HistoryError::NotInHistory("Commit"));
            }
        }
        (start, start)
    };
    let Some(start) = start else {
        return Ok(PageResult {
            records: Vec::new(),
            continuation: None,
        });
    };
    let mut records = Vec::new();
    while let Some(id) = current {
        if records.len() > capacity {
            break;
        }
        let record = commit(tx, id)?.ok_or(HistoryError::Integrity("Commit chain"))?;
        if record.stack != branch.stack {
            return Err(HistoryError::Integrity("Commit ownership"));
        }
        current = record.parent;
        records.push(record);
    }
    let more = records.len() > capacity;
    query::page(records, capacity, more, |record| {
        query::encode(
            context,
            &Cursor {
                anchor: start.as_slice().to_vec(),
                position: record.id.as_slice().to_vec(),
            },
        )
        .map(Some)
    })
}
