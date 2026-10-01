//! Bounded exact-key retirement, without refunding the captured native owner.

use super::graph_state::{Graph, GraphAttempt, GraphAttemptKind};
use super::{graph_build, graph_index, graph_write, profile};
use crate::error::{StorageError, StorageResult};
use layerfs_content::filesystem::state::GraphStage;
use rusqlite::Connection;

pub(crate) fn prepare(connection: &Connection, graph: &Graph) -> StorageResult<GraphAttempt> {
    if !matches!(graph.stage, GraphStage::Proved | GraphStage::Retiring) {
        return Err(StorageError::Integrity(
            "construction scratch graph retirement phase",
        ));
    }
    let mut attempt = GraphAttempt::new(GraphAttemptKind::Retire, graph, GraphStage::Retiring)?;
    let node_count = graph.remaining_nodes.min(128) as usize;
    let rows = graph_index::read_nodes(connection, &graph.scope, graph.after_node, node_count)?;
    if rows.len() != node_count {
        return Err(StorageError::Integrity(
            "construction scratch graph retirement node count",
        ));
    }
    for value in rows {
        attempt.proposed_node = Some(value.key());
        attempt.node(Some(value), None)?;
    }
    let edge_count = graph.remaining_edges.min((128 - node_count) as u64) as usize;
    let rows = graph_index::read_edges(connection, &graph.scope, graph.after_edge, edge_count)?;
    if rows.len() != edge_count {
        return Err(StorageError::Integrity(
            "construction scratch graph retirement edge count",
        ));
    }
    for value in rows {
        attempt.proposed_edge = Some(value.key());
        attempt.edge(Some(value), None)?;
    }
    attempt.remaining_nodes.1 -= node_count as u64;
    attempt.remaining_edges.1 -= edge_count as u64;
    if attempt.remaining_nodes.1 == 0 && attempt.remaining_edges.1 == 0 {
        if attempt.proposed_node != graph.maximum_node
            || attempt.proposed_edge != graph.maximum_edge
        {
            return Err(StorageError::Integrity(
                "construction scratch graph retirement exact EOF",
            ));
        }
        attempt.proposed_stage = GraphStage::Retired;
    }
    Ok(attempt)
}

pub(crate) fn commit(
    connection: &Connection,
    engine: Option<&'static crate::engine::EngineGuard>,
    graph: &Graph,
    attempt: &GraphAttempt,
) -> StorageResult<()> {
    if let Some(guard) = engine {
        guard.validate()?;
    }
    crate::sqlite::write::begin_immediate(connection)?;
    let result = (|| {
        graph_index::verify(connection, graph)?;
        graph_build::validate_rows(connection, graph, attempt)?;
        graph_write::rows(connection, attempt)?;
        if attempt.proposed_stage == GraphStage::Retired && !graph_index::empty(connection)? {
            return Err(StorageError::Integrity(
                "construction scratch retired graph tables/indexes remain",
            ));
        }
        if connection.execute("UPDATE graph_owner SET stage=?1,remaining_nodes=?2,remaining_edges=?3,after_node=?4,after_edge=?5 WHERE id=1 AND stage=?6 AND remaining_nodes=?7 AND remaining_edges=?8",
            rusqlite::params![i64::from(attempt.proposed_stage.code()), attempt.remaining_nodes.1 as i64, attempt.remaining_edges.1 as i64,
                attempt.proposed_node.as_ref().map(|key| key.as_bytes().as_slice()), attempt.proposed_edge.as_ref().map(|key| key.as_bytes().as_slice()),
                i64::from(attempt.prior_stage.code()), attempt.remaining_nodes.0 as i64, attempt.remaining_edges.0 as i64])? != 1
        { return Err(StorageError::Integrity("construction scratch graph retirement acknowledgement")); }
        Ok(())
    })();
    profile::finish_write_guarded(connection, result, engine)
}
