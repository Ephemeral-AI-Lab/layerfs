//! Ancestry over the history graph: merge base and reachability.
//!
//! A Commit reaches its parent (or, without one, its base Layer) and its
//! second parent; a Layer reaches its parent Layer and the Commit it was
//! published from. A stack's Layers form one chain, so a node's ancestry is
//! its *local* Commits, reached without passing a Layer, plus the ancestry of
//! the newest Layer it reaches. The lineage records (`lineage.rs`) answer the
//! Layer part by height and absorption; only local Commits that no older
//! Layer absorbed are explored, a run segment and a merge at a time. No walk
//! has a budget: answers are exact whatever the history's size.

use super::lineage::{self, CommitLine, Segment};
use super::transaction::Transaction;
use layerfs_history::error::HistoryResult;
use layerfs_history::identity::{CommitId, LayerStackId, NodeId};
use std::collections::HashMap;

enum Line {
    Layer(LayerStackId, i64),
    Commit(CommitLine),
}

impl Line {
    fn read(tx: &Transaction<'_>, node: NodeId) -> HistoryResult<Self> {
        Ok(match node {
            NodeId::Layer(id) => {
                let line = lineage::layer_line(tx, id)?;
                Self::Layer(line.stack, line.height)
            }
            NodeId::Commit(id) => Self::Commit(lineage::commit_line(tx, id)?),
        })
    }

    fn stack(&self) -> LayerStackId {
        match self {
            Self::Layer(stack, _) => *stack,
            Self::Commit(line) => line.stack,
        }
    }

    /// Height of the newest Layer in this node's ancestry.
    fn newest(&self) -> i64 {
        match self {
            Self::Layer(_, height) => *height,
            Self::Commit(line) => line.newest,
        }
    }

    /// Order among maximal common ancestors: newest Layer, then deepest
    /// first parent, then smallest identity.
    fn rank(&self, node: NodeId) -> (i64, i64, std::cmp::Reverse<Vec<u8>>) {
        let depth = match self {
            Self::Layer(..) => 0,
            Self::Commit(line) => line.depth,
        };
        (
            self.newest(),
            depth,
            std::cmp::Reverse(node.as_slice().to_vec()),
        )
    }
}

/// True when node `a` is in the ancestry of node `d` (same stack).
fn contains(tx: &Transaction<'_>, a: &Line, d: &Line) -> HistoryResult<bool> {
    let Line::Commit(commit) = a else {
        return Ok(a.newest() <= d.newest());
    };
    let absorbed = lineage::absorbed_height(tx, commit)?;
    if absorbed <= d.newest() {
        return Ok(true);
    }
    let Line::Commit(descendant) = d else {
        return Ok(false);
    };
    if commit.newest > descendant.newest {
        return Ok(false);
    }
    // Regions absorbed below `absorbed` lie in Layers that do not hold `a`.
    lineage::explore(tx, descendant, absorbed, |segment| {
        Ok(segment.run == commit.run && segment.lo < commit.depth && commit.depth <= segment.hi)
    })
}

/// True when `ancestor` is `descendant` or reachable from it.
pub(crate) fn is_ancestor(
    tx: &Transaction<'_>,
    ancestor: NodeId,
    descendant: NodeId,
) -> HistoryResult<bool> {
    if ancestor == descendant {
        return Ok(true);
    }
    let (a, d) = (Line::read(tx, ancestor)?, Line::read(tx, descendant)?);
    if a.stack() != d.stack() {
        return Ok(false);
    }
    contains(tx, &a, &d)
}

/// Each run's deepest explored segment of `start`'s local Commits not
/// absorbed below `below`.
fn segments(
    tx: &Transaction<'_>,
    start: &Line,
    below: i64,
) -> HistoryResult<HashMap<CommitId, Segment>> {
    let mut out = HashMap::new();
    if let Line::Commit(line) = start {
        lineage::explore(tx, line, below, |segment| {
            out.insert(segment.run, segment);
            Ok(false)
        })?;
    }
    Ok(out)
}

/// The newest common ancestor of two nodes.
///
/// Let `F` be the older of the two sides' newest Layers and `S` the side
/// whose newest Layer it is. Their common ancestry is `F`'s ancestry plus the
/// local Commits of `S` not absorbed by `F` that the other side contains;
/// within one run those form a range, so only its top can be maximal. The
/// candidates are those tops and `F`; the maximal ones are kept, and among
/// several (criss-cross) the best [`Line::rank`] wins.
pub(crate) fn merge_base(
    tx: &Transaction<'_>,
    left: NodeId,
    right: NodeId,
) -> HistoryResult<Option<NodeId>> {
    if left == right {
        return Ok(Some(left));
    }
    let (l, r) = (Line::read(tx, left)?, Line::read(tx, right)?);
    if l.stack() != r.stack() {
        return Ok(None);
    }
    let stack = l.stack();
    let (small, other) = if l.newest() <= r.newest() {
        (&l, &r)
    } else {
        (&r, &l)
    };
    let floor = small.newest();
    let theirs = segments(tx, other, other.newest() + 1)?;
    let mut candidates = vec![NodeId::Layer(lineage::layer_at(tx, stack, floor)?)];
    for segment in segments(tx, small, floor + 1)?.values() {
        let held = lineage::absorbed_below(tx, segment.run, other.newest() + 1)?
            .max(theirs.get(&segment.run).map_or(0, |s| s.hi));
        let top = segment.hi.min(held);
        if top > segment.lo {
            if let Some(commit) = lineage::commit_at(tx, segment.run, top)? {
                candidates.push(NodeId::Commit(commit));
            }
        }
    }
    let lines = candidates
        .iter()
        .map(|node| Line::read(tx, *node))
        .collect::<HistoryResult<Vec<_>>>()?;
    let mut best: Option<(usize, _)> = None;
    'candidates: for (index, line) in lines.iter().enumerate() {
        for (other_index, other_line) in lines.iter().enumerate() {
            if other_index != index && contains(tx, line, other_line)? {
                continue 'candidates;
            }
        }
        let rank = line.rank(candidates[index]);
        if best.as_ref().is_none_or(|(_, best)| rank > *best) {
            best = Some((index, rank));
        }
    }
    Ok(best.map(|(index, _)| candidates[index]))
}
