//! Iterative selective Tarjan; continuation and stack remain native graph rows.
use super::ValidationGraphWork;
use crate::error::{ContentError, ContentResult};
use crate::filesystem::state::{
    EffectiveGraphState, GraphAdjacencySeal, GraphMutation, GraphMutationLimit, GraphNode,
    GraphNodeCursor, GraphNodeKey, GraphPageLimit, GraphPopDisposition,
};

/// Solves one sealed Update graph with selective cyclic-Seed rejection.
/// Native selected rows own continuation/stack; no resident population is built.
pub fn solve_effective_graph<S: EffectiveGraphState + ?Sized>(
    state: &mut S,
    seal: &GraphAdjacencySeal,
    work: &mut ValidationGraphWork,
) -> ContentResult<()> {
    state.graph_select(seal.scope())?;
    let outcome = solve_selected(state, seal, work);
    if outcome.is_err() {
        let _ = state.graph_abandon(seal.scope());
    }
    outcome
}

pub(super) fn solve_selected<S: EffectiveGraphState + ?Sized>(
    state: &mut S,
    seal: &GraphAdjacencySeal,
    work: &mut ValidationGraphWork,
) -> ContentResult<()> {
    state.graph_begin_scc(seal)?;
    let mut roots = GraphNodeCursor::new(seal.clone());
    let mut discovery = 0u32;
    let mut edge_steps = 0u64;
    while !roots.finished() {
        let page = state.graph_node_page(seal, roots.after(), GraphPageLimit::default())?;
        roots.accept(&page, GraphPageLimit::default())?;
        for projection in page.records() {
            let before = node(state, seal, projection.key())?;
            if before.discovery() != 0 {
                continue;
            }
            discovery = discovery
                .checked_add(1)
                .ok_or(ContentError::LengthOverflow)?;
            let after = before.enter(seal.scope(), discovery, 0)?;
            cas(
                state,
                seal,
                GraphMutation::EnterRoot { before, after },
                work,
            )?;
            work.solver_entered = work.solver_entered.saturating_add(1);
            let mut current = after;
            loop {
                let limit = GraphPageLimit::new(1, 65536)?;
                let edges = state.graph_edge_page(
                    seal,
                    current.key().serial(),
                    (current.after_child() != 0).then_some(current.after_child()),
                    limit,
                )?;
                edges.check_limit(limit)?;
                if edges.seal() != seal || edges.parent() != current.key().serial() {
                    return Err(bad("graph solver edge selection"));
                }
                if let Some(edge) = edges.records().first().copied() {
                    if edge.key().child() <= current.after_child() {
                        return Err(bad("graph solver edge continuation"));
                    }
                    edge_steps = edge_steps
                        .checked_add(1)
                        .ok_or(ContentError::LengthOverflow)?;
                    if edge_steps > seal.edges() {
                        return Err(bad("graph solver arc count"));
                    }
                    work.edge_steps = work.edge_steps.saturating_add(1);
                    let child = node(
                        state,
                        seal,
                        GraphNodeKey::new(seal.scope(), edge.key().child())?,
                    )?;
                    if child.discovery() == 0 {
                        discovery = discovery
                            .checked_add(1)
                            .ok_or(ContentError::LengthOverflow)?;
                        let parent_after = current.advance(
                            seal.scope(),
                            child.key().serial(),
                            current.lowlink(),
                        )?;
                        let child_after =
                            child.enter(seal.scope(), discovery, current.key().serial())?;
                        cas(
                            state,
                            seal,
                            GraphMutation::Descend {
                                parent_before: current,
                                parent_after,
                                child_before: child,
                                child_after,
                                edge,
                            },
                            work,
                        )?;
                        work.solver_entered = work.solver_entered.saturating_add(1);
                        current = child_after;
                    } else {
                        if !child.on_stack() && !child.completed() {
                            return Err(bad("graph solver entered child"));
                        }
                        let low = if child.on_stack() {
                            current.lowlink().min(child.discovery())
                        } else {
                            current.lowlink()
                        };
                        let after = current.advance(seal.scope(), child.key().serial(), low)?;
                        cas(
                            state,
                            seal,
                            GraphMutation::Advance {
                                before: current,
                                after,
                                edge,
                            },
                            work,
                        )?;
                        current = after;
                    }
                    continue;
                }
                if !edges.eof() {
                    return Err(bad("graph solver outgoing progress"));
                }
                let finished = current.finish(seal.scope())?;
                cas(
                    state,
                    seal,
                    GraphMutation::Finish {
                        before: current,
                        after: finished,
                    },
                    work,
                )?;
                current = finished;
                if current.lowlink() == current.discovery() {
                    current = pop(state, seal, current, work)?;
                }
                if current.parent() == 0 {
                    cas(
                        state,
                        seal,
                        GraphMutation::LeaveRoot {
                            finished_root: current,
                        },
                        work,
                    )?;
                    break;
                }
                let parent = node(
                    state,
                    seal,
                    GraphNodeKey::new(seal.scope(), current.parent())?,
                )?;
                let after = parent.returned(seal.scope(), current)?;
                cas(
                    state,
                    seal,
                    GraphMutation::Return {
                        parent_before: parent,
                        parent_after: after,
                        child: current.key(),
                    },
                    work,
                )?;
                current = after;
            }
        }
    }
    if edge_steps != seal.edges() {
        return Err(bad("graph solver complete arcs"));
    }
    if u64::from(discovery) != seal.nodes() {
        return Err(bad("graph solver discovery count"));
    }
    Ok(())
}
fn node<S: EffectiveGraphState + ?Sized>(
    state: &mut S,
    seal: &GraphAdjacencySeal,
    key: GraphNodeKey,
) -> ContentResult<GraphNode> {
    let node = state
        .graph_node(seal, key)?
        .ok_or(bad("graph solver missing node"))?;
    GraphNode::decode(seal.scope(), &node.encode())?;
    if node.key() != key {
        return Err(bad("graph solver node selection"));
    }
    Ok(node)
}
fn cas<S: EffectiveGraphState + ?Sized>(
    state: &mut S,
    seal: &GraphAdjacencySeal,
    mutation: GraphMutation,
    work: &mut ValidationGraphWork,
) -> ContentResult<()> {
    GraphMutationLimit::default().check(seal.scope(), &[mutation])?;
    work.cas_batches = work.cas_batches.saturating_add(1);
    let ack = state.graph_cas(seal, &[mutation])?;
    if ack.scope() != seal.scope() || ack.nodes().len() != mutation.affected() {
        return Err(bad("graph solver acknowledgement"));
    }
    for (before, after) in mutation.changes() {
        if let (Some(before), Some(after)) = (before, after) {
            if !ack
                .nodes()
                .iter()
                .any(|row| row.before() == Some(before) && row.after() == after)
            {
                return Err(bad("graph solver acknowledged target"));
            }
        }
    }
    work.max_targets = work.max_targets.max(ack.nodes().len() as u64);
    Ok(())
}
fn pop<S: EffectiveGraphState + ?Sized>(
    state: &mut S,
    seal: &GraphAdjacencySeal,
    root: GraphNode,
    work: &mut ValidationGraphWork,
) -> ContentResult<GraphNode> {
    let mut popped = 0u64;
    let mut any_seed = false;
    let mut singleton_self_loop = false;
    let mut previous = u32::MAX;
    loop {
        work.pop_batches = work.pop_batches.saturating_add(1);
        let ack = state.graph_pop(seal, &root, GraphMutationLimit::default())?;
        if ack.scope() != seal.scope()
            || ack.root() != root.key()
            || ack.component() != root.discovery()
        {
            return Err(bad("graph component selection"));
        }
        for member in ack.members() {
            GraphNode::decode(seal.scope(), &member.encode())?;
            if !member.completed()
                || !member.dfs_finished()
                || member.on_stack()
                || member.lowlink() != root.discovery()
                || member.discovery() >= previous
                || member.discovery() < root.discovery()
            {
                return Err(bad("graph component member proof"));
            }
            previous = member.discovery();
            popped = popped.checked_add(1).ok_or(ContentError::LengthOverflow)?;
            any_seed |= member.seed();
            singleton_self_loop = popped == 1 && member.self_loop();
        }
        if ack.popped() != popped
            || ack.any_seed() != any_seed
            || ack.singleton_self_loop() != singleton_self_loop
        {
            return Err(bad("graph component cumulative proof"));
        }
        work.popped_members = work
            .popped_members
            .saturating_add(ack.members().len() as u64);
        work.max_page_records = work.max_page_records.max(ack.members().len() as u64);
        match ack.disposition() {
            GraphPopDisposition::More => {
                if previous <= root.discovery() {
                    return Err(bad("graph component premature continuation"));
                }
            }
            GraphPopDisposition::Complete | GraphPopDisposition::RejectedCycle => {
                let completed = ack
                    .members()
                    .last()
                    .copied()
                    .ok_or(bad("graph component empty terminal"))?;
                if completed != root.complete(seal.scope(), root.discovery())? {
                    return Err(bad("graph component terminal root"));
                }
                let rejected = any_seed && (popped > 1 || singleton_self_loop);
                if rejected != (ack.disposition() == GraphPopDisposition::RejectedCycle) {
                    return Err(bad("graph component semantic disposition"));
                }
                if rejected {
                    return Err(ContentError::InvalidRecord("effective tree cycle"));
                }
                return Ok(completed);
            }
        }
    }
}
fn bad(reason: &'static str) -> ContentError {
    ContentError::InvalidOrderingRecord(reason)
}
