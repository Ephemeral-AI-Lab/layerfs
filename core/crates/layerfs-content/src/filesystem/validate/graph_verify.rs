//! Independent complete stream folds and fresh selected declaration facts.
use super::ValidationWork;
use crate::error::{ContentError, ContentResult};
use crate::filesystem::rows::PreparedBindingRows;
use crate::filesystem::state::{
    EffectiveGraphState, EligibilityView, GraphAdjacencySeal, GraphEdgeCursor, GraphEdgeLedger,
    GraphNodeCursor, GraphNodeKey, GraphPageLimit, GraphProofCursor, GraphProofSeal, ParentCalls,
};
use crate::object::inode_leaf::InodeKind;
pub(super) fn adjacency<S: EffectiveGraphState + ?Sized>(
    state: &mut S,
    seal: &GraphAdjacencySeal,
    work: &mut ValidationWork,
) -> ContentResult<()> {
    let mut nodes = GraphNodeCursor::new(seal.clone());
    let mut edges = GraphEdgeLedger::new(seal.scope().clone());
    while !nodes.finished() {
        let page = state.graph_node_page(seal, nodes.after(), GraphPageLimit::default())?;
        nodes.accept(&page, GraphPageLimit::default())?;
        work.graph.max_page_records = work.graph.max_page_records.max(page.records().len() as u64);
        for node in page.records() {
            if !node.expanded() {
                return Err(bad("graph adjacency unfinished node"));
            }
            let mut cursor = GraphEdgeCursor::new(seal.clone(), node.key().serial())?;
            while !cursor.finished() {
                let page = state.graph_edge_page(
                    seal,
                    node.key().serial(),
                    cursor.after(),
                    GraphPageLimit::default(),
                )?;
                cursor.accept(&page, GraphPageLimit::default())?;
                edges.acknowledge(page.records())?;
                work.graph.max_page_records =
                    work.graph.max_page_records.max(page.records().len() as u64);
            }
        }
    }
    if edges.records() != seal.edges()
        || edges.digest() != *seal.edge_digest()
        || edges.multiplicity() != work.graph.multiplicity
    {
        return Err(bad("graph adjacency edge transcript"));
    }
    Ok(())
}
pub(super) fn fresh_with<S: EffectiveGraphState + ?Sized>(
    input: &dyn PreparedBindingRows,
    unreachable: EligibilityView<'_>,
    state: &mut S,
    seal: &GraphAdjacencySeal,
    parents: ParentCalls<S>,
) -> ContentResult<()> {
    for presence in [false, true] {
        let mut serials = input.new_inodes()?;
        while let Some(serial) = serials.next_row()? {
            if serial == input.root_serial()
                || unreachable.contains(state, serial, parents)?
                || !input
                    .value_for(serial)?
                    .is_some_and(|value| value.kind == InodeKind::Directory)
            {
                continue;
            }
            let key = GraphNodeKey::new(seal.scope(), serial)?;
            let node = state.graph_node(seal, key)?;
            if node.is_some_and(|node| node.key() != key) {
                return Err(bad("graph fresh selected node"));
            }
            if presence {
                if !node.is_some_and(|node| node.root_reached()) {
                    return Err(ContentError::InvalidRecord("effective tree cycle"));
                }
            } else if node.is_some_and(|node| node.incoming() > 1) {
                return Err(ContentError::InvalidRecord("multiple parents"));
            }
        }
    }
    Ok(())
}
pub(super) fn proof<S: EffectiveGraphState + ?Sized>(
    state: &mut S,
    adjacency: &GraphAdjacencySeal,
    work: &mut ValidationWork,
) -> ContentResult<GraphProofSeal> {
    let seal = state.graph_finish(adjacency)?;
    if seal.adjacency() != adjacency {
        return Err(bad("graph final adjacency"));
    }
    let mut cursor = GraphProofCursor::new(seal.clone());
    while !cursor.finished() {
        let page = state.graph_proof_page(&seal, cursor.after(), GraphPageLimit::default())?;
        cursor.accept(&page, GraphPageLimit::default())?;
        work.graph.proof_pages = work.graph.proof_pages.saturating_add(1);
        work.graph.max_page_records = work.graph.max_page_records.max(page.records().len() as u64);
    }
    Ok(seal)
}
fn bad(reason: &'static str) -> ContentError {
    ContentError::InvalidOrderingRecord(reason)
}
