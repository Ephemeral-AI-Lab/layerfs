//! Single-transaction ordered closure and terminal immutable-projection proof.

use super::graph_state::Graph;
use super::{graph_index, profile};
use crate::error::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{
    GraphAdjacencySeal, GraphEdgeKey, GraphEdgeLedger, GraphMode, GraphNodeKey, GraphNodeLedger,
    GraphProofSeal, GraphStage,
};
use rusqlite::{Connection, OptionalExtension};

pub(crate) fn node_maximum(
    connection: &Connection,
    graph: &Graph,
) -> StorageResult<Option<GraphNodeKey>> {
    let mut statement = connection
        .prepare("SELECT key,value,flags,discovery FROM graph_nodes ORDER BY key DESC LIMIT 1")?;
    let mut rows = statement.query([])?;
    rows.next()?
        .map(|row| Ok(graph_index::node(&graph.scope, row)?.key()))
        .transpose()
}

pub(crate) fn edge_maximum(
    connection: &Connection,
    graph: &Graph,
) -> StorageResult<Option<GraphEdgeKey>> {
    let mut statement =
        connection.prepare("SELECT key,multiplicity FROM graph_edges ORDER BY key DESC LIMIT 1")?;
    let mut rows = statement.query([])?;
    rows.next()?
        .map(|row| Ok(graph_index::edge(&graph.scope, row)?.key()))
        .transpose()
}

fn terminal_indexes(connection: &Connection, proof: bool) -> StorageResult<()> {
    let mut queries = [
        Some("SELECT 1 FROM graph_nodes INDEXED BY graph_unexpanded WHERE (flags&2)=0 LIMIT 1"),
        None,
    ];
    if proof {
        queries[1] = Some(
            "SELECT 1 FROM graph_nodes INDEXED BY graph_discovery_stack WHERE (flags&8)=8 LIMIT 1",
        );
    }
    for sql in queries.into_iter().flatten() {
        if connection
            .query_row(sql, [], |_| Ok(()))
            .optional()?
            .is_some()
        {
            return Err(StorageError::Integrity(
                "construction scratch graph unfinished index",
            ));
        }
    }
    Ok(())
}

struct Streams {
    adjacency: GraphAdjacencySeal,
    full: [u8; 32],
}

fn streams(connection: &Connection, graph: &Graph, proof: bool) -> StorageResult<Streams> {
    let maximum_node = node_maximum(connection, graph)?;
    let maximum_edge = edge_maximum(connection, graph)?;
    if maximum_node != graph.maximum_node
        || maximum_edge != graph.maximum_edge
        || maximum_node.is_none() != (graph.totals.nodes() == 0)
        || maximum_edge.is_none() != (graph.totals.edges() == 0)
    {
        return Err(StorageError::Integrity(
            "construction scratch graph exact terminal maxima",
        ));
    }
    terminal_indexes(connection, proof)?;
    let mut projection = GraphNodeLedger::adjacency(graph.scope.clone());
    let mut full = GraphNodeLedger::proof(graph.scope.clone());
    let mut incoming = 0u64;
    while projection.records() < graph.totals.nodes() {
        let count = (graph.totals.nodes() - projection.records()).min(128) as usize;
        let mut rows = graph_index::read_nodes(connection, &graph.scope, projection.last(), count)?;
        if rows.len() != count {
            return Err(StorageError::Integrity(
                "construction scratch graph node cardinality",
            ));
        }
        for node in &rows {
            if !node.expanded()
                || !proof && *node != node.projection()
                || proof
                    && graph.scope.subject().mode() == GraphMode::Update
                    && (!node.completed()
                        || !node.dfs_finished()
                        || node.on_stack()
                        || node.discovery() == 0)
            {
                return Err(StorageError::Integrity(
                    "construction scratch graph terminal node fields",
                ));
            }
            incoming =
                incoming
                    .checked_add(u64::from(node.incoming()))
                    .ok_or(StorageError::Integrity(
                        "construction scratch graph incoming sum",
                    ))?;
        }
        full.acknowledge(&rows)?;
        for node in &mut rows {
            *node = node.projection();
        }
        projection.acknowledge(&rows)?;
    }
    let mut edges = GraphEdgeLedger::new(graph.scope.clone());
    while edges.records() < graph.totals.edges() {
        let count = (graph.totals.edges() - edges.records()).min(128) as usize;
        let rows = graph_index::read_edges(connection, &graph.scope, edges.last(), count)?;
        if rows.len() != count {
            return Err(StorageError::Integrity(
                "construction scratch graph edge cardinality",
            ));
        }
        edges.acknowledge(&rows)?;
    }
    if projection.last() != maximum_node
        || edges.last() != maximum_edge
        || incoming != graph.totals.multiplicity()
        || edges.multiplicity() != graph.totals.multiplicity()
        || projection.encoded_bytes() != graph.totals.node_bytes()
        || edges.encoded_bytes() != graph.totals.edge_bytes()
    {
        return Err(StorageError::Integrity(
            "construction scratch graph closed incoming/multiplicity/EOF",
        ));
    }
    Ok(Streams {
        adjacency: GraphAdjacencySeal::new(
            graph.scope.clone(),
            projection.records(),
            edges.records(),
            projection.digest(),
            edges.digest(),
        )?,
        full: full.digest(),
    })
}

pub(crate) fn adjacency(
    connection: &Connection,
    engine: Option<&'static crate::engine::EngineGuard>,
    graph: &mut Graph,
) -> StorageResult<GraphAdjacencySeal> {
    if graph.stage != GraphStage::Expanding {
        return Err(StorageError::Integrity(
            "construction scratch graph seal phase",
        ));
    }
    if let Some(guard) = engine {
        guard.validate()?;
    }
    crate::sqlite::write::begin_immediate(connection)?;
    let result = (|| {
        graph_index::verify(connection, graph)?;
        let seal = streams(connection, graph, false)?.adjacency;
        graph.proposed_adjacency = Some(seal.clone());
        if connection.execute("UPDATE graph_owner SET stage=3,adjacency_seal=?1 WHERE id=1 AND stage=2 AND scope=?2 AND nodes=?3 AND edges=?4",
            rusqlite::params![seal.encode().as_slice(), graph.scope.as_bytes().as_slice(), graph.totals.nodes() as i64, graph.totals.edges() as i64])? != 1
        { return Err(StorageError::Integrity("construction scratch graph adjacency acknowledgement")); }
        Ok(seal)
    })();
    profile::finish_transaction_guarded(connection, result, engine)
}

pub(crate) fn proof(
    connection: &Connection,
    engine: Option<&'static crate::engine::EngineGuard>,
    graph: &mut Graph,
) -> StorageResult<GraphProofSeal> {
    graph.check_terminal_solver()?;
    if let Some(guard) = engine {
        guard.validate()?;
    }
    crate::sqlite::write::begin_immediate(connection)?;
    let result = (|| {
        graph_index::verify(connection, graph)?;
        let streams = streams(connection, graph, true)?;
        let adjacency = graph.adjacency.as_ref().ok_or(StorageError::Integrity(
            "construction scratch graph missing adjacency",
        ))?;
        if &streams.adjacency != adjacency {
            return Err(StorageError::Integrity(
                "construction scratch graph immutable adjacency projection",
            ));
        }
        let seal = GraphProofSeal::new(adjacency.clone(), streams.full);
        graph.proposed_proof = Some(seal.clone());
        if connection.execute("UPDATE graph_owner SET stage=5,proof_seal=?1,remaining_nodes=?2,remaining_edges=?3 WHERE id=1 AND stage=?4 AND scope=?5 AND adjacency_seal=?6",
            rusqlite::params![seal.encode().as_slice(), graph.totals.nodes() as i64, graph.totals.edges() as i64,
                i64::from(graph.stage.code()), graph.scope.as_bytes().as_slice(), adjacency.encode().as_slice()])? != 1
        { return Err(StorageError::Integrity("construction scratch graph proof acknowledgement")); }
        Ok(seal)
    })();
    profile::finish_transaction_guarded(connection, result, engine)
}
