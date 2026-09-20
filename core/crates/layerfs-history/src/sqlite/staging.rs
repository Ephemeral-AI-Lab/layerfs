//! Exact-token Workspace stages: insertion, inspection and exact removal.
//!
//! A stage is a saved candidate plus the publication context it was captured
//! against. Neither part is optional: an accepted stage always satisfies
//! `construction_base_root == expected_root` and
//! `intended_commit_base == expected_base`, and the profile and scope it names
//! must be the stack's. That is what makes a later `commit_staged` a comparison
//! rather than a guess, and what stops a caller from rebasing by editing a token.
//!
//! Removing a stage removes one row. It deletes no content, keeps the inode
//! reservations the candidate consumed and never recycles a token.

use crate::error::{HistoryError, HistoryResult, Missing};
use crate::identity::{CatalogId, StageToken, WorkspaceId};
use crate::records::{DiscardOutcome, DiscardRequest, Page, PageResult, StageRecord, StageRequest};
use rusqlite::Transaction;

use super::query::{self, Context, Cursor, Range};
use super::rows::{many, one, sql, stage_row, token_column, unsigned};

const STAGE_BY_WORKSPACE: &str = "SELECT workspace_id, stage_token, layer_stack_id, branch_id, \
     expected_head_commit_id, expected_base_layer_id, expected_root_id, construction_base_root_id, \
     intended_commit_base_layer_id, candidate_root_id, profile_id, scope_id, input_generation \
     FROM workspace_stages WHERE workspace_id = ?1";

const STAGE_FIRST_PAGE: &str = "SELECT workspace_id, stage_token, layer_stack_id, branch_id, \
     expected_head_commit_id, expected_base_layer_id, expected_root_id, construction_base_root_id, \
     intended_commit_base_layer_id, candidate_root_id, profile_id, scope_id, input_generation \
     FROM workspace_stages WHERE branch_id = ?1 ORDER BY stage_token LIMIT ?2";

const STAGE_NEXT_PAGE: &str = "SELECT workspace_id, stage_token, layer_stack_id, branch_id, \
     expected_head_commit_id, expected_base_layer_id, expected_root_id, construction_base_root_id, \
     intended_commit_base_layer_id, candidate_root_id, profile_id, scope_id, input_generation \
     FROM workspace_stages WHERE branch_id = ?1 AND stage_token > ?2 ORDER BY stage_token LIMIT ?3";

const INSERT_STAGE: &str = "INSERT INTO workspace_stages \
     (workspace_id, stage_token, layer_stack_id, branch_id, expected_head_commit_id, \
      expected_base_layer_id, expected_root_id, construction_base_root_id, \
      intended_commit_base_layer_id, candidate_root_id, profile_id, scope_id, input_generation) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)";

const NEXT_TOKEN: &str = "SELECT next_stage_token FROM history_meta WHERE id = 1";

const BUMP_TOKEN: &str = "UPDATE history_meta SET next_stage_token = ?1 \
     WHERE id = 1 AND next_stage_token = ?2";

const DELETE_STAGE: &str = "DELETE FROM workspace_stages \
     WHERE workspace_id = ?1 AND stage_token = ?2";

/// Reads one stage by producer incarnation.
pub(crate) fn stage(
    tx: &Transaction<'_>,
    workspace: WorkspaceId,
) -> HistoryResult<Option<StageRecord>> {
    one(tx, STAGE_BY_WORKSPACE, [workspace.as_slice()], |row| {
        stage_row(row)
    })
}

/// Reads one Branch's stages in token order, one bounded page at a time.
pub(crate) fn stages(
    tx: &Transaction<'_>,
    catalog: CatalogId,
    incarnation: u64,
    branch: crate::identity::BranchId,
    page: &Page,
) -> HistoryResult<PageResult<StageRecord>> {
    page.check()?;
    if super::branch::branch(tx, branch)?.is_none() {
        return Err(HistoryError::Missing(Missing::Branch));
    }
    let capacity = query::capacity(page.limit, StageRecord::MAXIMUM_ENCODED_BYTES)?;
    let window = capacity as i64 + 1;
    let context = Context {
        catalog,
        incarnation,
        range: Range::StageTokens,
        subject: branch.as_slice(),
    };
    let after = match &page.cursor {
        None => None,
        Some(bytes) => {
            let cursor = query::decode(context, bytes)?;
            if !cursor.anchor.is_empty() || cursor.position.len() != 8 {
                return Err(HistoryError::InvalidInput("page cursor anchor"));
            }
            let mut raw = [0u8; 8];
            raw.copy_from_slice(&cursor.position);
            Some(i64::from_be_bytes(raw))
        }
    };
    let records = match after {
        Some(token) => many(
            tx,
            STAGE_NEXT_PAGE,
            rusqlite::params![branch.as_slice(), token, window],
            stage_row,
        )?,
        None => many(
            tx,
            STAGE_FIRST_PAGE,
            rusqlite::params![branch.as_slice(), window],
            stage_row,
        )?,
    };
    let more = records.len() > capacity;
    query::page(records, capacity, more, |record| {
        query::encode(
            context,
            &Cursor {
                anchor: Vec::new(),
                position: record.token.value().to_be_bytes().to_vec(),
            },
        )
        .map(Some)
    })
}

/// Inserts one frozen stage with a freshly allocated exact token.
pub(crate) fn stage_changes(
    tx: &Transaction<'_>,
    request: &StageRequest,
) -> HistoryResult<StageRecord> {
    if request.construction_base_root != request.expected_root {
        return Err(HistoryError::InvalidInput("construction base root"));
    }
    if request.intended_commit_base != request.expected_base {
        return Err(HistoryError::InvalidInput("intended Commit base"));
    }
    let branch =
        super::branch::branch(tx, request.branch)?.ok_or(HistoryError::Missing(Missing::Branch))?;
    let stack = super::layerstack::layer_stack(tx, branch.stack)?
        .ok_or(HistoryError::Integrity("Branch LayerStack"))?;
    if request.profile != stack.profile {
        return Err(HistoryError::InvalidInput("stage profile"));
    }
    if request.scope != stack.scope {
        return Err(HistoryError::InvalidInput("stage scope"));
    }
    let base = super::layerstack::layer(tx, request.expected_base)?
        .ok_or(HistoryError::Missing(Missing::Layer))?;
    if base.stack != stack.id {
        return Err(HistoryError::Integrity("Layer ownership"));
    }
    let intended = super::layerstack::layer(tx, request.intended_commit_base)?
        .ok_or(HistoryError::Missing(Missing::Layer))?;
    if intended.stack != stack.id {
        return Err(HistoryError::Integrity("Layer ownership"));
    }
    let head_root = match request.expected_head {
        Some(head) => {
            let commit =
                super::commit::commit(tx, head)?.ok_or(HistoryError::Missing(Missing::Commit))?;
            if commit.stack != stack.id || commit.base_layer != request.expected_base {
                return Err(HistoryError::Integrity("Commit ownership"));
            }
            Some(commit.root)
        }
        None => None,
    };
    if head_root.unwrap_or(base.root) != request.expected_root {
        return Err(HistoryError::InvalidInput("captured Branch root"));
    }
    if stage(tx, request.workspace)?.is_some() {
        return Err(HistoryError::InvalidInput("stage already present"));
    }
    let token = allocate_token(tx)?;
    tx.execute(
        INSERT_STAGE,
        rusqlite::params![
            request.workspace.as_slice(),
            token.value() as i64,
            stack.id.as_slice(),
            branch.id.as_slice(),
            request.expected_head.map(|id| id.to_bytes().to_vec()),
            request.expected_base.as_slice(),
            request.expected_root.as_bytes().as_slice(),
            request.construction_base_root.as_bytes().as_slice(),
            request.intended_commit_base.as_slice(),
            request.candidate_root.as_bytes().as_slice(),
            request.profile.as_bytes().as_slice(),
            request.scope.as_bytes().as_slice(),
            i64::try_from(request.generation)
                .map_err(|_| HistoryError::InvalidInput("input generation"))?,
        ],
    )
    .map_err(sql)?;
    stage(tx, request.workspace)?.ok_or(HistoryError::Integrity("stage insertion"))
}

/// Draws the next exact token, refusing at the checked counter's terminal value.
fn allocate_token(tx: &Transaction<'_>) -> HistoryResult<StageToken> {
    let next = one(tx, NEXT_TOKEN, [], |row| unsigned(row, 0))?
        .ok_or(HistoryError::Missing(Missing::Catalog))?;
    if next >= StageToken::MAXIMUM {
        return Err(HistoryError::Capacity("stage tokens"));
    }
    let token = StageToken::new(next)?;
    let bumped = tx
        .execute(
            BUMP_TOKEN,
            rusqlite::params![
                i64::try_from(next + 1).map_err(|_| HistoryError::Capacity("stage tokens"))?,
                i64::try_from(next).map_err(|_| HistoryError::Capacity("stage tokens"))?
            ],
        )
        .map_err(sql)?;
    if bumped != 1 {
        return Err(HistoryError::Integrity("stage token counter"));
    }
    Ok(token)
}

/// Removes exactly one stage token, or reports its absence.
pub(crate) fn discard_stage(
    tx: &Transaction<'_>,
    request: &DiscardRequest,
) -> HistoryResult<DiscardOutcome> {
    let present = one(
        tx,
        STAGE_BY_WORKSPACE,
        [request.workspace.as_slice()],
        |row| token_column(row, 1),
    )?;
    let Some(actual) = present else {
        return Ok(DiscardOutcome::Absent);
    };
    if actual != request.token {
        return Err(HistoryError::StageChanged {
            expected: request.token,
            actual: Some(actual),
        });
    }
    let removed = tx
        .execute(
            DELETE_STAGE,
            rusqlite::params![request.workspace.as_slice(), request.token.value() as i64],
        )
        .map_err(sql)?;
    if removed != 1 {
        return Err(HistoryError::StageChanged {
            expected: request.token,
            actual: None,
        });
    }
    Ok(DiscardOutcome::Removed)
}
