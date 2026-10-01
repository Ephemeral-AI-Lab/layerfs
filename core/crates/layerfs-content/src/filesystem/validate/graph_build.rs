//! One selected effective adjacency construction; no resident node frontier.
use super::binding::{Bindings, Headers};
use super::effective::EffectiveEntries;
use super::facts::ValidationState;
use super::{walk_limit, FilesystemTopology, ValidationWork};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::rows::PreparedBindingRows;
use crate::filesystem::state::{
    EffectiveGraphState, GraphAdjacencySeal, GraphBuildAck, GraphMode, GraphNode, GraphNodeKey,
    GraphScope, GraphTotals,
};
use crate::object::{inode_leaf::InodeKind, AuthenticatedObjects};
use std::collections::BTreeMap;

pub(super) struct GraphInput<'a> {
    pub(super) reader: &'a dyn AuthenticatedObjects,
    pub(super) input: &'a dyn PreparedBindingRows,
    pub(super) topology: FilesystemTopology,
    pub(super) unreachable: &'a BTreeMap<u64, ()>,
}
pub(super) fn build<S: EffectiveGraphState + ?Sized>(
    context: GraphInput<'_>,
    work: &mut ValidationWork,
    facts: &mut ValidationState,
    state: &mut S,
    scope: &GraphScope,
) -> ContentResult<GraphAdjacencySeal> {
    let GraphInput {
        reader,
        input,
        topology,
        unreachable,
    } = context;
    if state.graph_capacity(scope)? != scope.capacity() {
        return Err(bad("graph admitted capacity"));
    }
    let mut totals = GraphTotals::default();
    if scope.subject().mode() == GraphMode::Fresh {
        let ack = state.graph_root(scope)?;
        verify_enrollment(
            scope,
            totals,
            &ack,
            &[GraphNodeKey::new(scope, input.root_serial())?],
            false,
        )?;
        totals = ack.after();
        charge_totals(work, totals);
    } else {
        let mut pending = Vec::new();
        pending
            .try_reserve_exact(128)
            .map_err(|_| ContentError::ResourceUnavailable {
                what: "graph.seed_window",
            })?;
        if pending.capacity() > 128 {
            return Err(bad("graph seed capacity"));
        }
        let mut headers = Headers::new(input)?;
        while let Some(header) = headers.next()? {
            let mut bindings = Bindings::new(input, header)?;
            while let Some((_, child)) = bindings.next()? {
                let Some(child) = child else {
                    continue;
                };
                if directory(reader, input, topology, facts, child, work)? {
                    work.graph.seed_occurrences = work.graph.seed_occurrences.saturating_add(1);
                    pending.push(GraphNodeKey::new(scope, child)?);
                    if pending.len() == 128 {
                        enroll(state, scope, work, &mut totals, &mut pending)?;
                    }
                }
            }
        }
        enroll(state, scope, work, &mut totals, &mut pending)?;
        drop(pending);
    }
    let mut visited = 0;
    let mut expanded = 0u64;
    while let Some(mut parent) = state.graph_unexpanded(scope)? {
        GraphNode::decode(scope, &parent.encode())?;
        if expanded >= totals.nodes() {
            return Err(bad("graph unexpanded progress"));
        }
        if parent.expanded() {
            return Err(bad("graph selected expanded"));
        }
        let serial = parent.key().serial();
        let mut self_loop = false;
        if !(scope.subject().mode() == GraphMode::Fresh && unreachable.contains_key(&serial)) {
            let base = match topology.table {
                Some(table) => facts.lookup_optional(reader, table, serial, work)?,
                None => None,
            };
            if base.is_some_and(|value| value.kind != InodeKind::Directory) {
                return Err(ContentError::InvalidRecord("directory child kind"));
            }
            let mut entries =
                EffectiveEntries::new(reader, input, serial, base.map(|value| value.content_root))?;
            let mut children = Vec::new();
            children
                .try_reserve_exact(63)
                .map_err(|_| ContentError::ResourceUnavailable {
                    what: "graph.children_window",
                })?;
            if children.capacity() > 63 {
                return Err(bad("graph children capacity"));
            }
            while let Some((_, child)) = entries.next(&mut visited, walk_limit(input), work)? {
                work.graph.source_entries = work.graph.source_entries.saturating_add(1);
                if scope.subject().mode() == GraphMode::Fresh {
                    work.entries_examined = work.entries_examined.saturating_add(1);
                }
                if directory(reader, input, topology, facts, child, work)? {
                    self_loop |= child == serial;
                    work.graph.raw_directory_edges =
                        work.graph.raw_directory_edges.saturating_add(1);
                    children.push(GraphNodeKey::new(scope, child)?);
                    if children.len() == 63 {
                        append(state, scope, work, &mut totals, &mut parent, &mut children)?;
                    }
                }
            }
            append(state, scope, work, &mut totals, &mut parent, &mut children)?;
        }
        let expected = parent.finish_expansion(scope, self_loop)?;
        let acknowledged = state.graph_expanded(scope, &parent)?;
        if acknowledged != expected {
            return Err(bad("graph expansion acknowledgement"));
        }
        expanded = expanded
            .checked_add(1)
            .ok_or(ContentError::LengthOverflow)?;
        work.graph.node_expansions = work.graph.node_expansions.saturating_add(1);
    }
    if expanded != totals.nodes() {
        return Err(bad("graph incomplete expansion"));
    }
    let seal = state.graph_seal(scope)?;
    if seal.scope() != scope
        || seal.nodes() != totals.nodes()
        || seal.edges() != totals.edges()
        || seal.node_bytes() != totals.node_bytes()
        || seal.edge_bytes() != totals.edge_bytes()
    {
        return Err(bad("graph construction seal totals"));
    }
    work.graph.nodes = totals.nodes();
    work.graph.edges = totals.edges();
    work.graph.multiplicity = totals.multiplicity();
    Ok(seal)
}
fn directory(
    reader: &dyn AuthenticatedObjects,
    input: &dyn PreparedBindingRows,
    topology: FilesystemTopology,
    facts: &mut ValidationState,
    serial: u64,
    work: &mut ValidationWork,
) -> ContentResult<bool> {
    let stored = match topology.table {
        Some(table) => facts.lookup_optional(reader, table, serial, work)?,
        None => None,
    };
    let value = match stored {
        Some(value) => Some(value),
        None => input.value_for(serial)?,
    };
    Ok(value.is_some_and(|value| value.kind == InodeKind::Directory))
}
fn enroll<S: EffectiveGraphState + ?Sized>(
    state: &mut S,
    scope: &GraphScope,
    work: &mut ValidationWork,
    totals: &mut GraphTotals,
    pending: &mut Vec<GraphNodeKey>,
) -> ContentResult<()> {
    if pending.is_empty() {
        return Ok(());
    }
    pending.sort_unstable();
    pending.dedup();
    work.graph.seed_batches = work.graph.seed_batches.saturating_add(1);
    let ack = state.graph_seed_batch(scope, pending)?;
    verify_enrollment(scope, *totals, &ack, pending, true)?;
    work.graph.max_targets = work.graph.max_targets.max(ack.nodes().len() as u64);
    *totals = ack.after();
    charge_totals(work, *totals);
    pending.clear();
    Ok(())
}
fn verify_enrollment(
    scope: &GraphScope,
    totals: GraphTotals,
    ack: &GraphBuildAck,
    seeds: &[GraphNodeKey],
    seed: bool,
) -> ContentResult<()> {
    if ack.scope() != scope
        || ack.before() != totals
        || !ack.edges().is_empty()
        || ack.parent().is_some()
        || ack.nodes().len() != seeds.len()
        || ack.after().multiplicity() != totals.multiplicity()
    {
        return Err(bad("graph enrollment acknowledgement"));
    }
    for (key, change) in seeds.iter().zip(ack.nodes()) {
        if change.after().key() != *key {
            return Err(bad("graph enrollment key"));
        }
        let expected = match change.before() {
            Some(before) if seed => before.seeded(scope)?,
            Some(_) => return Err(bad("graph duplicate fresh root")),
            None => GraphNode::birth(scope, key.serial(), seed)?,
        };
        if change.after() != expected {
            return Err(bad("graph enrollment proposal"));
        }
    }
    Ok(())
}
fn append<S: EffectiveGraphState + ?Sized>(
    state: &mut S,
    scope: &GraphScope,
    work: &mut ValidationWork,
    totals: &mut GraphTotals,
    parent: &mut GraphNode,
    children: &mut Vec<GraphNodeKey>,
) -> ContentResult<()> {
    if children.is_empty() {
        return Ok(());
    }
    work.graph.append_batches = work.graph.append_batches.saturating_add(1);
    work.graph.max_children = work.graph.max_children.max(children.len() as u64);
    let ack = state.graph_append(scope, parent, children)?;
    verify_append(scope, *totals, *parent, children, &ack)?;
    work.graph.max_targets = work
        .graph
        .max_targets
        .max((ack.nodes().len() + ack.edges().len()) as u64);
    *totals = ack.after();
    charge_totals(work, *totals);
    *parent = ack.parent().ok_or(bad("graph append missing parent"))?;
    children.clear();
    Ok(())
}
fn verify_append(
    scope: &GraphScope,
    totals: GraphTotals,
    parent: GraphNode,
    children: &[GraphNodeKey],
    ack: &GraphBuildAck,
) -> ContentResult<()> {
    if ack.scope() != scope
        || ack.before() != totals
        || ack.after().multiplicity()
            != totals
                .multiplicity()
                .checked_add(children.len() as u64)
                .ok_or(ContentError::LengthOverflow)?
        || ack.parent().is_none()
    {
        return Err(bad("graph append totals"));
    }
    let mut distinct = 0usize;
    for (i, key) in children.iter().enumerate() {
        if !children[..i].contains(key) {
            distinct += 1;
        }
    }
    let expected_nodes = distinct + usize::from(!children.contains(&parent.key()));
    if ack.nodes().len() != expected_nodes || ack.edges().len() != distinct {
        return Err(bad("graph append targets"));
    }
    for change in ack.nodes() {
        let key = change.after().key();
        if key != parent.key() && !children.contains(&key) {
            return Err(bad("graph append unexpected node"));
        }
        let count = children
            .iter()
            .filter(|candidate| **candidate == key)
            .count() as u32;
        let before = if key == parent.key() {
            if change.before() != Some(parent) {
                return Err(bad("graph append parent before"));
            }
            parent
        } else {
            change
                .before()
                .unwrap_or(GraphNode::birth(scope, key.serial(), false)?)
        };
        if change.after() != before.add_incoming(scope, count)? {
            return Err(bad("graph append incoming"));
        }
    }
    for change in ack.edges() {
        let key = change.after().key();
        if key.parent() != parent.key().serial() {
            return Err(bad("graph append arc parent"));
        }
        let child = GraphNodeKey::new(scope, key.child())?;
        let count = children
            .iter()
            .filter(|candidate| **candidate == child)
            .count() as u32;
        if count == 0 {
            return Err(bad("graph append unexpected arc"));
        }
        let expected = match change.before() {
            Some(before) => before.add(scope, count)?,
            None => super::super::state::GraphEdge::new(scope, key.parent(), key.child(), count)?,
        };
        if change.after() != expected {
            return Err(bad("graph append arc multiplicity"));
        }
    }
    if ack
        .nodes()
        .iter()
        .find(|change| change.after().key() == parent.key())
        .map(|change| change.after())
        != ack.parent()
    {
        return Err(bad("graph append returned parent"));
    }
    Ok(())
}
fn bad(reason: &'static str) -> ContentError {
    ContentError::InvalidOrderingRecord(reason)
}

fn charge_totals(work: &mut ValidationWork, totals: GraphTotals) {
    work.graph.nodes = totals.nodes();
    work.graph.edges = totals.edges();
    work.graph.multiplicity = totals.multiplicity();
}
