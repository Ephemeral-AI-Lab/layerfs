//! Commit insertion, conditional Branch advance, ancestry and history pages.
//!
//! One Commit transition is exactly one transaction: validate the frozen
//! expectations, insert or verify the immutable Commit, compare-and-swap the
//! Branch head, delete the exact stage, commit. Only one state-changing Commit
//! can win against a fixed expected head and base, and the loser keeps its stage
//! so the caller can decide what to do with it.

use crate::error::{HistoryError, HistoryResult, Missing, MovedState};
use crate::identity::{CatalogId, CommitId};
use crate::records::{
    CommitHistoryRequest, CommitRecord, CommitStagedOutcome, CommitStagedRequest, PageResult,
    MAXIMUM_LINEAGE_ROWS,
};
use rusqlite::Transaction;

use super::query::{self, Context, Cursor, Range};
use super::rows::{cell, commit_row, one, sql};

const COMMIT_BY_ID: &str = "SELECT commit_id, layer_stack_id, root_id, parent_commit_id, \
     base_layer_id FROM commits WHERE commit_id = ?1";

const INSERT_COMMIT: &str = "INSERT INTO commits \
     (commit_id, layer_stack_id, root_id, parent_commit_id, base_layer_id) \
     VALUES (?1, ?2, ?3, ?4, ?5)";

const ADVANCE_BRANCH: &str = "UPDATE branches SET head_commit_id = ?1 \
     WHERE branch_id = ?2 AND base_layer_id = ?3 AND head_commit_id IS ?4";

const BRANCH_HEAD: &str = "SELECT head_commit_id, base_layer_id FROM branches WHERE branch_id = ?1";

const DELETE_STAGE: &str = "DELETE FROM workspace_stages \
     WHERE workspace_id = ?1 AND stage_token = ?2";

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
                && stored.stack == record.stack
            {
                Ok(true)
            } else {
                Err(HistoryError::Integrity("Commit identity"))
            }
        }
    }
}

/// True when `target` is reachable from `head` through recorded ancestry.
///
/// The walk is bounded by [`MAXIMUM_LINEAGE_ROWS`]. Exhausting the bound is
/// `Capacity`, never a negative answer: an over-budget walk is unproven.
pub(crate) fn ancestry_contains(
    tx: &Transaction<'_>,
    head: Option<CommitId>,
    target: CommitId,
) -> HistoryResult<bool> {
    let mut current = head;
    let mut examined = 0u64;
    while let Some(id) = current {
        examined += 1;
        if examined > MAXIMUM_LINEAGE_ROWS {
            return Err(HistoryError::Capacity("lineage work"));
        }
        if id == target {
            return Ok(true);
        }
        let record = commit(tx, id)?.ok_or(HistoryError::Integrity("Commit chain"))?;
        current = record.parent;
    }
    Ok(false)
}

/// Commits one exact stage against its frozen expectations.
pub(crate) fn commit_staged(
    tx: &Transaction<'_>,
    request: &CommitStagedRequest,
    observed: &mut Option<Option<crate::records::StageRecord>>,
) -> HistoryResult<CommitStagedOutcome> {
    let stage = super::staging::stage(tx, request.workspace)?;
    *observed = Some(stage.clone());
    let stage = stage.ok_or(HistoryError::StageChanged {
        expected: request.token,
        actual: None,
    })?;
    if stage.token != request.token {
        return Err(HistoryError::StageChanged {
            expected: request.token,
            actual: Some(stage.token),
        });
    }
    let branch =
        super::branch::branch(tx, stage.branch)?.ok_or(HistoryError::Integrity("Branch"))?;
    if branch.stack != stage.stack {
        return Err(HistoryError::Integrity("Branch ownership"));
    }
    let base_root = super::layerstack::layer_root(tx, stage.expected_base)?;
    let head_root = match stage.expected_head {
        Some(head) => Some(
            commit(tx, head)?
                .ok_or(HistoryError::Integrity("Commit"))?
                .root,
        ),
        None => None,
    };
    let effective = head_root.unwrap_or(base_root);
    if effective != stage.expected_root
        || stage.construction_base_root != stage.expected_root
        || stage.intended_commit_base != stage.expected_base
        || stage.profile
            != super::layerstack::layer_stack(tx, stage.stack)?
                .ok_or(HistoryError::Integrity("LayerStack"))?
                .profile
    {
        return Err(HistoryError::Integrity("stage context"));
    }
    if branch.head_commit != stage.expected_head || branch.base_layer != stage.expected_base {
        return Err(HistoryError::HeadMoved(Box::new(MovedState {
            expected_head: stage.expected_head,
            actual_head: branch.head_commit,
            expected_base: stage.expected_base,
            actual_base: branch.base_layer,
        })));
    }
    if stage.candidate_root == effective && stage.intended_commit_base == branch.base_layer {
        remove_stage(tx, &stage)?;
        return Ok(CommitStagedOutcome::UpToDate {
            head: branch.head_commit,
            root: effective,
        });
    }
    let record = CommitRecord {
        id: CommitId::derive(
            stage.candidate_root,
            branch.head_commit,
            stage.intended_commit_base,
        ),
        stack: stage.stack,
        root: stage.candidate_root,
        parent: branch.head_commit,
        base_layer: stage.intended_commit_base,
    };
    if !verify_commit(tx, &record)? {
        tx.execute(
            INSERT_COMMIT,
            rusqlite::params![
                record.id.as_slice(),
                record.stack.as_slice(),
                record.root.as_bytes().as_slice(),
                record.parent.map(|id| id.to_bytes().to_vec()),
                record.base_layer.as_slice(),
            ],
        )
        .map_err(sql)?;
    }
    let advanced = tx
        .execute(
            ADVANCE_BRANCH,
            rusqlite::params![
                record.id.as_slice(),
                branch.id.as_slice(),
                stage.expected_base.as_slice(),
                stage.expected_head.map(|id| id.to_bytes().to_vec()),
            ],
        )
        .map_err(sql)?;
    if advanced == 0 {
        let actual = one(tx, BRANCH_HEAD, [branch.id.as_slice()], |row| {
            let head: Option<Vec<u8>> = cell(row, 0)?;
            let base: Vec<u8> = cell(row, 1)?;
            Ok((head, base))
        })?
        .ok_or(HistoryError::Integrity("Branch head"))?;
        let actual_head = match actual.0 {
            Some(bytes) => Some(CommitId::from_slice(&bytes)?),
            None => None,
        };
        return Err(HistoryError::HeadMoved(Box::new(MovedState {
            expected_head: stage.expected_head,
            actual_head,
            expected_base: stage.expected_base,
            actual_base: crate::identity::LayerId::from_slice(&actual.1)?,
        })));
    }
    remove_stage(tx, &stage)?;
    Ok(CommitStagedOutcome::Committed(record))
}

fn remove_stage(tx: &Transaction<'_>, stage: &crate::records::StageRecord) -> HistoryResult<()> {
    let removed = tx
        .execute(
            DELETE_STAGE,
            rusqlite::params![stage.workspace.as_slice(), stage.token.value() as i64],
        )
        .map_err(sql)?;
    if removed != 1 {
        return Err(HistoryError::StageChanged {
            expected: stage.token,
            actual: None,
        });
    }
    Ok(())
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
