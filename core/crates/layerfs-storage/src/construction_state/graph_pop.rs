//! Indexed descending SCC completion with bounded member acknowledgement.

use super::graph_state::{Graph, GraphAttempt, GraphAttemptKind};
use super::{graph_build, graph_index, graph_write, profile};
use crate::error::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{
    GraphMutationLimit, GraphNode, GraphPopAck, GraphPopDisposition, GraphPopTotals, GraphStage,
};
use rusqlite::Connection;

pub(crate) fn prepare(
    connection: &Connection,
    graph: &Graph,
    root: GraphNode,
    limit: GraphMutationLimit,
) -> StorageResult<(GraphAttempt, GraphPopAck)> {
    if graph.stage != GraphStage::Solving
        || graph.solver.current != root.key().serial()
        || !root.on_stack()
        || !root.dfs_finished()
        || root.lowlink() != root.discovery()
        || root.discovery() == 0
        || graph.solver.scc_root != 0 && graph.solver.scc_root != root.key().serial()
        || graph_index::get_node(connection, &graph.scope, root.key())? != Some(root)
    {
        return Err(StorageError::Integrity(
            "construction scratch graph selected SCC root",
        ));
    }
    let count = limit.records().min(limit.bytes().saturating_sub(350) / 60);
    if count == 0 {
        return Err(StorageError::CapacityExceeded {
            what: "construction scratch graph pop bytes",
            limit: limit.bytes() as u64,
            actual: 410,
        });
    }
    let mut records = graph_index::window(count)?;
    let mut statement = connection.prepare(graph_index::STACK)?;
    let mut rows = statement.query(rusqlite::params![i64::from(root.discovery()), count as i64])?;
    let mut previous = if graph.solver.scc_root == 0 {
        u64::from(u32::MAX) + 1
    } else {
        u64::from(graph.solver.last_stack)
    };
    let mut attempt = GraphAttempt::new(GraphAttemptKind::Pop, graph, graph.stage);
    attempt.selected_children[0] = Some(root);
    attempt.logical_items = 1;
    let mut solver = graph.solver;
    if solver.scc_root == 0 {
        solver.scc_root = root.key().serial();
        solver.boundary = root.discovery();
        solver.popped = 0;
        solver.last_stack = 0;
        solver.any_seed = false;
        solver.singleton_loop = false;
    }
    if solver.boundary != root.discovery() {
        return Err(StorageError::Integrity(
            "construction scratch graph SCC boundary",
        ));
    }
    while let Some(row) = rows.next()? {
        let old = graph_index::node(&graph.scope, row)?;
        if records.len() == count
            || !old.on_stack()
            || !old.dfs_finished()
            || u64::from(old.discovery()) >= previous
            || old.discovery() < solver.boundary
        {
            return Err(StorageError::Integrity(
                "construction scratch graph stack order/member",
            ));
        }
        previous = u64::from(old.discovery());
        let new = old.complete(&graph.scope, solver.boundary)?;
        attempt.node(Some(old), Some(new));
        records.push(new);
        solver.popped = solver.popped.checked_add(1).ok_or(StorageError::Integrity(
            "construction scratch graph pop count",
        ))?;
        solver.any_seed |= old.seed();
        solver.singleton_loop = solver.popped == 1 && old.self_loop();
        solver.last_stack = old.discovery();
    }
    drop(rows);
    drop(statement);
    let complete = records
        .last()
        .is_some_and(|node| node.key() == root.key() && node.discovery() == root.discovery());
    if records.is_empty() || !complete && records.len() != count {
        return Err(StorageError::Integrity(
            "construction scratch graph SCC exact EOF",
        ));
    }
    let rejected = complete && solver.any_seed && (solver.popped > 1 || solver.singleton_loop);
    let disposition = if rejected {
        GraphPopDisposition::RejectedCycle
    } else if complete {
        GraphPopDisposition::Complete
    } else {
        GraphPopDisposition::More
    };
    let ack = GraphPopAck::new(
        graph.scope.clone(),
        root.key(),
        solver.boundary,
        records,
        GraphPopTotals::new(
            u64::from(solver.popped),
            solver.any_seed,
            solver.singleton_loop,
        )?,
        disposition,
    )?;
    if complete {
        solver.scc_root = 0;
        solver.boundary = 0;
    }
    attempt.new_solver = solver;
    attempt.reject_cycle = rejected;
    if rejected {
        attempt.proposed_stage = GraphStage::Rejected;
    }
    Ok((attempt, ack))
}

pub(crate) fn commit(
    connection: &Connection,
    graph: &Graph,
    attempt: &GraphAttempt,
) -> StorageResult<()> {
    crate::sqlite::write::begin_immediate(connection)?;
    let result = (|| {
        graph_index::verify(connection, graph)?;
        graph_build::validate_rows(connection, graph, attempt)?;
        // Recheck the indexed top against every retained expected member. No
        // insertion rank, prefix COUNT or component-population allocation.
        let root = attempt.selected_children[0].unwrap();
        if graph_index::get_node(connection, &graph.scope, root.key())? != Some(root) {
            return Err(StorageError::Integrity(
                "construction scratch graph changed SCC root",
            ));
        }
        let boundary = attempt.new_nodes[0].unwrap().lowlink();
        let mut statement = connection.prepare(graph_index::STACK)?;
        let mut rows =
            statement.query(rusqlite::params![i64::from(boundary), attempt.nodes as i64])?;
        for expected in &attempt.old_nodes[..attempt.nodes] {
            let actual = rows.next()?.ok_or(StorageError::Integrity(
                "construction scratch graph changed pop window",
            ))?;
            if Some(graph_index::node(&graph.scope, actual)?) != *expected {
                return Err(StorageError::Integrity(
                    "construction scratch graph changed stack top",
                ));
            }
        }
        if rows.next()?.is_some() {
            return Err(StorageError::Integrity(
                "construction scratch graph pop cardinality",
            ));
        }
        drop(rows);
        drop(statement);
        graph_write::rows(connection, attempt)?;
        graph_write::solver(connection, attempt.new_solver)?;
        if connection.execute(
            "UPDATE graph_owner SET stage=?1 WHERE id=1 AND stage=4 AND scope=?2",
            rusqlite::params![
                i64::from(attempt.proposed_stage.code()),
                graph.scope.as_bytes().as_slice()
            ],
        )? != 1
        {
            return Err(StorageError::Integrity(
                "construction scratch graph pop stage acknowledgement",
            ));
        }
        Ok(())
    })();
    profile::finish_write(connection, result)
}
