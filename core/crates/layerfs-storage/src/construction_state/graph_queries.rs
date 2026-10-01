//! Seal-selected live rows and bounded immutable MAX-based graph pages.

use super::graph_state::Graph;
use super::{graph_index, ScratchSession};
use crate::error::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{
    GraphAdjacencySeal, GraphEdgeKey, GraphEdgePage, GraphNode, GraphNodeKey, GraphNodePage,
    GraphPageLimit, GraphProofPage, GraphProofSeal,
};
use rusqlite::Connection;

fn node_progress(
    graph: &Graph,
    after: Option<GraphNodeKey>,
    count: usize,
    limit: GraphPageLimit,
    header: usize,
) -> StorageResult<bool> {
    if let Some(key) = after {
        GraphNodeKey::decode(&graph.scope, key.as_bytes())?;
    }
    if after
        .zip(graph.maximum_node)
        .is_some_and(|(key, maximum)| key > maximum)
        || graph.maximum_node.is_none() && after.is_some()
    {
        return Err(StorageError::Integrity(
            "construction scratch graph selected node cursor",
        ));
    }
    let eof = after == graph.maximum_node;
    if !eof && count == 0 {
        limit.check(1, 1, header, 60)?;
    }
    limit.check(0, 0, header, 60)?;
    Ok(eof)
}

fn nodes(
    connection: &Connection,
    graph: &Graph,
    after: Option<GraphNodeKey>,
    count: usize,
    known_eof: bool,
) -> StorageResult<(Vec<GraphNode>, bool)> {
    let rows = graph_index::read_nodes(
        connection,
        &graph.scope,
        after,
        if known_eof { 0 } else { count },
    )?;
    let last = rows.last().map(|node| node.key()).or(after);
    let eof = last == graph.maximum_node;
    if rows.is_empty() && !eof {
        return Err(StorageError::Integrity(
            "construction scratch graph node page progress",
        ));
    }
    Ok((rows, eof))
}

pub(crate) fn edge_maximum(
    connection: &Connection,
    graph: &Graph,
    parent: u64,
) -> StorageResult<Option<u64>> {
    let mut lower = *GraphEdgeKey::new(&graph.scope, parent, 1)?.as_bytes();
    lower[25..].fill(0);
    let upper = GraphEdgeKey::new(&graph.scope, parent, i64::MAX as u64)?;
    let mut statement = connection.prepare("SELECT key,multiplicity FROM graph_edges WHERE key>?1 AND key<=?2 ORDER BY key DESC LIMIT 1")?;
    let mut rows = statement.query(rusqlite::params![
        lower.as_slice(),
        upper.as_bytes().as_slice()
    ])?;
    rows.next()?
        .map(|row| Ok(graph_index::edge(&graph.scope, row)?.key().child()))
        .transpose()
}

impl ScratchSession {
    /// Current checked mutable node; never substitute an adjacency projection for CAS.
    pub fn graph_node(
        &mut self,
        seal: &GraphAdjacencySeal,
        key: GraphNodeKey,
    ) -> StorageResult<Option<GraphNode>> {
        self.ensure_graph_scope(seal.scope())?;
        let result = (|| {
            let resource = self.graph_resource(seal.scope())?;
            resource.graph.as_ref().unwrap().check_adjacency(seal)?;
            GraphNodeKey::decode(seal.scope(), key.as_bytes())?;
            graph_index::get_node(resource.verify()?, seal.scope(), key)
        })();
        self.finish(result)
    }

    /// Immutable normalized projection page, complete batch validation before return.
    pub fn graph_node_page(
        &mut self,
        seal: &GraphAdjacencySeal,
        after: Option<GraphNodeKey>,
        limit: GraphPageLimit,
    ) -> StorageResult<GraphNodePage> {
        self.ensure_graph_scope(seal.scope())?;
        let result = (|| {
            let resource = self.graph_resource(seal.scope())?;
            let graph = resource.graph.as_ref().unwrap();
            graph.check_adjacency(seal)?;
            let count = limit.fitting_records();
            let known_eof = node_progress(graph, after, count, limit, 318)?;
            let (mut rows, eof) = nodes(resource.verify()?, graph, after, count, known_eof)?;
            for node in &mut rows {
                *node = node.projection();
            }
            let page = GraphNodePage::after(seal.clone(), after, rows, eof)?;
            page.check_limit(limit)?;
            Ok(page)
        })();
        self.finish(result)
    }

    /// Full terminal node proof under its exact immutable seal.
    pub fn graph_proof_page(
        &mut self,
        seal: &GraphProofSeal,
        after: Option<GraphNodeKey>,
        limit: GraphPageLimit,
    ) -> StorageResult<GraphProofPage> {
        self.ensure_graph_scope(seal.scope())?;
        let result = (|| {
            let resource = self.graph_resource(seal.scope())?;
            let graph = resource.graph.as_ref().unwrap();
            graph.check_proof(seal)?;
            let count = limit.fitting_proof_records();
            let known_eof = node_progress(graph, after, count, limit, 350)?;
            let (rows, eof) = nodes(resource.verify()?, graph, after, count, known_eof)?;
            let page = GraphProofPage::after(seal.clone(), after, rows, eof)?;
            page.check_limit(limit)?;
            Ok(page)
        })();
        self.finish(result)
    }

    /// Parent-scoped PK wave with actual acknowledged MAX; no prefix COUNT/rank.
    pub fn graph_edge_page(
        &mut self,
        seal: &GraphAdjacencySeal,
        parent: u64,
        after: Option<u64>,
        limit: GraphPageLimit,
    ) -> StorageResult<GraphEdgePage> {
        self.ensure_graph_scope(seal.scope())?;
        let result = (|| {
            let resource = self.graph_resource(seal.scope())?;
            let graph = resource.graph.as_ref().unwrap();
            graph.check_adjacency(seal)?;
            GraphNodeKey::new(seal.scope(), parent)?;
            if let Some(serial) = after {
                GraphNodeKey::new(seal.scope(), serial)?;
            }
            limit.check(0, 0, 318, 43)?;
            let connection = resource.verify()?;
            let maximum = edge_maximum(connection, graph, parent)?;
            if after.zip(maximum).is_some_and(|(a, m)| a > m)
                || maximum.is_none() && after.is_some()
            {
                return Err(StorageError::Integrity(
                    "construction scratch graph selected edge cursor",
                ));
            }
            let known_eof = after == maximum;
            let count = if known_eof { 0 } else { limit.fitting_edges() };
            if !known_eof && count == 0 {
                limit.check(1, 1, 318, 43)?;
            }
            let mut records = graph_index::window(count)?;
            if count != 0 {
                let mut lower = *GraphEdgeKey::new(seal.scope(), parent, 1)?.as_bytes();
                lower[25..].copy_from_slice(&after.unwrap_or(0).to_be_bytes());
                let upper = GraphEdgeKey::new(seal.scope(), parent, maximum.unwrap())?;
                let mut statement = connection.prepare("SELECT key,multiplicity FROM graph_edges WHERE key>?1 AND key<=?2 ORDER BY key LIMIT ?3")?;
                let mut rows = statement.query(rusqlite::params![
                    lower.as_slice(),
                    upper.as_bytes().as_slice(),
                    count as i64
                ])?;
                while let Some(row) = rows.next()? {
                    if records.len() == count {
                        return Err(StorageError::Integrity(
                            "construction scratch graph edge page cardinality",
                        ));
                    }
                    records.push(graph_index::edge(seal.scope(), row)?);
                }
            }
            let eof = records.last().map(|edge| edge.key().child()).or(after) == maximum;
            let page = GraphEdgePage::after(seal.clone(), parent, after, maximum, records, eof)?;
            page.check_limit(limit)?;
            Ok(page)
        })();
        self.finish(result)
    }
}
