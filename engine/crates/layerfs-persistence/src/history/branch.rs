//! Branch reads, coherent snapshots and zero-copy forks.
//!
//! A fork copies no content and no history rows: it records one base Layer or
//! one selected ancestor Commit and shares everything reachable from it. Forking
//! from a historical Commit uses that Commit's own base, never the source
//! Branch's current base, because the Branch may have advanced past it.

use super::params;
use super::transaction::Transaction;
use layerfs_history::error::{
    HistoryError, HistoryResult, HolderKind, HolderState, Missing, NameHolder, NameSource,
    NameTakenState,
};
use layerfs_history::identity::{BranchId, CatalogId, HistoryName, LayerStackId};
use layerfs_history::records::{
    BranchRecord, BranchSnapshot, ForkRequest, ForkSource, Page, PageResult,
};
use layerfs_history::NamedSnapshot;

use super::query::{self, Context, Cursor, Range};
use super::rows::{branch_row, cell, commit_row, many, one, sql, stack_row};
use crate::backend::records::{Record as Row, Value};
use layerfs_content::ObjectId;
use layerfs_history::records::{CommitRecord, LayerStackRecord};

const BRANCH_BY_ID: &str = "branch_branch_by_id";

/// The same row without the visibility filter. Only rendering keeps a retired
/// Branch's name and stack; every decision a caller can act on (fork source,
/// name ownership, removal) uses the filtered [`branch`].
const BRANCH_BY_ID_ANY: &str = "branch_branch_by_id_any";

const BRANCH_BY_NAME: &str = "branch_branch_by_name";

const BRANCH_FIRST_PAGE: &str = "branch_branch_first_page";

const BRANCH_NEXT_PAGE: &str = "branch_branch_next_page";

const BRANCH_FROM_PAGE: &str = "branch_branch_from_page";

const BRANCH_NAME_TAKEN: &str = "branch_branch_name_taken";

const INSERT_BRANCH: &str = "branch_insert_branch";

/// Reads one Branch by identity.
pub(crate) fn branch(tx: &Transaction<'_>, id: BranchId) -> HistoryResult<Option<BranchRecord>> {
    one(tx, BRANCH_BY_ID, [id.as_slice()], branch_row)
}

/// Reads one Branch by identity without the visibility filter. Only
/// rendering uses it; every caller-facing lookup is filtered.
pub(crate) fn branch_any(
    tx: &Transaction<'_>,
    id: BranchId,
) -> HistoryResult<Option<BranchRecord>> {
    one(tx, BRANCH_BY_ID_ANY, [id.as_slice()], branch_row)
}

/// The typed `NameTaken` state for a name held in `stack`, live or retired;
/// `None` when no Branch row holds it. The query reads the retired holder on
/// purpose: a removed Branch still owns its name (spec 4.4).
pub(crate) fn name_taken(
    tx: &Transaction<'_>,
    stack: LayerStackId,
    name: &HistoryName,
) -> HistoryResult<Option<NameTakenState>> {
    one(
        tx,
        BRANCH_NAME_TAKEN,
        params![stack.as_slice(), name.as_str()],
        |row| {
            let name = cell::<String>(row, 0)?;
            let retired = cell::<i64>(row, 1)? != 0;
            let state = if retired {
                HolderState::Retired
            } else {
                HolderState::Live
            };
            Ok(NameTakenState::new(
                NameHolder::new(HolderKind::Branch, name, state),
                NameSource::Store,
            ))
        },
    )
}

/// The typed refusal for a Branch name that a row already holds.
pub(crate) fn taken(
    tx: &Transaction<'_>,
    stack: LayerStackId,
    name: &HistoryName,
) -> HistoryResult<HistoryError> {
    match name_taken(tx, stack, name)? {
        Some(state) => Ok(HistoryError::NameTaken(Box::new(state))),
        None => Err(HistoryError::Integrity("Branch name")),
    }
}

/// Reads one Branch by its name in a stack, through the unique name index.
pub(crate) fn branch_named(
    tx: &Transaction<'_>,
    stack: LayerStackId,
    name: &HistoryName,
) -> HistoryResult<Option<BranchRecord>> {
    one(
        tx,
        BRANCH_BY_NAME,
        params![stack.as_slice(), name.as_str()],
        branch_row,
    )
}

/// One Branch row with the rows its snapshot resolves to, read by one
/// statement: its LayerStack, its base Layer's root and its head Commit.
/// Each part stays undecoded until a caller checks it, in its own order.
pub(crate) struct SnapshotRows {
    pub(crate) branch: BranchRecord,
    row: Row,
}

impl SnapshotRows {
    fn part(&self, columns: std::ops::Range<usize>) -> Option<Row> {
        match self.row.cells.get(columns.start) {
            None | Some(Value::Null) => None,
            Some(_) => Some(Row {
                cells: self.row.cells[columns].to_vec(),
            }),
        }
    }
    /// The Branch's LayerStack, when its row exists.
    pub(crate) fn stack(&self) -> HistoryResult<Option<LayerStackRecord>> {
        self.part(6..11).as_ref().map(stack_row).transpose()
    }
    /// The root of the Branch's base Layer, when its row exists.
    pub(crate) fn base_root(&self) -> HistoryResult<Option<ObjectId>> {
        self.part(11..12)
            .map(|row| super::rows::object(&row, 0))
            .transpose()
    }
    /// The Branch's head Commit, when it has one and its row exists.
    pub(crate) fn head(&self) -> HistoryResult<Option<CommitRecord>> {
        self.part(12..29).as_ref().map(commit_row).transpose()
    }
}

const SNAPSHOT: &str = "branch_snapshot";

const SNAPSHOT_NAMED: &str = "branch_snapshot_named";

/// Reads one Branch and the rows its snapshot resolves to in one statement.
pub(crate) fn snapshot_rows(
    tx: &Transaction<'_>,
    id: BranchId,
) -> HistoryResult<Option<SnapshotRows>> {
    snapshot_of(tx.query(SNAPSHOT, [id.as_slice()])?)
}

const SNAPSHOT_EPOCH: &str = "commit_snapshot_epoch";

/// [`snapshot_rows`] and the `project_epoch` of the Branch's LayerStack, from
/// one statement and so one snapshot: a publication's fence opens on the
/// rows it then checks (spec 5). `Some(None)` is a Branch whose project has
/// no epoch row.
pub(crate) fn snapshot_rows_with_epoch(
    tx: &Transaction<'_>,
    id: BranchId,
) -> HistoryResult<Option<(SnapshotRows, Option<i64>)>> {
    let mut rows = tx.query(SNAPSHOT_EPOCH, [id.as_slice()])?;
    let mut epoch = None;
    for row in &mut rows {
        epoch = match row.cells.pop() {
            Some(Value::Integer(epoch)) => Some(epoch),
            Some(Value::Null) => None,
            _ => return Err(HistoryError::Integrity("catalog row")),
        };
    }
    Ok(snapshot_of(rows)?.map(|rows| (rows, epoch)))
}

/// The one snapshot row of `rows`, decoded, if any.
fn snapshot_of(mut rows: Vec<Row>) -> HistoryResult<Option<SnapshotRows>> {
    if rows.len() > 1 {
        return Err(HistoryError::Integrity("catalog row count"));
    }
    let Some(row) = rows.pop() else {
        return Ok(None);
    };
    if row.cells.len() != 29 {
        return Err(HistoryError::Integrity("catalog row"));
    }
    Ok(Some(SnapshotRows {
        branch: branch_row(&row)?,
        row,
    }))
}

/// Reads one coherent Branch snapshot with every root it resolves to, in
/// one statement.
pub(crate) fn branch_snapshot(
    tx: &Transaction<'_>,
    id: BranchId,
) -> HistoryResult<Option<BranchSnapshot>> {
    Ok(branch_snapshot_with_stack_name(tx, id)?.map(|(snapshot, _)| snapshot))
}

/// [`branch_snapshot`] with the stack name its statement already reads.
pub(crate) fn branch_snapshot_with_stack_name(
    tx: &Transaction<'_>,
    id: BranchId,
) -> HistoryResult<Option<(BranchSnapshot, HistoryName)>> {
    snapshot_rows(tx, id)?.map(resolve).transpose()
}

/// The snapshot of Branch `branch` of the stack named `stack`, found by both
/// names in one statement: no row is no such stack, a row without a Branch
/// is no such Branch.
pub(crate) fn branch_snapshot_named(
    tx: &Transaction<'_>,
    stack: &HistoryName,
    branch: &HistoryName,
) -> HistoryResult<NamedSnapshot> {
    let rows = tx.query(SNAPSHOT_NAMED, params![stack.as_str(), branch.as_str()])?;
    match rows.first().map(|row| row.cells.first()) {
        None => return Ok(NamedSnapshot::NoStack),
        Some(Some(Value::Null)) if rows.len() == 1 => return Ok(NamedSnapshot::NoBranch),
        Some(_) => {}
    }
    let rows = snapshot_of(rows)?.ok_or(HistoryError::Integrity("catalog row"))?;
    resolve(rows).map(|(snapshot, _)| NamedSnapshot::Found(Box::new(snapshot)))
}

/// The snapshot one Branch's rows resolve to, checked, with its stack's name.
fn resolve(rows: SnapshotRows) -> HistoryResult<(BranchSnapshot, HistoryName)> {
    let stack = rows
        .stack()?
        .ok_or(HistoryError::Integrity("Branch LayerStack"))?;
    let base_root = rows
        .base_root()?
        .ok_or(HistoryError::Missing(Missing::Layer))?;
    let record = rows.branch.clone();
    let head_root = match record.head_commit {
        Some(_) => {
            let commit = rows
                .head()?
                .ok_or(HistoryError::Integrity("Branch head Commit"))?;
            if commit.base_layer != record.base_layer || commit.stack != record.stack {
                return Err(HistoryError::Integrity("Branch head ancestry"));
            }
            Some(commit.root)
        }
        None => None,
    };
    let snapshot = BranchSnapshot {
        branch: record,
        head_root,
        base_root,
        effective_root: head_root.unwrap_or(base_root),
        scope: stack.scope,
        profile: stack.profile,
    };
    Ok((snapshot, stack.name))
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
            params![stack.as_slice(), name, window],
            branch_row,
        )?,
        None => many(
            tx,
            BRANCH_FIRST_PAGE,
            params![stack.as_slice(), window],
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

/// Reads one stack's Branches in name order from `from` on (after it, or at
/// it too when `inclusive`), at most `limit`, through the name index.
pub(crate) fn branches_from(
    tx: &Transaction<'_>,
    stack: LayerStackId,
    from: &str,
    inclusive: bool,
    limit: u16,
) -> HistoryResult<Vec<BranchRecord>> {
    if limit == 0 || limit > layerfs_history::records::MAXIMUM_PAGE_RECORDS {
        return Err(HistoryError::InvalidInput("page limit"));
    }
    if super::layerstack::layer_stack(tx, stack)?.is_none() {
        return Err(HistoryError::Missing(Missing::LayerStack));
    }
    let statement = if inclusive {
        BRANCH_FROM_PAGE
    } else {
        BRANCH_NEXT_PAGE
    };
    many(
        tx,
        statement,
        params![stack.as_slice(), from.to_owned(), i64::from(limit)],
        branch_row,
    )
}

/// Creates one Branch that shares the selected ancestry.
pub(crate) fn fork(tx: &Transaction<'_>, request: &ForkRequest) -> HistoryResult<BranchSnapshot> {
    // Spec 5: a fork is a write and takes the project epoch fence. The stack
    // is read once to refuse a missing project with `Missing`, the fence
    // opens, and the stack is read again so a removal that committed in the
    // window is visible (its row is filtered); one that commits later changes
    // the epoch `confirm` re-checks. Forking from a Branch a removal retires
    // in that window is therefore never green-lit.
    let _ = super::layerstack::layer_stack(tx, request.stack)?
        .ok_or(HistoryError::Missing(Missing::LayerStack))?;
    let fence = super::epoch::open(tx, request.stack)?;
    let _ = super::layerstack::layer_stack(tx, request.stack)?
        .ok_or(HistoryError::Missing(Missing::LayerStack))?;
    if branch_any(tx, request.branch)?.is_some() {
        return Err(taken(tx, request.stack, &request.name)?);
    }
    if let Some(state) = name_taken(tx, request.stack, &request.name)? {
        return Err(HistoryError::NameTaken(Box::new(state)));
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
        ForkSource::Commit {
            branch: source_branch,
            commit,
        } => {
            // The marker-filtered lookup: a Branch a removal retired before
            // this point cannot start a fork, even from a Commit it retained.
            // The unfiltered row would keep resolving provenance and let the
            // fork outlive the removal that won the race.
            let source =
                self::branch(tx, source_branch)?.ok_or(HistoryError::Missing(Missing::Branch))?;
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
        params![
            request.branch.as_slice(),
            request.stack.as_slice(),
            request.name.as_str(),
            base_layer.as_slice(),
            head_commit.map(|id| id.to_bytes().to_vec()),
            match request.source {
                ForkSource::Layer(id) => id.as_slice().to_vec(),
                ForkSource::Commit { commit, .. } => commit.as_slice().to_vec(),
            },
        ],
    )
    .map_err(sql)?;
    let snapshot =
        branch_snapshot(tx, request.branch)?.ok_or(HistoryError::Integrity("Branch insertion"))?;
    fence.confirm(tx)?;
    Ok(snapshot)
}
