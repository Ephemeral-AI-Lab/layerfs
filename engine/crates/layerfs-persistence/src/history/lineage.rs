//! Lineage records that answer ancestry without walking history (#353).
//!
//! A Layer records its height in the stack chain. A Commit records its *run*
//! (a first-parent path, named by its first Commit), its first-parent depth
//! above the base Layer, the height of the newest Layer in its ancestry and,
//! for a merge, its Commit second parent. The first Layer whose ancestry holds
//! a Commit *absorbs* it; what one publication absorbs is a set of run
//! prefixes, stored as `absorbed(run, depth, height)`. `reach` keeps two
//! frontiers of the Layers published from a source: by the source's run and
//! depth, and, for a source an older Layer had already absorbed, by stack and
//! that older height. A frontier keeps only rows no newer row dominates, so its
//! maximum is one indexed lookup.
//!
//! Walks visit run segments and merges, never ordinary Commits or the Layers
//! between two heights, and have no budget: every answer is exact.

use super::params;
use super::rows::{cell, commit_column, layer_column, many, one, required, sql, stack_column};
use super::transaction::Transaction;
use layerfs_history::error::{HistoryError, HistoryResult, Missing};
use layerfs_history::identity::{CommitId, LayerId, LayerStackId, NodeId};
use layerfs_history::records::CommitRecord;
use std::collections::HashMap;

const COMMIT: &str = "lineage_commit";
const LAYER: &str = "lineage_layer";
const LAYERS_ABOVE: &str = "lineage_layers_above";
const COMMIT_AT: &str = "lineage_commit_at";
const INSERT_COMMIT: &str = "lineage_insert_commit";
const INSERT_LAYER: &str = "lineage_insert_layer";
const MERGES: &str = "lineage_merges";
const ABSORBED_BELOW: &str = "lineage_absorbed_below";
const ABSORBED_HEIGHT: &str = "lineage_absorbed_height";
const ABSORB: &str = "lineage_absorb";
const REACH: &str = "lineage_reach";
const REACH_PRUNE: &str = "lineage_reach_prune";
const REACH_INSERT: &str = "lineage_reach_insert";

/// The height of a Layer that does not exist: absorbed by none yet.
pub(crate) const NEVER: i64 = i64::MAX;

/// One Commit's lineage row with the Commit fields walks follow.
pub(crate) struct CommitLine {
    pub(crate) stack: LayerStackId,
    pub(crate) run: CommitId,
    pub(crate) depth: i64,
    pub(crate) newest: i64,
    pub(crate) parent: Option<CommitId>,
}

/// One Layer's place in its stack chain.
pub(crate) struct LayerLine {
    pub(crate) stack: LayerStackId,
    pub(crate) height: i64,
    pub(crate) overwrote: i64,
}

pub(crate) fn commit_line(tx: &Transaction<'_>, id: CommitId) -> HistoryResult<CommitLine> {
    one(tx, COMMIT, [id.as_slice()], |row| {
        Ok(CommitLine {
            run: required(commit_column(row, 0)?, "Commit run")?,
            depth: cell(row, 1)?,
            newest: cell(row, 2)?,
            stack: required(stack_column(row, 3)?, "LayerStack identity")?,
            parent: commit_column(row, 4)?,
        })
    })?
    .ok_or(HistoryError::Missing(Missing::Commit))
}

pub(crate) fn layer_line(tx: &Transaction<'_>, id: LayerId) -> HistoryResult<LayerLine> {
    one(tx, LAYER, [id.as_slice()], |row| {
        Ok(LayerLine {
            stack: required(stack_column(row, 0)?, "LayerStack identity")?,
            height: cell(row, 1)?,
            overwrote: cell(row, 2)?,
        })
    })?
    .ok_or(HistoryError::Missing(Missing::Layer))
}

/// The newest Layers of `stack` above `floor` up to `top`, newest first.
pub(crate) fn layers_above(
    tx: &Transaction<'_>,
    stack: LayerStackId,
    floor: i64,
    top: i64,
    limit: usize,
) -> HistoryResult<Vec<LayerId>> {
    many(
        tx,
        LAYERS_ABOVE,
        params![stack.as_slice(), floor, top, limit as i64],
        |row| required(layer_column(row, 0)?, "Layer identity"),
    )
}

/// The Layers `layer`'s publication overwrote, newest first, after skipping
/// `position`: the `overwrote` Layers directly below it in the chain.
pub(crate) fn overwritten(
    tx: &Transaction<'_>,
    layer: LayerId,
    position: u64,
    limit: usize,
) -> HistoryResult<Vec<LayerId>> {
    let line = layer_line(tx, layer)?;
    let skip = i64::try_from(position)
        .unwrap_or(i64::MAX)
        .min(line.overwrote);
    layers_above(
        tx,
        line.stack,
        line.height - 1 - line.overwrote,
        line.height - 1 - skip,
        limit,
    )
}

/// The Layer of `stack` at `height`.
pub(crate) fn layer_at(
    tx: &Transaction<'_>,
    stack: LayerStackId,
    height: i64,
) -> HistoryResult<LayerId> {
    layers_above(tx, stack, height - 1, height, 1)?
        .pop()
        .ok_or(HistoryError::Integrity("Layer chain"))
}

/// The Commit of `run` at `depth`, when the run reaches it.
pub(crate) fn commit_at(
    tx: &Transaction<'_>,
    run: CommitId,
    depth: i64,
) -> HistoryResult<Option<CommitId>> {
    one(tx, COMMIT_AT, params![run.as_slice(), depth], |row| {
        required(commit_column(row, 0)?, "Commit identity")
    })
}

/// Records a new Commit's lineage from its parents' rows.
pub(crate) fn record_commit(tx: &Transaction<'_>, record: &CommitRecord) -> HistoryResult<()> {
    record_parents(
        tx,
        record.id,
        record.parent,
        record.base_layer,
        record.second_parent,
    )
}

/// Records one Commit's lineage; its parents' rows exist.
pub(crate) fn record_parents(
    tx: &Transaction<'_>,
    id: CommitId,
    parent: Option<CommitId>,
    base_layer: LayerId,
    second_parent: Option<NodeId>,
) -> HistoryResult<()> {
    let (run, depth, mut newest) = match parent {
        Some(parent) => {
            let line = commit_line(tx, parent)?;
            // A run is a path: a second child of `parent` starts its own.
            let run = match commit_at(tx, line.run, line.depth + 1)? {
                Some(_) => id,
                None => line.run,
            };
            (run, line.depth + 1, line.newest)
        }
        None => (id, 1, layer_line(tx, base_layer)?.height),
    };
    let merged = match second_parent {
        Some(NodeId::Commit(second)) => {
            newest = newest.max(commit_line(tx, second)?.newest);
            Some(second)
        }
        Some(NodeId::Layer(second)) => {
            newest = newest.max(layer_line(tx, second)?.height);
            None
        }
        None => None,
    };
    tx.execute(
        INSERT_COMMIT,
        params![
            id.as_slice(),
            run.as_slice(),
            depth,
            newest,
            merged.map(|id| id.as_slice().to_vec()),
        ],
    )
    .map_err(sql)?;
    Ok(())
}

pub(crate) fn record_layer(
    tx: &Transaction<'_>,
    id: LayerId,
    stack: LayerStackId,
    height: i64,
    overwrote: i64,
) -> HistoryResult<()> {
    tx.execute(
        INSERT_LAYER,
        params![id.as_slice(), stack.as_slice(), height, overwrote],
    )
    .map_err(sql)?;
    Ok(())
}

/// The deepest prefix of `run` absorbed by a Layer below height `below`; 0
/// when none is.
pub(crate) fn absorbed_below(
    tx: &Transaction<'_>,
    run: CommitId,
    below: i64,
) -> HistoryResult<i64> {
    Ok(
        one(tx, ABSORBED_BELOW, params![run.as_slice(), below], |row| {
            cell(row, 0)
        })?
        .unwrap_or(0),
    )
}

/// The height of the Layer that absorbed the Commit, or [`NEVER`].
pub(crate) fn absorbed_height(tx: &Transaction<'_>, line: &CommitLine) -> HistoryResult<i64> {
    Ok(one(
        tx,
        ABSORBED_HEIGHT,
        params![line.run.as_slice(), line.depth],
        |row| cell(row, 0),
    )?
    .unwrap_or(NEVER))
}

/// The newest Layer a frontier holds at `x` or below.
fn reach(tx: &Transaction<'_>, key: &[u8], x: i64) -> HistoryResult<Option<i64>> {
    one(tx, REACH, params![key, x], |row| cell(row, 0))
}

/// Adds the newest Layer to a frontier, dropping the rows it dominates.
fn reach_insert(tx: &Transaction<'_>, key: &[u8], x: i64, height: i64) -> HistoryResult<()> {
    tx.execute(REACH_PRUNE, params![key, x]).map_err(sql)?;
    tx.execute(REACH_INSERT, params![key, x, height])
        .map_err(sql)?;
    Ok(())
}

/// Depths `(lo, hi]` of `run`: local Commits no Layer below the explore's
/// height absorbed.
#[derive(Clone, Copy)]
pub(crate) struct Segment {
    pub(crate) run: CommitId,
    pub(crate) lo: i64,
    pub(crate) hi: i64,
}

/// Visits the run segments of `start`'s local Commits (reached without
/// passing a Layer) that no Layer below height `below` absorbed. A skipped
/// region is in that Layer's ancestry. Each segment's merges are followed
/// through their Commit second parents, and a run's start through its first
/// parent. Returns true when `visit` stopped the walk.
pub(crate) fn explore(
    tx: &Transaction<'_>,
    start: &CommitLine,
    below: i64,
    mut visit: impl FnMut(Segment) -> HistoryResult<bool>,
) -> HistoryResult<bool> {
    let mut explored: HashMap<CommitId, (i64, i64)> = HashMap::new();
    let mut pending = vec![(start.run, start.depth)];
    while let Some((run, hi)) = pending.pop() {
        let (lo, from, first) = match explored.get(&run) {
            Some(&(_, done)) if done >= hi => continue,
            Some(&(lo, done)) => (lo, done.max(lo), false),
            None => {
                let lo = absorbed_below(tx, run, below)?;
                (lo, lo, true)
            }
        };
        if hi <= lo {
            explored.insert(run, (lo, lo));
            continue;
        }
        explored.insert(run, (lo, hi));
        if visit(Segment { run, lo, hi })? {
            return Ok(true);
        }
        let merges = many(tx, MERGES, params![run.as_slice(), from, hi], |row| {
            required(commit_column(row, 0)?, "Commit second parent")
        })?;
        for merged in merges {
            let line = commit_line(tx, merged)?;
            pending.push((line.run, line.depth));
        }
        if first && lo == 0 {
            if let Some(parent) = commit_line(tx, run)?.parent {
                let line = commit_line(tx, parent)?;
                pending.push((line.run, line.depth));
            }
        }
    }
    Ok(false)
}

/// The newest Layer up to `head` that publishing `source` does not skip: one
/// in its ancestry (at most `source.newest`) or one published from a Commit in
/// its ancestry. A source in the ancestry of the Layer at `source.newest` was
/// published either below it or as a late Layer (the stack frontier); one in
/// the rest is in a segment explored above that height (the run frontiers).
pub(crate) fn covered(
    tx: &Transaction<'_>,
    stack: LayerStackId,
    head: i64,
    source: &CommitLine,
) -> HistoryResult<i64> {
    let mut covered = source.newest;
    if covered >= head {
        return Ok(head);
    }
    if let Some(height) = reach(tx, stack.as_slice(), covered)? {
        covered = covered.max(height);
    }
    explore(tx, source, source.newest + 1, |segment| {
        if let Some(height) = reach(tx, segment.run.as_slice(), segment.hi)? {
            covered = covered.max(height);
        }
        Ok(covered >= head)
    })?;
    Ok(covered.min(head))
}

/// Records what the Layer at `height`, published from `source`, absorbs and
/// adds it to the frontiers. Runs inside the publishing transaction.
pub(crate) fn absorb(
    tx: &Transaction<'_>,
    stack: LayerStackId,
    source: &CommitLine,
    height: i64,
) -> HistoryResult<()> {
    let before = absorbed_height(tx, source)?;
    explore(tx, source, NEVER, |segment| {
        tx.execute(ABSORB, params![segment.run.as_slice(), segment.hi, height])
            .map_err(sql)?;
        Ok(false)
    })?;
    reach_insert(tx, source.run.as_slice(), source.depth, height)?;
    if before != NEVER {
        reach_insert(tx, stack.as_slice(), before, height)?;
    }
    Ok(())
}
