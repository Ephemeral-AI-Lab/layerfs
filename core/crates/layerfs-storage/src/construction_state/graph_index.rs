//! Exact v4 owner association and bounded primary-key graph reads.

use layerfs_content::filesystem::state::{
    GraphEdge, GraphEdgeKey, GraphNode, GraphNodeKey, GraphScope,
};
use rusqlite::{types::ValueRef, Connection, OptionalExtension, Row};

use super::graph_state::Graph;
use crate::error::{StorageError, StorageResult};

pub(crate) const NODE_GET: &str = "SELECT key,value,flags,discovery FROM graph_nodes WHERE key=?1";
pub(crate) const NODE_FIRST: &str =
    "SELECT key,value,flags,discovery FROM graph_nodes ORDER BY key LIMIT ?1";
pub(crate) const NODE_AFTER: &str =
    "SELECT key,value,flags,discovery FROM graph_nodes WHERE key>?1 ORDER BY key LIMIT ?2";
pub(crate) const EDGE_GET: &str = "SELECT key,multiplicity FROM graph_edges WHERE key=?1";
pub(crate) const EDGE_FIRST: &str =
    "SELECT key,multiplicity FROM graph_edges ORDER BY key LIMIT ?1";
pub(crate) const EDGE_AFTER: &str =
    "SELECT key,multiplicity FROM graph_edges WHERE key>?1 ORDER BY key LIMIT ?2";
pub(crate) const UNEXPANDED: &str = "SELECT key,value,flags,discovery FROM graph_nodes INDEXED BY graph_unexpanded WHERE (flags&2)=0 ORDER BY key LIMIT 1";
pub(crate) const STACK: &str = "SELECT key,value,flags,discovery FROM graph_nodes INDEXED BY graph_discovery_stack WHERE (flags&8)=8 AND discovery>=?1 ORDER BY discovery DESC LIMIT ?2";

pub(crate) fn blob(value: ValueRef<'_>, width: usize) -> StorageResult<&[u8]> {
    match value {
        ValueRef::Blob(bytes) if bytes.len() == width => Ok(bytes),
        _ => Err(StorageError::Integrity("construction scratch graph BLOB")),
    }
}

pub(crate) fn optional_blob(value: ValueRef<'_>, expected: Option<&[u8]>) -> StorageResult<()> {
    match expected {
        Some(bytes) if blob(value, bytes.len())? == bytes => Ok(()),
        None if matches!(value, ValueRef::Null) => Ok(()),
        _ => Err(StorageError::Integrity(
            "construction scratch graph owner field",
        )),
    }
}

pub(crate) fn node(scope: &GraphScope, row: &Row<'_>) -> StorageResult<GraphNode> {
    let key = GraphNodeKey::decode(scope, blob(row.get_ref(0)?, 25)?)?;
    let value = GraphNode::decode_value(scope, key, blob(row.get_ref(1)?, 29)?)?;
    if row.get::<_, i64>(2)? != i64::from(value.flags())
        || row.get::<_, i64>(3)? != i64::from(value.discovery())
    {
        return Err(StorageError::Integrity(
            "construction scratch graph redundant node",
        ));
    }
    Ok(value)
}

pub(crate) fn edge(scope: &GraphScope, row: &Row<'_>) -> StorageResult<GraphEdge> {
    let key = GraphEdgeKey::decode(scope, blob(row.get_ref(0)?, 33)?)?;
    Ok(GraphEdge::decode_value(
        scope,
        key,
        blob(row.get_ref(1)?, 4)?,
    )?)
}

pub(crate) fn verify(connection: &Connection, graph: &Graph) -> StorageResult<()> {
    let capacity = graph.scope.capacity();
    let declared: (i64, i64, i64, i64) = connection.query_row(
        "SELECT declared_sites,selected_bytes,graph_records,graph_record_bytes FROM session_owner WHERE id=1", [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    if declared
        != (
            graph.declared_seeds as i64,
            capacity.scratch_bytes() as i64,
            capacity.records() as i64,
            capacity.encoded_bytes() as i64,
        )
    {
        return Err(StorageError::Integrity(
            "construction scratch captured graph capacity",
        ));
    }
    let mut statement = connection.prepare("SELECT scope,stage,nodes,edges,record_bytes,source_multiplicity,remaining_nodes,remaining_edges,adjacency_seal,proof_seal,max_node,max_edge,after_node,after_edge,seed_count FROM graph_owner WHERE id=1")?;
    let mut rows = statement.query([])?;
    let row = rows
        .next()?
        .ok_or(StorageError::Integrity("construction scratch graph owner"))?;
    if blob(row.get_ref(0)?, 188)? != graph.scope.as_bytes()
        || row.get::<_, i64>(1)? != i64::from(graph.stage.code())
        || row.get::<_, i64>(2)? != graph.totals.nodes() as i64
        || row.get::<_, i64>(3)? != graph.totals.edges() as i64
        || row.get::<_, i64>(4)? != graph.totals.encoded_bytes() as i64
        || blob(row.get_ref(5)?, 8)? != graph.totals.multiplicity().to_be_bytes()
        || row.get::<_, i64>(6)? != graph.remaining_nodes as i64
        || row.get::<_, i64>(7)? != graph.remaining_edges as i64
        || row.get::<_, i64>(14)? != graph.seed_count as i64
    {
        return Err(StorageError::Integrity(
            "construction scratch exact graph owner",
        ));
    }
    let adjacency = graph.adjacency.as_ref().map(|seal| seal.encode());
    let proof = graph.proof.as_ref().map(|seal| seal.encode());
    optional_blob(
        row.get_ref(8)?,
        adjacency.as_ref().map(|bytes| bytes.as_slice()),
    )?;
    optional_blob(
        row.get_ref(9)?,
        proof.as_ref().map(|bytes| bytes.as_slice()),
    )?;
    optional_blob(
        row.get_ref(10)?,
        graph
            .maximum_node
            .as_ref()
            .map(|key| key.as_bytes().as_slice()),
    )?;
    optional_blob(
        row.get_ref(11)?,
        graph
            .maximum_edge
            .as_ref()
            .map(|key| key.as_bytes().as_slice()),
    )?;
    optional_blob(
        row.get_ref(12)?,
        graph
            .after_node
            .as_ref()
            .map(|key| key.as_bytes().as_slice()),
    )?;
    optional_blob(
        row.get_ref(13)?,
        graph
            .after_edge
            .as_ref()
            .map(|key| key.as_bytes().as_slice()),
    )?;
    let mut statement = connection.prepare("SELECT scope,current_serial,dfs_root_serial,next_discovery,scc_root_serial,scc_boundary,cumulative_pop_count,last_stack_discovery,any_seed,singleton_self_loop FROM solver_owner WHERE id=1")?;
    let mut rows = statement.query([])?;
    let row = rows
        .next()?
        .ok_or(StorageError::Integrity("construction scratch graph solver"))?;
    let solver = graph.solver;
    if blob(row.get_ref(0)?, 188)? != graph.scope.as_bytes()
        || row.get::<_, i64>(1)? != solver.current as i64
        || row.get::<_, i64>(2)? != solver.root as i64
        || row.get::<_, i64>(3)? != solver.next as i64
        || row.get::<_, i64>(4)? != solver.scc_root as i64
        || row.get::<_, i64>(5)? != i64::from(solver.boundary)
        || row.get::<_, i64>(6)? != i64::from(solver.popped)
        || row.get::<_, i64>(7)? != i64::from(solver.last_stack)
        || row.get::<_, i64>(8)? != i64::from(solver.any_seed)
        || row.get::<_, i64>(9)? != i64::from(solver.singleton_loop)
    {
        return Err(StorageError::Integrity(
            "construction scratch exact graph solver",
        ));
    }
    Ok(())
}

pub(crate) fn get_node(
    connection: &Connection,
    scope: &GraphScope,
    key: GraphNodeKey,
) -> StorageResult<Option<GraphNode>> {
    GraphNodeKey::decode(scope, key.as_bytes())?;
    let mut statement = connection.prepare(NODE_GET)?;
    let mut rows = statement.query([key.as_bytes().as_slice()])?;
    rows.next()?.map(|row| node(scope, row)).transpose()
}

pub(crate) fn get_edge(
    connection: &Connection,
    scope: &GraphScope,
    key: GraphEdgeKey,
) -> StorageResult<Option<GraphEdge>> {
    GraphEdgeKey::decode(scope, key.as_bytes())?;
    let mut statement = connection.prepare(EDGE_GET)?;
    let mut rows = statement.query([key.as_bytes().as_slice()])?;
    rows.next()?.map(|row| edge(scope, row)).transpose()
}

pub(crate) fn window<T>(count: usize) -> StorageResult<Vec<T>> {
    if count > 128 {
        return Err(StorageError::Integrity("construction scratch graph window"));
    }
    let mut output = Vec::new();
    output.try_reserve_exact(count).map_err(|_| {
        StorageError::Content(layerfs_content::ContentError::ResourceUnavailable {
            what: "construction scratch graph page",
        })
    })?;
    if output.capacity() > 128 {
        return Err(StorageError::Integrity(
            "construction scratch graph page capacity",
        ));
    }
    Ok(output)
}

pub(crate) fn read_nodes(
    connection: &Connection,
    scope: &GraphScope,
    after: Option<GraphNodeKey>,
    count: usize,
) -> StorageResult<Vec<GraphNode>> {
    let mut output = window(count)?;
    if count == 0 {
        return Ok(output);
    }
    let mut statement = connection.prepare(if after.is_some() {
        NODE_AFTER
    } else {
        NODE_FIRST
    })?;
    let mut rows = match after {
        Some(key) => statement.query(rusqlite::params![key.as_bytes().as_slice(), count as i64])?,
        None => statement.query([count as i64])?,
    };
    let mut last = after;
    while let Some(row) = rows.next()? {
        let value = node(scope, row)?;
        if output.len() == count || last.is_some_and(|key| key >= value.key()) {
            return Err(StorageError::Integrity(
                "construction scratch graph node order",
            ));
        }
        last = Some(value.key());
        output.push(value);
    }
    Ok(output)
}

pub(crate) fn read_edges(
    connection: &Connection,
    scope: &GraphScope,
    after: Option<GraphEdgeKey>,
    count: usize,
) -> StorageResult<Vec<GraphEdge>> {
    let mut output = window(count)?;
    if count == 0 {
        return Ok(output);
    }
    let mut statement = connection.prepare(if after.is_some() {
        EDGE_AFTER
    } else {
        EDGE_FIRST
    })?;
    let mut rows = match after {
        Some(key) => statement.query(rusqlite::params![key.as_bytes().as_slice(), count as i64])?,
        None => statement.query([count as i64])?,
    };
    let mut last = after;
    while let Some(row) = rows.next()? {
        let value = edge(scope, row)?;
        if output.len() == count || last.is_some_and(|key| key >= value.key()) {
            return Err(StorageError::Integrity(
                "construction scratch graph edge order",
            ));
        }
        last = Some(value.key());
        output.push(value);
    }
    Ok(output)
}

pub(crate) fn unexpanded(
    connection: &Connection,
    scope: &GraphScope,
) -> StorageResult<Option<GraphNode>> {
    let mut statement = connection.prepare(UNEXPANDED)?;
    let mut rows = statement.query([])?;
    let value = rows.next()?.map(|row| node(scope, row)).transpose()?;
    if value.is_some_and(|node| node.expanded()) {
        return Err(StorageError::Integrity(
            "construction scratch unexpanded projection",
        ));
    }
    Ok(value)
}

pub(crate) fn first_edge(
    connection: &Connection,
    scope: &GraphScope,
    parent: u64,
    after: u64,
) -> StorageResult<Option<GraphEdge>> {
    let mut lower = *GraphEdgeKey::new(scope, parent, 1)?.as_bytes();
    if after > i64::MAX as u64 {
        return Err(StorageError::Integrity(
            "construction scratch graph edge cursor",
        ));
    }
    lower[25..].copy_from_slice(&after.to_be_bytes());
    let upper = GraphEdgeKey::new(scope, parent, i64::MAX as u64)?;
    let mut statement = connection.prepare(
        "SELECT key,multiplicity FROM graph_edges WHERE key>?1 AND key<=?2 ORDER BY key LIMIT 1",
    )?;
    let mut rows = statement.query(rusqlite::params![
        lower.as_slice(),
        upper.as_bytes().as_slice()
    ])?;
    rows.next()?.map(|row| edge(scope, row)).transpose()
}

pub(crate) fn empty(connection: &Connection) -> StorageResult<bool> {
    for sql in [
        "SELECT 1 FROM graph_nodes LIMIT 1",
        "SELECT 1 FROM graph_edges LIMIT 1",
        "SELECT 1 FROM graph_nodes INDEXED BY graph_unexpanded WHERE (flags&2)=0 LIMIT 1",
        "SELECT 1 FROM graph_nodes INDEXED BY graph_discovery_stack WHERE (flags&8)=8 LIMIT 1",
    ] {
        if connection
            .query_row(sql, [], |_| Ok(()))
            .optional()?
            .is_some()
        {
            return Ok(false);
        }
    }
    Ok(true)
}
