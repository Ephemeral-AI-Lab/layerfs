//! One-off lineage backfill for a Store written before #353 (REBUILD.md kind
//! 2): linear in the history once, at open, in the migrating transaction.
//! Nothing existing is rewritten.

use super::lineage;
use super::transaction::Transaction;
use crate::backend::records::BackendError;
use crate::backend::Transaction as BackendTransaction;
use layerfs_history::identity::{CommitId, LayerId, LayerStackId, NodeId};
use std::collections::HashMap;

const HEIGHTS: &str = "WITH RECURSIVE chain(id, stack, height) AS (\
SELECT layer_id, layer_stack_id, 0 FROM layer WHERE parent_layer_id IS NULL \
UNION ALL SELECT l.layer_id, l.layer_stack_id, c.height + 1 FROM layer l \
JOIN chain c ON l.layer_stack_id = c.stack AND l.parent_layer_id = c.id) \
INSERT INTO layer_lineage (layer_id, layer_stack_id, height, overwrote_count) \
SELECT c.id, c.stack, c.height, length(y.overwrote) / 33 FROM chain c JOIN layer y ON y.layer_id = c.id";

const COMMITS: &str =
    "SELECT commit_id, parent_commit_id, base_layer_id, second_parent_id FROM \"commit\"";

const SOURCES: &str = "SELECT y.layer_stack_id, y.source_commit_id, l.height FROM layer y \
JOIN layer_lineage l ON l.layer_id = y.layer_id WHERE y.source_commit_id IS NOT NULL ORDER BY l.height";

type Parents = (Option<CommitId>, LayerId, Option<NodeId>);

/// Fills the lineage tables: Layer heights down each chain, Commits parents
/// first, then every Layer's absorption in height order, as its publication
/// would have recorded it.
pub(crate) fn backfill_lineage(backend: &BackendTransaction<'_>) -> Result<(), BackendError> {
    let fail = |_| BackendError::Integrity;
    let tx = Transaction::new(backend);
    backend.query(HEIGHTS, vec![])?;
    let mut commits: HashMap<CommitId, Parents> = HashMap::new();
    for row in backend.query(COMMITS, vec![])? {
        let id = CommitId::from_slice(&row.get::<Vec<u8>>(0)?).map_err(fail)?;
        let parent = row
            .get::<Option<Vec<u8>>>(1)?
            .map(|b| CommitId::from_slice(&b))
            .transpose()
            .map_err(fail)?;
        let base = LayerId::from_slice(&row.get::<Vec<u8>>(2)?).map_err(fail)?;
        let second = row
            .get::<Option<Vec<u8>>>(3)?
            .map(|b| NodeId::from_slice(&b))
            .transpose()
            .map_err(fail)?;
        commits.insert(id, (parent, base, second));
    }
    let mut done: std::collections::HashSet<CommitId> = std::collections::HashSet::new();
    let mut ids: Vec<CommitId> = commits.keys().copied().collect();
    ids.sort_by(|a, b| a.as_slice().cmp(b.as_slice()));
    for id in ids {
        // Depth first, parents before children, without recursion.
        let mut stack = vec![id];
        while let Some(&top) = stack.last() {
            if done.contains(&top) {
                stack.pop();
                continue;
            }
            let (parent, base, second) = commits[&top];
            let waiting: Vec<CommitId> = [
                parent,
                second.and_then(|n| match n {
                    NodeId::Commit(c) => Some(c),
                    NodeId::Layer(_) => None,
                }),
            ]
            .into_iter()
            .flatten()
            .filter(|c| !done.contains(c))
            .collect();
            if waiting.is_empty() {
                lineage::record_parents(&tx, top, parent, base, second).map_err(fail)?;
                done.insert(top);
                stack.pop();
            } else {
                stack.extend(waiting);
            }
        }
    }
    for row in backend.query(SOURCES, vec![])? {
        let stack = LayerStackId::from_slice(&row.get::<Vec<u8>>(0)?).map_err(fail)?;
        let source = CommitId::from_slice(&row.get::<Vec<u8>>(1)?).map_err(fail)?;
        let line = lineage::commit_line(&tx, source).map_err(fail)?;
        lineage::absorb(&tx, stack, &line, row.get::<i64>(2)?).map_err(fail)?;
    }
    Ok(())
}
