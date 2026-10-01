//! Native closed DFS transitions; indexed FIRST-edge and exact full-row CAS.

use super::graph_state::{Graph, GraphAttempt, GraphAttemptKind, Solver};
use super::{graph_build, graph_index, graph_write, profile};
use crate::error::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{GraphEdge, GraphMutation, GraphNode, GraphStage};
use rusqlite::Connection;

struct Transition {
    solver: Solver,
    child: Option<GraphNode>,
    edge: Option<GraphEdge>,
    code: u8,
}

fn error() -> StorageError {
    StorageError::Integrity("construction scratch graph closed transition")
}

fn selected(connection: &Connection, graph: &Graph, expected: GraphNode) -> StorageResult<()> {
    if graph_index::get_node(connection, &graph.scope, expected.key())? != Some(expected) {
        return Err(error());
    }
    Ok(())
}

fn first(
    connection: &Connection,
    graph: &Graph,
    parent: GraphNode,
    edge: GraphEdge,
) -> StorageResult<GraphNode> {
    if edge.key().parent() != parent.key().serial()
        || graph_index::first_edge(
            connection,
            &graph.scope,
            parent.key().serial(),
            parent.after_child(),
        )? != Some(edge)
    {
        return Err(error());
    }
    graph_index::get_node(
        connection,
        &graph.scope,
        layerfs_content::filesystem::state::GraphNodeKey::new(&graph.scope, edge.key().child())?,
    )?
    .ok_or_else(error)
}

fn clear_component(solver: &mut Solver) {
    solver.scc_root = 0;
    solver.boundary = 0;
    solver.popped = 0;
    solver.last_stack = 0;
    solver.any_seed = false;
    solver.singleton_loop = false;
}

fn transition(
    connection: &Connection,
    graph: &Graph,
    mut solver: Solver,
    mutation: GraphMutation,
) -> StorageResult<Transition> {
    mutation.validate(&graph.scope)?;
    if solver.scc_root != 0 {
        return Err(error());
    }
    for (before, _) in mutation.changes() {
        if let Some(before) = before {
            selected(connection, graph, before)?;
        }
    }
    let mut child = None;
    let mut receipt = None;
    let code = match mutation {
        GraphMutation::EnterRoot { before, after } => {
            if solver.current != 0
                || solver.root != 0
                || solver.next > graph.totals.nodes()
                || after
                    != before.enter(
                        &graph.scope,
                        solver.next.try_into().map_err(|_| error())?,
                        0,
                    )?
            {
                return Err(error());
            }
            clear_component(&mut solver);
            solver.current = before.key().serial();
            solver.root = solver.current;
            solver.next += 1;
            1
        }
        GraphMutation::Descend {
            parent_before,
            parent_after,
            child_before,
            child_after,
            edge,
        } => {
            let actual = first(connection, graph, parent_before, edge)?;
            if solver.current != parent_before.key().serial()
                || solver.root == 0
                || actual != child_before
                || actual.discovery() != 0
                || solver.next > graph.totals.nodes()
                || parent_after
                    != parent_before.advance(
                        &graph.scope,
                        edge.key().child(),
                        parent_before.lowlink(),
                    )?
                || child_after
                    != child_before.enter(
                        &graph.scope,
                        solver.next.try_into().map_err(|_| error())?,
                        parent_before.key().serial(),
                    )?
            {
                return Err(error());
            }
            solver.current = child_before.key().serial();
            solver.next += 1;
            child = Some(actual);
            receipt = Some(edge);
            2
        }
        GraphMutation::Advance {
            before,
            after,
            edge,
        } => {
            let actual = first(connection, graph, before, edge)?;
            let low = if actual.on_stack() {
                before.lowlink().min(actual.discovery())
            } else {
                before.lowlink()
            };
            if solver.current != before.key().serial()
                || solver.root == 0
                || actual.discovery() == 0
                || after != before.advance(&graph.scope, edge.key().child(), low)?
            {
                return Err(error());
            }
            child = Some(actual);
            receipt = Some(edge);
            3
        }
        GraphMutation::Finish { before, after } => {
            if solver.current != before.key().serial()
                || solver.root == 0
                || graph_index::first_edge(
                    connection,
                    &graph.scope,
                    before.key().serial(),
                    before.after_child(),
                )?
                .is_some()
                || after != before.finish(&graph.scope)?
            {
                return Err(error());
            }
            4
        }
        GraphMutation::Return {
            parent_before,
            parent_after,
            child: key,
        } => {
            let actual = graph_index::get_node(connection, &graph.scope, key)?.ok_or_else(error)?;
            if solver.current != actual.key().serial()
                || solver.root == 0
                || actual.on_stack() && actual.lowlink() == actual.discovery()
                || parent_after != parent_before.returned(&graph.scope, actual)?
            {
                return Err(error());
            }
            solver.current = parent_before.key().serial();
            clear_component(&mut solver);
            child = Some(actual);
            5
        }
        GraphMutation::LeaveRoot { finished_root } => {
            selected(connection, graph, finished_root)?;
            if solver.current != finished_root.key().serial()
                || solver.root != solver.current
                || finished_root.parent() != 0
                || !finished_root.dfs_finished()
                || !finished_root.completed()
            {
                return Err(error());
            }
            solver.current = 0;
            solver.root = 0;
            clear_component(&mut solver);
            child = Some(finished_root);
            6
        }
    };
    Ok(Transition {
        solver,
        child,
        edge: receipt,
        code,
    })
}

pub(crate) fn prepare(
    connection: &Connection,
    graph: &Graph,
    mutations: &[GraphMutation],
) -> StorageResult<GraphAttempt> {
    if graph.stage != GraphStage::Solving {
        return Err(error());
    }
    layerfs_content::filesystem::state::GraphMutationLimit::default()
        .check(&graph.scope, mutations)?;
    let mut attempt = GraphAttempt::new(GraphAttemptKind::Mutation, graph, graph.stage);
    let mut solver = graph.solver;
    for (index, mutation) in mutations.iter().enumerate() {
        let result = transition(connection, graph, solver, *mutation)?;
        solver = result.solver;
        attempt.mutation_codes[index] = result.code;
        attempt.selected_children[index] = result.child;
        attempt.selected_edges[index] = result.edge;
        for (before, after) in mutation.changes() {
            if let (Some(before), Some(after)) = (before, after) {
                if attempt.old_nodes[..attempt.nodes]
                    .iter()
                    .any(|node| node.is_some_and(|node| node.key() == before.key()))
                {
                    return Err(error());
                }
                attempt.node(Some(before), Some(after));
            }
        }
    }
    attempt.logical_items = mutations.len();
    attempt.new_solver = solver;
    Ok(attempt)
}

pub(crate) fn commit(
    connection: &Connection,
    graph: &Graph,
    attempt: &GraphAttempt,
    mutations: &[GraphMutation],
) -> StorageResult<()> {
    crate::sqlite::write::begin_immediate(connection)?;
    let result = (|| {
        graph_index::verify(connection, graph)?;
        graph_build::validate_rows(connection, graph, attempt)?;
        let mut solver = graph.solver;
        for (index, mutation) in mutations.iter().enumerate() {
            let result = transition(connection, graph, solver, *mutation)?;
            if result.child != attempt.selected_children[index]
                || result.edge != attempt.selected_edges[index]
                || result.code != attempt.mutation_codes[index]
            {
                return Err(error());
            }
            solver = result.solver;
        }
        if solver != attempt.new_solver {
            return Err(error());
        }
        graph_write::rows(connection, attempt)?;
        graph_write::solver(connection, solver)?;
        Ok(())
    })();
    profile::finish_write(connection, result)
}
