//! Genuine issued zero source/phase authority; no native or physical proof implied.
use layerfs_content::{
    filesystem::{
        inode::read::InodeTable,
        root::FilesystemRootId,
        rows::{BindingRows, RowSource, SpoolDeclaration, SpoolPreparation},
        state::*,
        InodeScope,
    },
    ContentError, ObjectId,
};
use std::io::{self, Cursor};
fn id(value: u8) -> ObjectId {
    ObjectId::from_bytes(&[value; 32]).unwrap()
}
fn prepare() -> SpoolPreparation {
    SpoolPreparation::new(
        SpoolDeclaration {
            directories: 0,
            inodes: 0,
            fresh: 0,
            bindings: 0,
            wire_name_bytes: 0,
        },
        48,
    )
    .unwrap()
}
fn hash(domain: &[u8], scope: &[u8]) -> [u8; 32] {
    let mut hash = blake3::Hasher::new();
    hash.update(domain);
    hash.update(scope);
    hash.update(&0u64.to_be_bytes());
    hash.update(&0u64.to_be_bytes());
    *hash.finalize().as_bytes()
}
#[test]
fn source_requires_declared_zero_and_actual_eof_without_issuing_another_id() {
    let preparation = prepare();
    let original = preparation.source_id();
    let rows = preparation.verify_empty_eof(&mut io::empty()).unwrap();
    assert_eq!(rows.source_id(), original);
    assert_eq!(
        (rows.directory_rows(), rows.inode_rows(), rows.new_rows()),
        (0, 0, 0)
    );
    for _ in 0..2 {
        assert!(rows
            .directory_headers()
            .unwrap()
            .next_header()
            .unwrap()
            .is_none());
        assert!(rows.inodes().unwrap().next_row().unwrap().is_none());
    }
    assert!(matches!(
        prepare().verify_empty_eof(&mut Cursor::new([1u8])),
        Err(ContentError::TrailingBytes)
    ));
    let nonempty = SpoolPreparation::new(
        SpoolDeclaration {
            directories: 0,
            inodes: 1,
            fresh: 0,
            bindings: 0,
            wire_name_bytes: 0,
        },
        1024,
    )
    .unwrap();
    assert!(matches!(
        nonempty.verify_empty_eof(&mut io::empty()),
        Err(ContentError::InvalidRecord("empty source declaration"))
    ));
}
#[test]
fn checked_empty_selective_scc_and_roots_finish_exact_transcripts() {
    let preparation = prepare();
    let subject = GraphSubject::new(
        preparation.source_id(),
        InodeScope::from_object(id(10)),
        Some(FilesystemRootId(id(11))),
        1,
        GraphCapacity::default(),
    )
    .unwrap();
    let mut state = VerifiedEmptyState::new([13; 32], subject.clone()).unwrap();
    let scopes = state.scopes().clone();
    assert!(state.graph_capacity(scopes.graph()).is_err());
    let rows = preparation.verify_empty_eof(&mut io::empty()).unwrap();
    state.confirm(&rows).unwrap();
    assert!(state.confirm(&rows).is_err());
    let selected = FactSubject::new(
        subject,
        Some(InodeTable {
            root: id(12),
            root_serial: 1,
        }),
    )
    .unwrap();
    let facts = FactScope::new(state.selection().clone(), selected.clone()).unwrap();
    let parents = FactScope::parents(state.selection().clone(), selected).unwrap();
    assert_eq!(state.fact_capacity(&facts).unwrap().facts(), 0);
    state.fact_bind(&facts).unwrap();
    state.parent_bind(&parents).unwrap();
    state.parent_close_declarations(&parents).unwrap();
    let parents = state.parent_seal(&parents).unwrap();
    let births = SiteBirthLedger::new(scopes.sites().clone()).unwrap().seal();
    let members = state.site_close_membership(&births).unwrap();
    assert_eq!(state.alias_capacity(&members).unwrap().records(), 0);
    state.alias_begin(&members, None).unwrap();
    let aliases = state.alias_finish(&members).unwrap();
    state.alias_retire(&aliases).unwrap();
    let sites = state.site_final_seal(&members).unwrap();
    let page = state
        .site_sealed_page(&sites, None, SitePageLimit::default())
        .unwrap();
    assert!(page.records().is_empty() && page.eof());
    state.site_retire(&sites).unwrap();
    assert!(state.graph_unexpanded(scopes.graph()).unwrap().is_none());
    let expected_node = hash(
        b"layerfs/effective-graph/adjacency-nodes/v1\0",
        &scopes.graph().as_bytes(),
    );
    let expected_edge = hash(
        b"layerfs/effective-graph/adjacency-edges/v1\0",
        &scopes.graph().as_bytes(),
    );
    let adjacency = state.graph_seal(scopes.graph()).unwrap();
    assert_eq!(adjacency.node_digest(), &expected_node);
    assert_eq!(adjacency.edge_digest(), &expected_edge);
    let page = state
        .graph_node_page(&adjacency, None, GraphPageLimit::default())
        .unwrap();
    assert!(page.records().is_empty() && page.eof());
    state.graph_begin_scc(&adjacency).unwrap();
    state
        .graph_node_page(&adjacency, None, GraphPageLimit::default())
        .unwrap();
    let proof = state.graph_finish(&adjacency).unwrap();
    assert_eq!(
        proof.node_digest(),
        &hash(
            b"layerfs/effective-graph/proof-nodes/v1\0",
            &scopes.graph().as_bytes()
        )
    );
    state
        .graph_proof_page(&proof, None, GraphPageLimit::default())
        .unwrap();
    state.graph_retire(&proof).unwrap();
    let facts = state.fact_seal(&facts).unwrap();
    let held = state.fact_page(&facts, None, 128, 65536).unwrap();
    assert!(held.records().is_empty() && held.eof);
    let before_drop = state.working_bytes();
    state.fact_retire(&facts).unwrap();
    drop(held);
    assert!(state.working_bytes() < before_drop);
    assert_eq!(state.capacity(scopes.roots()).unwrap().records(), 0);
    let roots = state.seal(scopes.roots()).unwrap();
    state.page(&roots, None, PageLimit::default()).unwrap();
    state.parent_page(&parents, None, 128, 65536).unwrap();
    state.parent_retire(&parents).unwrap();
    state.release(scopes.roots()).unwrap();
    assert!(state.completed());
    let physical = state.physical();
    assert_eq!(physical.native_binding, None);
    assert_eq!(
        (
            physical.native_files,
            physical.reserved_bytes,
            physical.allocated_bytes,
            physical.cleanup_events
        ),
        (0, 0, 0, 0)
    );
    assert!(state.graph_retire(&proof).is_err());
    assert!(state.release(scopes.roots()).is_err());
}
#[test]
fn another_issued_source_and_any_real_growth_refuse_without_native_effects() {
    let preparation = prepare();
    let subject = GraphSubject::new(
        preparation.source_id(),
        InodeScope::from_object(id(10)),
        Some(FilesystemRootId(id(11))),
        1,
        GraphCapacity::default(),
    )
    .unwrap();
    let mut state = VerifiedEmptyState::new([14; 32], subject.clone()).unwrap();
    let foreign = prepare().verify_empty_eof(&mut io::empty()).unwrap();
    assert!(state.confirm(&foreign).is_err());
    let rows = preparation.verify_empty_eof(&mut io::empty()).unwrap();
    state.confirm(&rows).unwrap();
    let scopes = state.scopes().clone();
    let facts = FactScope::new(
        state.selection().clone(),
        FactSubject::new(
            subject,
            Some(InodeTable {
                root: id(12),
                root_serial: 1,
            }),
        )
        .unwrap(),
    )
    .unwrap();
    state.fact_bind(&facts).unwrap();
    assert!(state
        .fact_insert(
            &facts,
            &[BaseFact {
                serial: 1,
                value: None
            }]
        )
        .is_err());
    let births = SiteBirthLedger::new(scopes.sites().clone()).unwrap().seal();
    let members = state.site_close_membership(&births).unwrap();
    assert!(state.alias_begin(&members, Some(1)).is_err());
    let key = GraphNodeKey::new(scopes.graph(), 1).unwrap();
    assert!(state.graph_seed_batch(scopes.graph(), &[key]).is_err());
    assert_eq!(state.physical().native_files, 0);
}
