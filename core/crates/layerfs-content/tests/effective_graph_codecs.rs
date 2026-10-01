//! Independent literal private grammar, budgets, windows and stream custody.
#![allow(dead_code)]
#[path = "support/graph_state.rs"]
mod oracle;
mod support;
mod budget {
    include!("fixtures/effective_graph/budget_manifest.rs");
}
use layerfs_content::filesystem::rows::{BindingRows, SliceBindingRows};
use layerfs_content::filesystem::state::*;
use layerfs_content::filesystem::{
    scope_for_seed, DirectoryUpdate, FilesystemInput, FilesystemRootId, InodeUpdate,
};
use layerfs_content::ContentError;
use oracle::scopes;
use support::filesystem::{resources, synthetic};
fn selected(fresh: bool) -> GraphConstructionScopes {
    let directories: Vec<DirectoryUpdate> = Vec::new();
    let values: Vec<InodeUpdate> = Vec::new();
    let new: Vec<u64> = Vec::new();
    let input = FilesystemInput {
        base: (!fresh).then_some(FilesystemRootId(synthetic("base"))),
        scope: scope_for_seed([8; 32]),
        root_serial: 1,
        directories: &directories,
        inodes: &values,
        new_inodes: &new,
        resources: resources(),
    };
    let source = SliceBindingRows::new(&input).unwrap();
    scopes(
        source.binding_source_id().unwrap(),
        input.scope,
        input.base,
        1,
        GraphCapacity::default(),
    )
}
fn context(scope: &GraphScope) -> [u8; 188] {
    let mut bytes = [0; 188];
    let selection = scope.nodes().selection();
    bytes[..32].copy_from_slice(selection.selector());
    bytes[32..40].copy_from_slice(&selection.token().to_be_bytes());
    bytes[40..72].copy_from_slice(selection.owner_binding().unwrap());
    bytes[72..80].copy_from_slice(&2u64.to_be_bytes());
    bytes[80] = 4;
    bytes[81] = 5;
    let subject = scope.subject();
    bytes[82..90].copy_from_slice(&subject.source_id().as_bytes());
    bytes[90..122].copy_from_slice(subject.namespace().object().as_bytes());
    bytes[122] = if subject.base().is_some() { 2 } else { 1 };
    if let Some(base) = subject.base() {
        bytes[123] = 1;
        bytes[124..156].copy_from_slice(base.0.as_bytes());
    }
    bytes[156..164].copy_from_slice(&1u64.to_be_bytes());
    bytes[164..172].copy_from_slice(&16_777_216u64.to_be_bytes());
    bytes[172..180].copy_from_slice(&65_536u64.to_be_bytes());
    bytes[180..188].copy_from_slice(&4_128_768u64.to_be_bytes());
    bytes
}
struct NodeFields {
    flags: u8,
    discovery: u32,
    lowlink: u32,
    parent: u64,
    after: u64,
    incoming: u32,
}
fn node_frame(scope: &GraphScope, serial: u64, fields: NodeFields) -> [u8; 60] {
    let NodeFields {
        flags,
        discovery,
        lowlink,
        parent,
        after,
        incoming,
    } = fields;
    let mut b = [0; 60];
    b[..2].copy_from_slice(&25u16.to_be_bytes());
    b[2..10].copy_from_slice(&scope.nodes().selection().token().to_be_bytes());
    b[10..18].copy_from_slice(&2u64.to_be_bytes());
    b[18] = 4;
    b[19..27].copy_from_slice(&serial.to_be_bytes());
    b[27..31].copy_from_slice(&29u32.to_be_bytes());
    b[31] = flags;
    b[32..36].copy_from_slice(&discovery.to_be_bytes());
    b[36..40].copy_from_slice(&lowlink.to_be_bytes());
    b[40..48].copy_from_slice(&parent.to_be_bytes());
    b[48..56].copy_from_slice(&after.to_be_bytes());
    b[56..60].copy_from_slice(&incoming.to_be_bytes());
    b
}
fn edge_frame(scope: &GraphScope, parent: u64, child: u64, multiplicity: u32) -> [u8; 43] {
    let mut b = [0; 43];
    b[..2].copy_from_slice(&33u16.to_be_bytes());
    b[2..10].copy_from_slice(&scope.nodes().selection().token().to_be_bytes());
    b[10..18].copy_from_slice(&2u64.to_be_bytes());
    b[18] = 5;
    b[19..27].copy_from_slice(&parent.to_be_bytes());
    b[27..35].copy_from_slice(&child.to_be_bytes());
    b[35..39].copy_from_slice(&4u32.to_be_bytes());
    b[39..43].copy_from_slice(&multiplicity.to_be_bytes());
    b
}
fn digest(domain: &[u8], scope: &GraphScope, frames: &[&[u8]]) -> [u8; 32] {
    let mut hash = blake3::Hasher::new();
    hash.update(domain);
    hash.update(&context(scope));
    for frame in frames {
        hash.update(frame);
    }
    hash.update(&(frames.len() as u64).to_be_bytes());
    hash.update(&(frames.iter().map(|frame| frame.len() as u64).sum::<u64>()).to_be_bytes());
    *hash.finalize().as_bytes()
}
#[test]
fn independent_nine_budget_literals() {
    for &(name, size, valid, records, bytes, pages) in budget::BUDGET_CASES {
        let capacity = GraphCapacity::new(size);
        assert_eq!(capacity.is_ok(), valid, "{name}");
        if valid {
            let capacity = capacity.unwrap();
            assert_eq!(
                (
                    capacity.records(),
                    capacity.encoded_bytes(),
                    capacity.max_pages()
                ),
                (records, bytes, pages),
                "{name}"
            );
        }
    }
}
#[test]
fn independent_subject106_scope188_and_capacity_derivation() {
    let scopes = selected(false);
    let scope = scopes.graph();
    assert_eq!(scope.as_bytes(), context(scope));
    let subject = scope.subject();
    assert_eq!(subject.encode(), context(scope)[82..]);
    assert_eq!(
        GraphSubject::decode_for(subject.source_id(), &subject.encode()).unwrap(),
        *subject
    );
    let mut bad = subject.encode();
    bad[90..98].copy_from_slice(&65_537u64.to_be_bytes());
    assert!(GraphSubject::decode_for(subject.source_id(), &bad).is_err());
    bad = subject.encode();
    bad[40] = 1;
    assert!(GraphSubject::decode_for(subject.source_id(), &bad).is_err());
    assert_eq!(scopes.sites().state().phase(), 1);
    assert_eq!(scopes.roots().phase(), 3);
}
#[test]
fn independent_node60_edge43_and_mode_fences() {
    let scopes = selected(false);
    let scope = scopes.graph();
    let node = GraphNode::birth(scope, i64::MAX as u64, true).unwrap();
    assert_eq!(
        node.encode(),
        node_frame(
            scope,
            i64::MAX as u64,
            NodeFields {
                flags: 1,
                discovery: 0,
                lowlink: 0,
                parent: 0,
                after: 0,
                incoming: 0
            }
        )
    );
    let edge = GraphEdge::new(scope, 1, i64::MAX as u64, 3).unwrap();
    assert_eq!(edge.encode(), edge_frame(scope, 1, i64::MAX as u64, 3));
    assert_eq!(
        GraphNode::decode_value(scope, node.key(), &node.value()).unwrap(),
        node
    );
    assert!(GraphNodeKey::new(scope, 0).is_err());
    assert!(GraphEdgeKey::new(scope, 1, i64::MAX as u64 + 1).is_err());
    let fresh = selected(true);
    assert!(GraphNode::birth(fresh.graph(), 2, true).is_err());
    let fresh_node = GraphNode::birth(fresh.graph(), 1, false).unwrap();
    assert_eq!(
        fresh_node.encode(),
        node_frame(
            fresh.graph(),
            1,
            NodeFields {
                flags: 4,
                discovery: 0,
                lowlink: 0,
                parent: 0,
                after: 0,
                incoming: 0
            }
        )
    );
    assert!(fresh_node
        .finish_expansion(fresh.graph(), false)
        .unwrap()
        .enter(fresh.graph(), 1, 0)
        .is_err());
    let mut frame = node.encode();
    frame[18] = 5;
    assert!(GraphNode::decode(scope, &frame).is_err());
    frame = node.encode();
    frame[31] = 128;
    assert!(GraphNode::decode(scope, &frame).is_err());
    assert!(StateKey::directory_root(scope.nodes(), 1).is_err());
}
#[test]
fn independent_adjacency285_and_proof317_literal_hashes() {
    let scopes = selected(false);
    let scope = scopes.graph();
    let node = GraphNode::birth(scope, 2, true)
        .unwrap()
        .add_incoming(scope, 3)
        .unwrap()
        .finish_expansion(scope, true)
        .unwrap();
    let edge = GraphEdge::new(scope, 2, 2, 3).unwrap();
    let nf = node_frame(
        scope,
        2,
        NodeFields {
            flags: 35,
            discovery: 0,
            lowlink: 0,
            parent: 0,
            after: 0,
            incoming: 3,
        },
    );
    let ef = edge_frame(scope, 2, 2, 3);
    let nd = digest(
        b"layerfs/effective-graph/adjacency-nodes/v1\0",
        scope,
        &[&nf],
    );
    let ed = digest(
        b"layerfs/effective-graph/adjacency-edges/v1\0",
        scope,
        &[&ef],
    );
    let mut nl = GraphNodeLedger::adjacency(scope.clone());
    nl.acknowledge(&[node]).unwrap();
    let mut el = GraphEdgeLedger::new(scope.clone());
    el.acknowledge(&[edge]).unwrap();
    assert_eq!(nl.digest(), nd);
    assert_eq!(el.digest(), ed);
    let seal = GraphAdjacencySeal::new(scope.clone(), 1, 1, nd, ed).unwrap();
    let mut expected = [0; 285];
    expected[0] = 1;
    expected[1..189].copy_from_slice(&context(scope));
    expected[189..197].copy_from_slice(&1u64.to_be_bytes());
    expected[197..205].copy_from_slice(&1u64.to_be_bytes());
    expected[205..213].copy_from_slice(&60u64.to_be_bytes());
    expected[213..221].copy_from_slice(&43u64.to_be_bytes());
    expected[221..253].copy_from_slice(&nd);
    expected[253..285].copy_from_slice(&ed);
    assert_eq!(seal.encode(), expected);
    let completed = node
        .enter(scope, 1, 0)
        .unwrap()
        .advance(scope, 2, 1)
        .unwrap()
        .finish(scope)
        .unwrap()
        .complete(scope, 1)
        .unwrap();
    let full = node_frame(
        scope,
        2,
        NodeFields {
            flags: 115,
            discovery: 1,
            lowlink: 1,
            parent: 0,
            after: 2,
            incoming: 3,
        },
    );
    assert_eq!(completed.encode(), full);
    let pd = digest(b"layerfs/effective-graph/proof-nodes/v1\0", scope, &[&full]);
    let proof = GraphProofSeal::new(seal.clone(), pd);
    let mut expected_proof = [0; 317];
    expected_proof[..285].copy_from_slice(&expected);
    expected_proof[221..253].copy_from_slice(&pd);
    let mut ah = blake3::Hasher::new();
    ah.update(b"layerfs/effective-graph/adjacency-seal/v1\0");
    ah.update(&expected);
    expected_proof[285..].copy_from_slice(ah.finalize().as_bytes());
    assert_eq!(proof.encode(), expected_proof);
    assert_eq!(
        GraphProofSeal::decode(&seal, &expected_proof).unwrap(),
        proof
    );
}
#[test]
fn aggregate_full_records_refuse_at_exact_default_capacity() {
    let capacity = GraphCapacity::default();
    assert!(capacity.check_growth(65_536, 0, 0, 0).is_ok());
    assert!(capacity.check_growth(32_768, 32_768, 0, 0).is_ok());
    assert!(matches!(
        capacity.check_growth(65_536, 0, 0, 1),
        Err(ContentError::BoundedCapacityExceeded {
            what: "graph.records",
            ..
        })
    ));
    let larger = GraphCapacity::new(48 * 1024 * 1024).unwrap();
    assert!(larger.check_growth(65_536, 65_536, 0, 1).is_ok());
    assert_eq!(larger.records(), 196_608);
}
#[test]
fn cross_table_keys_and_issued_owner_are_not_interchangeable() {
    let first = selected(false);
    let second = selected(false);
    let node = GraphNode::birth(first.graph(), 2, false).unwrap();
    assert!(GraphNode::decode(second.graph(), &node.encode()).is_err());
    let edge = GraphEdge::new(first.graph(), 1, 2, 1).unwrap();
    assert!(GraphNodeKey::decode(first.graph(), edge.key().as_bytes()).is_err());
    assert!(GraphEdgeKey::decode(first.graph(), node.key().as_bytes()).is_err());
}
#[test]
fn header_only_empty_eof_and_sticky_forged_count_refusal() {
    let scopes = selected(false);
    let scope = scopes.graph();
    let nd = digest(b"layerfs/effective-graph/adjacency-nodes/v1\0", scope, &[]);
    let ed = digest(b"layerfs/effective-graph/adjacency-edges/v1\0", scope, &[]);
    let seal = GraphAdjacencySeal::new(scope.clone(), 0, 0, nd, ed).unwrap();
    let page = GraphNodePage::after(seal.clone(), None, Vec::new(), true).unwrap();
    page.check_limit(GraphPageLimit::new(128, 318).unwrap())
        .unwrap();
    let mut cursor = GraphNodeCursor::new(seal.clone());
    cursor.accept(&page, GraphPageLimit::default()).unwrap();
    assert!(cursor.finished());
    assert!(cursor.accept(&page, GraphPageLimit::default()).is_err());
    let populated = GraphAdjacencySeal::new(scope.clone(), 1, 0, nd, ed).unwrap();
    let false_eof = GraphNodePage::after(populated.clone(), None, Vec::new(), true).unwrap();
    let mut cursor = GraphNodeCursor::new(populated);
    let first = cursor
        .accept(&false_eof, GraphPageLimit::default())
        .unwrap_err();
    assert_eq!(
        cursor
            .accept(&false_eof, GraphPageLimit::default())
            .unwrap_err(),
        first
    );
    assert!(GraphNodePage::after(seal, None, Vec::new(), false).is_err());
}
#[test]
fn actual_vec_capacity_and_combined_build_window_are_independent() {
    let scopes = selected(false);
    let scope = scopes.graph();
    let node = GraphNode::birth(scope, 1, true)
        .unwrap()
        .finish_expansion(scope, false)
        .unwrap();
    let seal = GraphAdjacencySeal::new(scope.clone(), 1, 0, [7; 32], [8; 32]).unwrap();
    let mut over = Vec::with_capacity(129);
    over.push(node);
    assert!(GraphNodePage::after(seal, None, over, true).is_err());
    let n = GraphNodeChange::new(scope, None, node).unwrap();
    let nodes = vec![n; 65];
    let edge = GraphEdgeChange::new(scope, None, GraphEdge::new(scope, 1, 2, 1).unwrap()).unwrap();
    let edges = vec![edge; 64];
    assert!(GraphBuildAck::new(
        scope.clone(),
        GraphTotals::default(),
        GraphTotals::new(65, 64, 64).unwrap(),
        nodes,
        edges,
        None
    )
    .is_err());
}
#[test]
fn descend_equal_targets_refuses_before_provider_callback() {
    let scopes = selected(false);
    let scope = scopes.graph();
    let white = GraphNode::birth(scope, 1, true)
        .unwrap()
        .finish_expansion(scope, false)
        .unwrap();
    let parent = white.enter(scope, 1, 0).unwrap();
    let edge = GraphEdge::new(scope, 1, 1, 1).unwrap();
    let mutation = GraphMutation::Descend {
        parent_before: parent,
        parent_after: parent.advance(scope, 1, 1).unwrap(),
        child_before: white,
        child_after: white.enter(scope, 2, 1).unwrap(),
        edge,
    };
    let calls = std::cell::Cell::new(0u64);
    let outcome = GraphMutationLimit::default()
        .check(scope, &[mutation])
        .map(|()| calls.set(calls.get() + 1));
    assert!(outcome.is_err());
    assert_eq!(calls.get(), 0);
}
#[test]
fn atomic_descend_counts_two_targets_and_duplicate_batches_refuse() {
    let scopes = selected(false);
    let scope = scopes.graph();
    let parent = GraphNode::birth(scope, 1, false)
        .unwrap()
        .finish_expansion(scope, false)
        .unwrap()
        .enter(scope, 1, 0)
        .unwrap();
    let child = GraphNode::birth(scope, 2, true)
        .unwrap()
        .finish_expansion(scope, false)
        .unwrap();
    let mutation = GraphMutation::Descend {
        parent_before: parent,
        parent_after: parent.advance(scope, 2, 1).unwrap(),
        child_before: child,
        child_after: child.enter(scope, 2, 1).unwrap(),
        edge: GraphEdge::new(scope, 1, 2, 1).unwrap(),
    };
    assert!(GraphMutationLimit::new(1, 65536)
        .unwrap()
        .check(scope, &[mutation])
        .is_err());
    assert!(GraphMutationLimit::new(2, 65536)
        .unwrap()
        .check(scope, &[mutation])
        .is_ok());
    assert!(GraphMutationLimit::default()
        .check(scope, &[mutation, mutation])
        .is_err());
}
#[test]
fn parent_edge_max_and_positive_after_are_checked_without_rank() {
    let scopes = selected(false);
    let scope = scopes.graph();
    let seal = GraphAdjacencySeal::new(scope.clone(), 2, 1, [4; 32], [5; 32]).unwrap();
    let edge = GraphEdge::new(scope, 1, 2, 1).unwrap();
    let page = GraphEdgePage::after(seal.clone(), 1, None, Some(2), vec![edge], true).unwrap();
    assert_eq!(page.encode_header().len(), 318);
    let ended = GraphEdgePage::after(seal.clone(), 1, Some(2), Some(2), Vec::new(), true).unwrap();
    ended
        .check_limit(GraphPageLimit::new(128, 318).unwrap())
        .unwrap();
    assert!(GraphEdgePage::after(seal.clone(), 1, Some(0), Some(2), Vec::new(), true).is_err());
    assert!(GraphEdgePage::after(seal, 1, None, Some(2), vec![edge], false).is_err());
}

#[test]
fn leave_root_full_record_bytes_refuse_before_provider_at377_boundary() {
    let scopes = selected(false);
    let scope = scopes.graph();
    let finished = GraphNode::birth(scope, 1, false)
        .unwrap()
        .finish_expansion(scope, false)
        .unwrap()
        .enter(scope, 1, 0)
        .unwrap()
        .finish(scope)
        .unwrap()
        .complete(scope, 1)
        .unwrap();
    let mutation = GraphMutation::LeaveRoot {
        finished_root: finished,
    };
    assert_eq!(mutation.encoded_record_bytes(), 60);
    let calls = std::cell::Cell::new(0u64);
    let result = GraphMutationLimit::new(1, 376)
        .unwrap()
        .check(scope, &[mutation])
        .map(|()| calls.set(calls.get() + 1));
    assert!(result.is_err());
    assert_eq!(calls.get(), 0);
    GraphMutationLimit::new(1, 377)
        .unwrap()
        .check(scope, &[mutation])
        .unwrap();
}

#[test]
fn native_type_ownership_counts_are_separate_from_encoded_windows() {
    use std::mem::size_of;
    assert_eq!(size_of::<GraphNodeKey>(), 25);
    assert_eq!(size_of::<GraphEdgeKey>(), 33);
    let maximum_ack_records = 128 * size_of::<GraphNodeChange>().max(size_of::<GraphEdgeChange>());
    let child_window = 63 * size_of::<GraphNodeKey>();
    let solver_pages = 256 * size_of::<GraphNode>() + size_of::<GraphEdge>();
    let proof_hashers = 2 * size_of::<blake3::Hasher>();
    eprintln!("C1 native ownership count diagnostic:Node={} Edge={} NodeChange={} EdgeChange={} Mutation={} Hasher={}; combined128-target ack max{} +63child keys{}; simultaneous chooser128+pop128+edge1 record bytes{}; proof hashers{}. Encoded widths60/43 and128 records are separate; source/peeks/allocator metadata/memo/provider/cache/Unknown/native/OS excluded.",size_of::<GraphNode>(),size_of::<GraphEdge>(),size_of::<GraphNodeChange>(),size_of::<GraphEdgeChange>(),size_of::<GraphMutation>(),size_of::<blake3::Hasher>(),maximum_ack_records,child_window,solver_pages,proof_hashers);
}
