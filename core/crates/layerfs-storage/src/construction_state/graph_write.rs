//! Closed conditional graph row writes after whole-window validation.

use super::graph_state::{GraphAttempt, Solver};
use crate::error::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{GraphEdge, GraphNode};
use rusqlite::Connection;

pub(crate) fn node(
    connection: &Connection,
    old: Option<GraphNode>,
    new: Option<GraphNode>,
) -> StorageResult<()> {
    let changed = match (old, new) {
        (None, Some(value)) => connection.execute(
            "INSERT INTO graph_nodes(key,value,flags,discovery) VALUES(?1,?2,?3,?4)",
            rusqlite::params![value.key().as_bytes().as_slice(), value.value().as_slice(), i64::from(value.flags()), i64::from(value.discovery())])?,
        (Some(old), Some(new)) if old != new => connection.execute(
            "UPDATE graph_nodes SET value=?1,flags=?2,discovery=?3 WHERE key=?4 AND value=?5 AND flags=?6 AND discovery=?7",
            rusqlite::params![new.value().as_slice(), i64::from(new.flags()), i64::from(new.discovery()),
                old.key().as_bytes().as_slice(), old.value().as_slice(), i64::from(old.flags()), i64::from(old.discovery())])?,
        (Some(old), None) => connection.execute(
            "DELETE FROM graph_nodes WHERE key=?1 AND value=?2 AND flags=?3 AND discovery=?4",
            rusqlite::params![old.key().as_bytes().as_slice(), old.value().as_slice(), i64::from(old.flags()), i64::from(old.discovery())])?,
        (Some(old), Some(new)) if old == new => return Ok(()),
        _ => return Err(StorageError::Integrity("construction scratch graph node change")),
    };
    if changed != 1 {
        return Err(StorageError::Integrity(
            "construction scratch graph node affected count",
        ));
    }
    Ok(())
}

pub(crate) fn edge(
    connection: &Connection,
    old: Option<GraphEdge>,
    new: Option<GraphEdge>,
) -> StorageResult<()> {
    let changed = match (old, new) {
        (None, Some(value)) => connection.execute(
            "INSERT INTO graph_edges(key,multiplicity) VALUES(?1,?2)",
            rusqlite::params![value.key().as_bytes().as_slice(), value.value().as_slice()],
        )?,
        (Some(old), Some(new)) if old != new => connection.execute(
            "UPDATE graph_edges SET multiplicity=?1 WHERE key=?2 AND multiplicity=?3",
            rusqlite::params![
                new.value().as_slice(),
                old.key().as_bytes().as_slice(),
                old.value().as_slice()
            ],
        )?,
        (Some(old), None) => connection.execute(
            "DELETE FROM graph_edges WHERE key=?1 AND multiplicity=?2",
            rusqlite::params![old.key().as_bytes().as_slice(), old.value().as_slice()],
        )?,
        (Some(old), Some(new)) if old == new => return Ok(()),
        _ => {
            return Err(StorageError::Integrity(
                "construction scratch graph edge change",
            ))
        }
    };
    if changed != 1 {
        return Err(StorageError::Integrity(
            "construction scratch graph edge affected count",
        ));
    }
    Ok(())
}

pub(crate) fn rows(connection: &Connection, attempt: &GraphAttempt) -> StorageResult<()> {
    for index in 0..attempt.nodes {
        node(
            connection,
            attempt.old_nodes[index],
            attempt.new_nodes[index],
        )?;
    }
    for index in 0..attempt.edges {
        edge(
            connection,
            attempt.old_edges[index],
            attempt.new_edges[index],
        )?;
    }
    Ok(())
}

pub(crate) fn solver(connection: &Connection, value: Solver) -> StorageResult<()> {
    if connection.execute("UPDATE solver_owner SET current_serial=?1,dfs_root_serial=?2,next_discovery=?3,scc_root_serial=?4,scc_boundary=?5,cumulative_pop_count=?6,last_stack_discovery=?7 WHERE id=1",
        rusqlite::params![value.current as i64, value.root as i64, value.next as i64, value.scc_root as i64,
            i64::from(value.boundary), i64::from(value.popped), i64::from(value.last_stack)])? != 1
        || connection.execute("UPDATE solver_owner SET any_seed=?1,singleton_self_loop=?2 WHERE id=1",
            rusqlite::params![i64::from(value.any_seed), i64::from(value.singleton_loop)])? != 1
    { return Err(StorageError::Integrity("construction scratch graph solver affected count")); }
    Ok(())
}

pub(crate) fn totals(
    connection: &Connection,
    attempt: &GraphAttempt,
    seed_count: u64,
) -> StorageResult<()> {
    if connection.execute("UPDATE graph_owner SET stage=?1,nodes=?2,edges=?3,record_bytes=?4,source_multiplicity=?5,max_node=?6,max_edge=?7,seed_count=?8 WHERE id=1",
        rusqlite::params![i64::from(attempt.proposed_stage.code()), attempt.proposed.nodes() as i64,
            attempt.proposed.edges() as i64, attempt.proposed.encoded_bytes() as i64,
            attempt.proposed.multiplicity().to_be_bytes().as_slice(),
            attempt.maximum_node.as_ref().map(|key| key.as_bytes().as_slice()),
            attempt.maximum_edge.as_ref().map(|key| key.as_bytes().as_slice()), seed_count as i64])? != 1
    { return Err(StorageError::Integrity("construction scratch graph totals affected count")); }
    Ok(())
}
