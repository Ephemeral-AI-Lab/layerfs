//! LayerStack creation, Layer publication, reads and the publication chain.
//!
//! Publication is a bounded metadata operation with no content traversal: a
//! Layer records a root the caller already saved, and the only write besides its
//! own row is one conditional stack-head advance. Both happen inside the
//! caller's transaction, so a refused advance inserts nothing.
//!
//! An exact earlier publication is checked before any expected-head comparison,
//! because asking twice is idempotent while a re-read of the head is not: the
//! stack may legitimately have advanced since the first successful publication.

use crate::error::{HistoryError, HistoryResult, Missing, MovedState};
use crate::identity::{CatalogId, LayerId, LayerStackId};
use crate::records::{
    AddLayerOutcome, AddLayerRequest, LayerHistoryRequest, LayerRecord, LayerStackRecord, Page,
    PageResult, StackInitialization,
};
use layerfs_content::ObjectId;
use rusqlite::Transaction;

use super::query::{self, Context, Cursor, Range};
use super::rows::{cell, layer_row, many, one, sql, stack_row};

const STACK_BY_ID: &str = "SELECT layer_stack_id, name, scope_id, profile_id, head_layer_id \
     FROM layer_stacks WHERE layer_stack_id = ?1";

const STACK_FIRST_PAGE: &str = "SELECT layer_stack_id, name, scope_id, profile_id, head_layer_id \
     FROM layer_stacks ORDER BY name LIMIT ?1";

const STACK_NEXT_PAGE: &str = "SELECT layer_stack_id, name, scope_id, profile_id, head_layer_id \
     FROM layer_stacks WHERE name > ?1 ORDER BY name LIMIT ?2";

const LAYER_BY_ID: &str = "SELECT layer_id, layer_stack_id, parent_layer_id, root_id, \
     source_branch_id, source_commit_id FROM layers WHERE layer_id = ?1";

const LAYER_ROOT: &str = "SELECT root_id FROM layers WHERE layer_id = ?1";

const INSERT_STACK: &str = "INSERT INTO layer_stacks \
     (layer_stack_id, name, scope_id, profile_id, head_layer_id) VALUES (?1, ?2, ?3, ?4, ?5)";

const INSERT_LAYER: &str = "INSERT INTO layers \
     (layer_id, layer_stack_id, parent_layer_id, root_id, source_branch_id, source_commit_id) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6)";

const STACK_NAME_TAKEN: &str = "SELECT 1 FROM layer_stacks WHERE name = ?1";

const ADVANCE_STACK: &str = "UPDATE layer_stacks SET head_layer_id = ?1 \
     WHERE layer_stack_id = ?2 AND head_layer_id = ?3";

const STACK_HEAD: &str = "SELECT head_layer_id FROM layer_stacks WHERE layer_stack_id = ?1";

const LAYER_BY_SOURCE: &str = "SELECT layer_id FROM layers \
     WHERE source_branch_id = ?1 AND source_commit_id = ?2";

/// Creates the genesis Layer and the stack in one transaction.
pub(crate) fn initialize_layerstack(
    tx: &Transaction<'_>,
    request: &StackInitialization,
) -> HistoryResult<LayerStackRecord> {
    if layer_stack(tx, request.stack)?.is_some() {
        return Err(HistoryError::Integrity("LayerStack identity"));
    }
    if one(tx, STACK_NAME_TAKEN, [request.name.as_str()], |_| Ok(()))?.is_some() {
        return Err(HistoryError::Integrity("LayerStack name"));
    }
    let genesis = LayerId::derive(request.stack, None, request.genesis_root);
    tx.execute(
        INSERT_STACK,
        rusqlite::params![
            request.stack.as_slice(),
            request.name.as_str(),
            request.scope.as_bytes().as_slice(),
            request.profile.as_bytes().as_slice(),
            genesis.as_slice(),
        ],
    )
    .map_err(sql)?;
    tx.execute(
        INSERT_LAYER,
        rusqlite::params![
            genesis.as_slice(),
            request.stack.as_slice(),
            Option::<Vec<u8>>::None,
            request.genesis_root.as_bytes().as_slice(),
            Option::<Vec<u8>>::None,
            Option::<Vec<u8>>::None,
        ],
    )
    .map_err(sql)?;
    layer_stack(tx, request.stack)?.ok_or(HistoryError::Integrity("LayerStack insertion"))
}

/// Reads one stack by identity.
pub(crate) fn layer_stack(
    tx: &Transaction<'_>,
    id: LayerStackId,
) -> HistoryResult<Option<LayerStackRecord>> {
    one(tx, STACK_BY_ID, [id.as_slice()], stack_row)
}

/// Reads stacks in name order, one bounded page at a time.
pub(crate) fn layer_stacks(
    tx: &Transaction<'_>,
    catalog: CatalogId,
    incarnation: u64,
    page: &Page,
) -> HistoryResult<PageResult<LayerStackRecord>> {
    page.check()?;
    let capacity = query::capacity(page.limit, LayerStackRecord::MAXIMUM_ENCODED_BYTES)?;
    let window = capacity as i64 + 1;
    let context = Context {
        catalog,
        incarnation,
        range: Range::StackNames,
        subject: &[],
    };
    let after = match &page.cursor {
        None => None,
        Some(bytes) => {
            let cursor = query::decode(context, bytes)?;
            if !cursor.anchor.is_empty() {
                return Err(HistoryError::InvalidInput("page cursor anchor"));
            }
            Some(
                String::from_utf8(cursor.position)
                    .map_err(|_| HistoryError::InvalidInput("page cursor position"))?,
            )
        }
    };
    let records = match after {
        Some(name) => many(
            tx,
            STACK_NEXT_PAGE,
            rusqlite::params![name, window],
            stack_row,
        )?,
        None => many(tx, STACK_FIRST_PAGE, rusqlite::params![window], |row| {
            stack_row(row)
        })?,
    };
    let more = records.len() > capacity;
    query::page(records, capacity, more, |record| {
        query::encode(
            context,
            &Cursor {
                anchor: Vec::new(),
                position: record.name.as_str().as_bytes().to_vec(),
            },
        )
        .map(Some)
    })
}

/// Reads one Layer by identity.
pub(crate) fn layer(tx: &Transaction<'_>, id: LayerId) -> HistoryResult<Option<LayerRecord>> {
    one(tx, LAYER_BY_ID, [id.as_slice()], layer_row)
}

/// Root recorded by one Layer.
pub(crate) fn layer_root(tx: &Transaction<'_>, id: LayerId) -> HistoryResult<ObjectId> {
    one(tx, LAYER_ROOT, [id.as_slice()], |row| {
        super::rows::object(row, 0)
    })?
    .ok_or(HistoryError::Missing(Missing::Layer))
}

/// True when an existing row for this identity carries the same immutable fields.
pub(crate) fn verify_layer(tx: &Transaction<'_>, expected: &LayerRecord) -> HistoryResult<bool> {
    match layer(tx, expected.id)? {
        None => Ok(false),
        Some(stored) => {
            if stored.parent == expected.parent
                && stored.root == expected.root
                && stored.stack == expected.stack
                && stored.source_branch == expected.source_branch
                && stored.source_commit == expected.source_commit
            {
                Ok(true)
            } else {
                Err(HistoryError::Integrity("Layer provenance"))
            }
        }
    }
}

/// Walks one stack's publication chain from an immutable start, newest first.
pub(crate) fn layer_history(
    tx: &Transaction<'_>,
    catalog: CatalogId,
    incarnation: u64,
    request: &LayerHistoryRequest,
) -> HistoryResult<PageResult<LayerRecord>> {
    let stack =
        layer_stack(tx, request.stack)?.ok_or(HistoryError::Missing(Missing::LayerStack))?;
    let start = match request.start {
        Some(start) => {
            let record = layer(tx, start)?.ok_or(HistoryError::Missing(Missing::Layer))?;
            if record.stack != request.stack {
                return Err(HistoryError::Integrity("Layer ownership"));
            }
            start
        }
        None => stack.head_layer,
    };
    let context = Context {
        catalog,
        incarnation,
        range: Range::LayerChain,
        subject: request.stack.as_slice(),
    };
    let mut current = Some(start);
    if let Some(bytes) = &request.cursor {
        let cursor = query::decode(context, bytes)?;
        if cursor.anchor != start.as_slice() {
            return Err(HistoryError::InvalidInput("page cursor anchor"));
        }
        // The continuation names the last delivered Layer; the walk resumes at
        // its parent, so a page never repeats the record it ended on.
        current = match cursor.position.is_empty() {
            true => None,
            false => {
                layer(tx, LayerId::from_slice(&cursor.position)?)?
                    .ok_or(HistoryError::Integrity("Layer chain"))?
                    .parent
            }
        };
    }
    let capacity = query::capacity(request.limit, LayerRecord::MAXIMUM_ENCODED_BYTES)?;
    let mut records = Vec::new();
    while let Some(id) = current {
        if records.len() > capacity {
            break;
        }
        let record = layer(tx, id)?.ok_or(HistoryError::Integrity("Layer chain"))?;
        if record.stack != request.stack {
            return Err(HistoryError::Integrity("Layer ownership"));
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

/// Publishes one Commit root as a new Layer against the expected stack head.
pub(crate) fn add_layer(
    tx: &Transaction<'_>,
    request: &AddLayerRequest,
) -> HistoryResult<AddLayerOutcome> {
    let stack =
        layer_stack(tx, request.stack)?.ok_or(HistoryError::Missing(Missing::LayerStack))?;
    let branch =
        super::branch::branch(tx, request.branch)?.ok_or(HistoryError::Missing(Missing::Branch))?;
    if branch.stack != request.stack {
        return Err(HistoryError::Integrity("Branch ownership"));
    }
    let commit =
        super::commit::commit(tx, request.commit)?.ok_or(HistoryError::Missing(Missing::Commit))?;
    if commit.stack != request.stack {
        return Err(HistoryError::Integrity("Commit ownership"));
    }
    if let Some(layer) = published_source(tx, request.branch, request.commit)? {
        return Ok(AddLayerOutcome::UpToDate { layer });
    }
    let moved = || {
        HistoryError::HeadMoved(Box::new(MovedState {
            expected_head: Some(request.commit),
            actual_head: branch.head_commit,
            expected_base: request.expected_branch_base,
            actual_base: branch.base_layer,
        }))
    };
    if branch.head_commit != Some(request.commit) {
        return Err(moved());
    }
    if branch.base_layer != request.expected_branch_base {
        return Err(moved());
    }
    if commit.base_layer != branch.base_layer {
        return Err(HistoryError::BaseMismatch {
            commit_base: commit.base_layer,
            branch_base: branch.base_layer,
        });
    }
    if stack.head_layer != request.expected_stack_head {
        return Err(HistoryError::StackMoved {
            expected: request.expected_stack_head,
            actual: stack.head_layer,
        });
    }
    if commit.root == layer_root(tx, branch.base_layer)? {
        return Ok(AddLayerOutcome::NoChanges {
            head: stack.head_layer,
        });
    }
    let layer = LayerRecord {
        id: LayerId::derive(request.stack, Some(branch.base_layer), commit.root),
        stack: request.stack,
        parent: Some(branch.base_layer),
        root: commit.root,
        source_branch: Some(request.branch),
        source_commit: Some(request.commit),
    };
    if !verify_layer(tx, &layer)? {
        tx.execute(
            INSERT_LAYER,
            rusqlite::params![
                layer.id.as_slice(),
                layer.stack.as_slice(),
                layer.parent.map(|parent| parent.to_bytes().to_vec()),
                layer.root.as_bytes().as_slice(),
                layer.source_branch.map(|id| id.to_bytes().to_vec()),
                layer.source_commit.map(|id| id.to_bytes().to_vec()),
            ],
        )
        .map_err(sql)?;
    }
    let advanced = tx
        .execute(
            ADVANCE_STACK,
            rusqlite::params![
                layer.id.as_slice(),
                request.stack.as_slice(),
                request.expected_stack_head.as_slice(),
            ],
        )
        .map_err(sql)?;
    if advanced == 0 {
        let actual = one(tx, STACK_HEAD, [request.stack.as_slice()], |row| {
            cell::<Vec<u8>>(row, 0)
        })?
        .ok_or(HistoryError::Integrity("LayerStack head"))?;
        return Err(HistoryError::StackMoved {
            expected: request.expected_stack_head,
            actual: LayerId::from_slice(&actual)?,
        });
    }
    Ok(AddLayerOutcome::Added(layer))
}

fn published_source(
    tx: &Transaction<'_>,
    branch: crate::identity::BranchId,
    commit: crate::identity::CommitId,
) -> HistoryResult<Option<LayerId>> {
    one(
        tx,
        LAYER_BY_SOURCE,
        rusqlite::params![branch.as_slice(), commit.as_slice()],
        |row| {
            let bytes: Vec<u8> = cell(row, 0)?;
            LayerId::from_slice(&bytes)
        },
    )
}
