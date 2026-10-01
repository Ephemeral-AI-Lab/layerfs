//! Certificate fixtures use actual C1 codecs and real SQLite, not product hooks.
use layerfs_content::{
    file::encode_whole_file_payload,
    file::mapping::{self, ChildDescriptor, ExtentNode, ExtentSlice, FileState},
    *,
};
use phase6_live_probe::file_facts;
use rusqlite::Connection;
fn db() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(file_facts::SCHEMA).unwrap();
    db
}
fn store(db: &Connection, role: ObjectRole, canonical: &[u8]) -> ObjectId {
    let id = ObjectId::for_bytes(canonical);
    file_facts::record(db, id, role, canonical).unwrap();
    id
}
fn root(db: &Connection, node: ObjectId, bytes: u64, extents: u64, level: u8) -> ObjectId {
    let state = FileState {
        logical_len: bytes,
        extent_count: extents,
        tree_level: level,
        profile_id: mapping::profile_id(),
        mapping_root: node,
    };
    store(
        db,
        ObjectRole::FileState,
        &mapping::encode_file_state(state).unwrap(),
    )
}
fn leaf(db: &Connection, chunk: ObjectId, len: u32, count: usize) -> ObjectId {
    let node = ExtentNode::Leaf {
        subtree_logical_bytes: u64::from(len) * count as u64,
        extents: vec![ExtentSlice::new(chunk, 0, len).unwrap(); count],
    };
    store(
        db,
        ObjectRole::ExtentLeaf,
        &mapping::encode_node(&node, true).unwrap(),
    )
}
#[test]
fn deferred_child_certification_and_immutable_reuse() {
    let db = db();
    let canonical = mapping::encode_chunk_object(b"literal").unwrap();
    let chunk = ObjectId::for_bytes(&canonical);
    let node = leaf(&db, chunk, 7, 1);
    let file = root(&db, node, 7, 1, 0);
    assert!(
        file_facts::certify_file(&db, file).is_err(),
        "missing payload cannot certify a graph"
    );
    store(&db, ObjectRole::Chunk, &canonical);
    let w = file_facts::certify_file(&db, file).unwrap();
    assert_eq!(w.newly_certified, 3);
    assert_eq!(w.edges, 2);
    let w = file_facts::certify_file(&db, file).unwrap();
    assert_eq!((w.visited, w.reused, w.edges), (1, 1, 0));
    file_facts::record(&db, chunk, ObjectRole::Chunk, &canonical).unwrap();
    assert_eq!(file_facts::certify_file(&db, file).unwrap().reused, 1);
}
#[test]
fn rejects_authenticated_but_short_slice_and_wrong_role_or_eof() {
    let db = db();
    let chunk = store(
        &db,
        ObjectRole::Chunk,
        &mapping::encode_chunk_object(b"x").unwrap(),
    );
    let node = leaf(&db, chunk, 2, 1);
    let file = root(&db, node, 2, 1, 0);
    assert!(file_facts::certify_file(&db, file)
        .unwrap_err()
        .contains("role/range/summary/partition"));
    assert!(file_facts::certify_file(&db, chunk)
        .unwrap_err()
        .contains("root role"));
    let canonical = encode_whole_file_payload(b"literal").unwrap();
    assert!(file_facts::record(
        &db,
        ObjectId::for_bytes(&canonical),
        ObjectRole::FileState,
        &canonical
    )
    .is_err());
    let mut trailing = canonical;
    trailing.push(0);
    assert!(file_facts::record(
        &db,
        ObjectId::for_bytes(&trailing),
        ObjectRole::WholeFile,
        &trailing
    )
    .is_err());
}
#[test]
fn rejects_forged_child_summary_and_short_nonroot() {
    let db = db();
    let chunk = store(
        &db,
        ObjectRole::Chunk,
        &mapping::encode_chunk_object(b"Q").unwrap(),
    );
    let child = leaf(&db, chunk, 1, 64);
    let branch = ExtentNode::Branch {
        level: 1,
        subtree_logical_bytes: 128,
        subtree_extent_count: 128,
        children: vec![
            ChildDescriptor {
                cumulative_logical_end: 65,
                cumulative_extent_end: 64,
                child_object_id: child,
            },
            ChildDescriptor {
                cumulative_logical_end: 128,
                cumulative_extent_end: 128,
                child_object_id: child,
            },
        ],
    };
    let bad = store(
        &db,
        ObjectRole::ExtentBranch,
        &mapping::encode_node(&branch, true).unwrap(),
    );
    assert!(file_facts::certify_file(&db, root(&db, bad, 128, 128, 1)).is_err());
    let short = leaf(&db, chunk, 1, 1);
    let branch = ExtentNode::Branch {
        level: 1,
        subtree_logical_bytes: 2,
        subtree_extent_count: 2,
        children: vec![
            ChildDescriptor {
                cumulative_logical_end: 1,
                cumulative_extent_end: 1,
                child_object_id: short,
            },
            ChildDescriptor {
                cumulative_logical_end: 2,
                cumulative_extent_end: 2,
                child_object_id: short,
            },
        ],
    };
    let bad = store(
        &db,
        ObjectRole::ExtentBranch,
        &mapping::encode_node(&branch, true).unwrap(),
    );
    assert!(
        file_facts::certify_file(&db, root(&db, bad, 2, 2, 1)).is_err(),
        "root-valid short leaves are not valid nonroot children"
    );
}
#[test]
fn changed_graph_reuses_certified_payloads_and_validates_exact_state() {
    let db = db();
    let chunk = store(
        &db,
        ObjectRole::Chunk,
        &mapping::encode_chunk_object(b"xy").unwrap(),
    );
    let node = leaf(&db, chunk, 1, 64);
    let first = root(&db, node, 64, 64, 0);
    file_facts::certify_file(&db, first).unwrap();
    let node = leaf(&db, chunk, 2, 64);
    let second = root(&db, node, 128, 64, 0);
    let w = file_facts::certify_file(&db, second).unwrap();
    assert_eq!(w.newly_certified, 2);
    assert_eq!(w.reused, 64);
    assert!(file_facts::certify_file(&db, root(&db, node, 129, 64, 0)).is_err());
    let whole = store(
        &db,
        ObjectRole::WholeFile,
        &encode_whole_file_payload(b"literal").unwrap(),
    );
    assert_eq!(
        file_facts::certify_file(&db, whole)
            .unwrap()
            .newly_certified,
        1
    );
}
