//! Bounded distinct birth and grouped effective-arc construction plans.

use super::graph_state::{Graph, GraphAttempt, GraphAttemptKind};
use super::{graph_index, graph_write, profile};
use crate::error::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{
    GraphEdge, GraphEdgeKey, GraphMode, GraphNode, GraphNodeKey, GraphStage, GraphTotals,
};
use rusqlite::Connection;

pub(crate) fn check_window(keys: &[GraphNodeKey], maximum: usize) -> StorageResult<()> {
    if keys.len() > maximum {
        return Err(StorageError::CapacityExceeded {
            what: "construction scratch graph source window",
            limit: maximum as u64,
            actual: keys.len() as u64,
        });
    }
    Ok(())
}

fn unique(keys: &[GraphNodeKey], output: &mut [Option<(GraphNodeKey, u32)>; 128]) -> usize {
    let mut count = 0;
    for key in keys {
        match output[..count]
            .iter_mut()
            .find(|entry| entry.as_ref().is_some_and(|(old, _)| old == key))
        {
            Some(entry) => entry.as_mut().unwrap().1 += 1,
            None => {
                output[count] = Some((*key, 1));
                count += 1;
            }
        }
    }
    count
}

pub(crate) fn known_growth(
    graph: &Graph,
    keys: &[GraphNodeKey],
    parent: Option<GraphNode>,
) -> StorageResult<()> {
    check_window(keys, if parent.is_some() { 63 } else { 128 })?;
    for key in keys {
        GraphNodeKey::decode(&graph.scope, key.as_bytes())?;
    }
    if let Some(parent) = parent {
        GraphNode::decode(&graph.scope, &parent.encode())?;
    }
    let mut groups = [None; 128];
    let distinct = unique(keys, &mut groups);
    // Count only membership made certain by acknowledged MAX fields. Exact
    // remaining membership is resolved by bounded selected reads later.
    let new_nodes = groups[..distinct]
        .iter()
        .filter(|entry| {
            graph
                .maximum_node
                .is_none_or(|maximum| entry.unwrap().0 > maximum)
        })
        .count() as u64;
    let mut new_edges = 0;
    if let Some(parent) = parent {
        for entry in &groups[..distinct] {
            let key = entry.unwrap().0;
            let edge = GraphEdgeKey::new(&graph.scope, parent.key().serial(), key.serial())?;
            if graph.maximum_node.is_none_or(|maximum| key > maximum)
                || graph.maximum_edge.is_none_or(|maximum| edge > maximum)
            {
                new_edges += 1;
            }
        }
    }
    graph.scope.capacity().check_growth(
        graph.totals.nodes(),
        graph.totals.edges(),
        new_nodes,
        new_edges,
    )?;
    Ok(())
}

pub(crate) fn seeds(
    connection: &Connection,
    graph: &Graph,
    keys: &[GraphNodeKey],
) -> StorageResult<GraphAttempt> {
    if graph.scope.subject().mode() != GraphMode::Update
        || !matches!(graph.stage, GraphStage::Deferred | GraphStage::Seeding)
    {
        return Err(StorageError::Integrity(
            "construction scratch graph seed phase",
        ));
    }
    known_growth(graph, keys, None)?;
    let mut attempt = GraphAttempt::new(GraphAttemptKind::Seeds, graph, GraphStage::Seeding);
    let mut groups = [None; 128];
    let count = unique(keys, &mut groups);
    let mut added = 0;
    for entry in &groups[..count] {
        let key = entry.unwrap().0;
        let old = graph_index::get_node(connection, &graph.scope, key)?;
        let value = match old {
            Some(value) if value == value.projection() && !value.expanded() => {
                value.seeded(&graph.scope)?
            }
            Some(_) => {
                return Err(StorageError::Integrity(
                    "construction scratch graph seed fields",
                ))
            }
            None => {
                added += 1;
                GraphNode::birth(&graph.scope, key.serial(), true)?
            }
        };
        if old.is_none_or(|old| !old.seed()) {
            attempt.seed_count += 1;
        }
        attempt.maximum_node = Some(attempt.maximum_node.map_or(key, |maximum| maximum.max(key)));
        attempt.node(old, Some(value));
    }
    if attempt.seed_count > graph.declared_seeds {
        return Err(StorageError::CapacityExceeded {
            what: "construction scratch graph seeds",
            limit: graph.declared_seeds,
            actual: attempt.seed_count,
        });
    }
    graph
        .scope
        .capacity()
        .check_growth(graph.totals.nodes(), graph.totals.edges(), added, 0)?;
    attempt.proposed = GraphTotals::new(
        graph.totals.nodes() + added,
        graph.totals.edges(),
        graph.totals.multiplicity(),
    )?;
    Ok(attempt)
}

pub(crate) fn root(graph: &Graph) -> StorageResult<GraphAttempt> {
    if graph.stage != GraphStage::Deferred
        || graph.scope.subject().mode() != GraphMode::Fresh
        || graph.totals != GraphTotals::default()
    {
        return Err(StorageError::Integrity(
            "construction scratch fresh graph root phase",
        ));
    }
    graph.scope.capacity().check_growth(0, 0, 1, 0)?;
    let value = GraphNode::birth(&graph.scope, graph.scope.subject().root_serial(), false)?;
    let mut attempt = GraphAttempt::new(GraphAttemptKind::Root, graph, GraphStage::Expanding);
    attempt.node(None, Some(value));
    attempt.maximum_node = Some(value.key());
    attempt.proposed = GraphTotals::new(1, 0, 0)?;
    Ok(attempt)
}

pub(crate) fn append(
    connection: &Connection,
    graph: &Graph,
    parent: GraphNode,
    keys: &[GraphNodeKey],
) -> StorageResult<GraphAttempt> {
    if graph.stage != GraphStage::Expanding || parent.expanded() || parent != parent.projection() {
        return Err(StorageError::Integrity(
            "construction scratch graph append phase",
        ));
    }
    known_growth(graph, keys, Some(parent))?;
    if graph_index::get_node(connection, &graph.scope, parent.key())? != Some(parent) {
        return Err(StorageError::Integrity(
            "construction scratch graph parent snapshot",
        ));
    }
    let mut attempt = GraphAttempt::new(GraphAttemptKind::Append, graph, graph.stage);
    attempt.node(Some(parent), Some(parent));
    let mut groups = [None; 128];
    let count = unique(keys, &mut groups);
    let mut added_nodes = 0;
    let mut added_edges = 0;
    for entry in &groups[..count] {
        let (key, increment) = entry.unwrap();
        let old = graph_index::get_node(connection, &graph.scope, key)?;
        let value = old.unwrap_or(GraphNode::birth(&graph.scope, key.serial(), false)?);
        if value != value.projection() {
            return Err(StorageError::Integrity(
                "construction scratch graph birth fields",
            ));
        }
        let new = value.add_incoming(&graph.scope, increment)?;
        if key == parent.key() {
            attempt.new_nodes[0] = Some(new);
        } else {
            attempt.node(old, Some(new));
        }
        if old.is_none() {
            added_nodes += 1;
        }
        attempt.maximum_node = Some(attempt.maximum_node.map_or(key, |maximum| maximum.max(key)));
        let edge_key = GraphEdgeKey::new(&graph.scope, parent.key().serial(), key.serial())?;
        let old_edge = graph_index::get_edge(connection, &graph.scope, edge_key)?;
        let new_edge = match old_edge {
            Some(value) => value.add(&graph.scope, increment)?,
            None => {
                added_edges += 1;
                GraphEdge::new(&graph.scope, parent.key().serial(), key.serial(), increment)?
            }
        };
        attempt.edge(old_edge, Some(new_edge));
        attempt.maximum_edge = Some(
            attempt
                .maximum_edge
                .map_or(edge_key, |maximum| maximum.max(edge_key)),
        );
    }
    if attempt.nodes + attempt.edges > 128 {
        return Err(StorageError::Integrity(
            "construction scratch graph affected window",
        ));
    }
    graph.scope.capacity().check_growth(
        graph.totals.nodes(),
        graph.totals.edges(),
        added_nodes,
        added_edges,
    )?;
    attempt.proposed = GraphTotals::new(
        graph.totals.nodes() + added_nodes,
        graph.totals.edges() + added_edges,
        graph
            .totals
            .multiplicity()
            .checked_add(keys.len() as u64)
            .ok_or(StorageError::Integrity(
                "construction scratch graph multiplicity overflow",
            ))?,
    )?;
    Ok(attempt)
}

pub(crate) fn expanded(
    connection: &Connection,
    graph: &Graph,
    parent: GraphNode,
) -> StorageResult<GraphAttempt> {
    if graph.stage != GraphStage::Expanding
        || parent.expanded()
        || parent != parent.projection()
        || graph_index::get_node(connection, &graph.scope, parent.key())? != Some(parent)
    {
        return Err(StorageError::Integrity(
            "construction scratch graph expansion snapshot",
        ));
    }
    let edge = GraphEdgeKey::new(&graph.scope, parent.key().serial(), parent.key().serial())?;
    let loop_present = graph_index::get_edge(connection, &graph.scope, edge)?.is_some();
    let value = parent.finish_expansion(&graph.scope, loop_present)?;
    let mut attempt = GraphAttempt::new(GraphAttemptKind::Expanded, graph, graph.stage);
    attempt.node(Some(parent), Some(value));
    Ok(attempt)
}

pub(crate) fn validate_rows(
    connection: &Connection,
    graph: &Graph,
    attempt: &GraphAttempt,
) -> StorageResult<()> {
    for index in 0..attempt.nodes {
        let key = attempt.old_nodes[index]
            .or(attempt.new_nodes[index])
            .ok_or(StorageError::Integrity(
                "construction scratch graph node target",
            ))?
            .key();
        if graph_index::get_node(connection, &graph.scope, key)? != attempt.old_nodes[index] {
            return Err(StorageError::Integrity(
                "construction scratch graph exact old node",
            ));
        }
    }
    for index in 0..attempt.edges {
        let key = attempt.old_edges[index]
            .or(attempt.new_edges[index])
            .ok_or(StorageError::Integrity(
                "construction scratch graph edge target",
            ))?
            .key();
        if graph_index::get_edge(connection, &graph.scope, key)? != attempt.old_edges[index] {
            return Err(StorageError::Integrity(
                "construction scratch graph exact old edge",
            ));
        }
    }
    Ok(())
}

pub(crate) fn commit(
    connection: &Connection,
    graph: &Graph,
    attempt: &GraphAttempt,
) -> StorageResult<()> {
    crate::sqlite::write::begin_immediate(connection)?;
    let result = (|| {
        graph_index::verify(connection, graph)?;
        validate_rows(connection, graph, attempt)?;
        graph_write::rows(connection, attempt)?;
        graph_write::totals(connection, attempt, attempt.seed_count)?;
        Ok(())
    })();
    profile::finish_write(connection, result)
}
