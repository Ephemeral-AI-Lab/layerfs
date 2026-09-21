//! Branch reads, coherent snapshots and zero-copy forks.
//!
//! A fork copies no content and no history rows: it records one base Layer or
//! one selected ancestor Commit and shares everything reachable from it. Forking
//! from a historical Commit uses that Commit's own base, never the source
//! Branch's current base, because the Branch may have advanced past it.

use crate::error::{HistoryError, HistoryResult, Missing};
use crate::identity::{BranchId, CatalogId, LayerStackId};
use crate::records::{BranchRecord, BranchSnapshot, ForkRequest, ForkSource, Page, PageResult};
use rusqlite::Transaction;

use super::query::{self, Context, Cursor, Range};
use super::rows::{branch_row, many, one, sql};

const BRANCH_BY_ID: &str = "SELECT branch_id, layer_stack_id, name, base_layer_id, head_commit_id \
     FROM branches WHERE branch_id = ?1";

const BRANCH_FIRST_PAGE: &str = "SELECT branch_id, layer_stack_id, name, base_layer_id, \
     head_commit_id FROM branches WHERE layer_stack_id = ?1 ORDER BY name LIMIT ?2";

const BRANCH_NEXT_PAGE: &str = "SELECT branch_id, layer_stack_id, name, base_layer_id, \
     head_commit_id FROM branches WHERE layer_stack_id = ?1 AND name > ?2 ORDER BY name LIMIT ?3";

const BRANCH_NAME_TAKEN: &str = "SELECT 1 FROM branches \
     WHERE layer_stack_id = ?1 AND name = ?2";

const INSERT_BRANCH: &str = "INSERT INTO branches \
     (branch_id, layer_stack_id, name, base_layer_id, head_commit_id) VALUES (?1, ?2, ?3, ?4, ?5)";

/// Reads one Branch by identity.
pub(crate) fn branch(tx: &Transaction<'_>, id: BranchId) -> HistoryResult<Option<BranchRecord>> {
    one(tx, BRANCH_BY_ID, [id.as_slice()], branch_row)
}

/// Reads one coherent Branch snapshot with every root it resolves to.
pub(crate) fn branch_snapshot(
    tx: &Transaction<'_>,
    id: BranchId,
) -> HistoryResult<Option<BranchSnapshot>> {
    let Some(record) = branch(tx, id)? else {
        return Ok(None);
    };
    let stack = super::layerstack::layer_stack(tx, record.stack)?
        .ok_or(HistoryError::Integrity("Branch LayerStack"))?;
    let base_root = super::layerstack::layer_root(tx, record.base_layer)?;
    let head_root = match record.head_commit {
        Some(head) => {
            let commit = super::commit::commit(tx, head)?
                .ok_or(HistoryError::Integrity("Branch head Commit"))?;
            if commit.base_layer != record.base_layer || commit.stack != record.stack {
                return Err(HistoryError::Integrity("Branch head ancestry"));
            }
            Some(commit.root)
        }
        None => None,
    };
    Ok(Some(BranchSnapshot {
        branch: record,
        head_root,
        base_root,
        effective_root: head_root.unwrap_or(base_root),
        scope: stack.scope,
        profile: stack.profile,
    }))
}

/// Reads one stack's Branches in name order, one bounded page at a time.
pub(crate) fn branches(
    tx: &Transaction<'_>,
    catalog: CatalogId,
    incarnation: u64,
    key: &[u8; 32],
    stack: LayerStackId,
    page: &Page,
) -> HistoryResult<PageResult<BranchRecord>> {
    page.check()?;
    if super::layerstack::layer_stack(tx, stack)?.is_none() {
        return Err(HistoryError::Missing(Missing::LayerStack));
    }
    let capacity = query::capacity(page.limit, BranchRecord::MAXIMUM_ENCODED_BYTES)?;
    let window = capacity as i64 + 1;
    let context = Context {
        key,
        catalog,
        incarnation,
        range: Range::BranchNames,
        subject: stack.as_slice(),
    };
    let after = match &page.cursor {
        None => None,
        Some(bytes) => {
            let cursor = query::decode(context, bytes)?;
            if !cursor.anchor.is_empty() {
                return Err(HistoryError::InvalidInput("page cursor anchor"));
            }
            {
                let record = branch(tx, BranchId::from_slice(&cursor.position)?)?
                    .ok_or(HistoryError::Integrity("page cursor position"))?;
                if record.stack != stack {
                    return Err(HistoryError::Integrity("page cursor ownership"));
                }
                Some(record.name.as_str().to_owned())
            }
        }
    };
    let records = match after {
        Some(name) => many(
            tx,
            BRANCH_NEXT_PAGE,
            rusqlite::params![stack.as_slice(), name, window],
            branch_row,
        )?,
        None => many(
            tx,
            BRANCH_FIRST_PAGE,
            rusqlite::params![stack.as_slice(), window],
            branch_row,
        )?,
    };
    let more = records.len() > capacity;
    query::page(records, capacity, more, |record| {
        query::encode(
            context,
            &Cursor {
                anchor: Vec::new(),
                position: record.id.as_slice().to_vec(),
            },
        )
        .map(Some)
    })
}

/// Creates one Branch that shares the selected ancestry.
pub(crate) fn fork(tx: &Transaction<'_>, request: &ForkRequest) -> HistoryResult<BranchSnapshot> {
    if super::layerstack::layer_stack(tx, request.stack)?.is_none() {
        return Err(HistoryError::Missing(Missing::LayerStack));
    }
    if branch(tx, request.branch)?.is_some() {
        return Err(HistoryError::Integrity("Branch identity"));
    }
    if one(
        tx,
        BRANCH_NAME_TAKEN,
        rusqlite::params![request.stack.as_slice(), request.name.as_str()],
        |_| Ok(()),
    )?
    .is_some()
    {
        return Err(HistoryError::Integrity("Branch name"));
    }
    let (base_layer, head_commit) = match request.source {
        ForkSource::Layer(layer) => {
            let record = super::layerstack::layer(tx, layer)?
                .ok_or(HistoryError::Missing(Missing::Layer))?;
            if record.stack != request.stack {
                return Err(HistoryError::Integrity("Layer ownership"));
            }
            (layer, None)
        }
        ForkSource::Commit { branch, commit } => {
            let source = self::branch(tx, branch)?.ok_or(HistoryError::Missing(Missing::Branch))?;
            if source.stack != request.stack {
                return Err(HistoryError::Integrity("Branch ownership"));
            }
            let selected =
                super::commit::commit(tx, commit)?.ok_or(HistoryError::Missing(Missing::Commit))?;
            if selected.stack != request.stack {
                return Err(HistoryError::Integrity("Commit ownership"));
            }
            if !super::commit::ancestry_contains(tx, source.head_commit, commit)? {
                return Err(HistoryError::NotInHistory("Commit"));
            }
            (selected.base_layer, Some(commit))
        }
    };
    tx.execute(
        INSERT_BRANCH,
        rusqlite::params![
            request.branch.as_slice(),
            request.stack.as_slice(),
            request.name.as_str(),
            base_layer.as_slice(),
            head_commit.map(|id| id.to_bytes().to_vec()),
        ],
    )
    .map_err(sql)?;
    branch_snapshot(tx, request.branch)?.ok_or(HistoryError::Integrity("Branch insertion"))
}
