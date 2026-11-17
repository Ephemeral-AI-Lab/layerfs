//! PostgreSQL runtime evidence for the GC additions that shipped without it:
//! T1.3b's additive GC tables, T7.1's `kept_node` table and history closure,
//! and T7.2's object closure and live-heads digest.
//!
//! Each test creates one disposable schema and drives it through
//! `PostgresHandles`; a test that simulates an older Store rewrites the live
//! schema, its recorded source and its recorded definition exactly as the
//! release before the feature wrote them, then opens it writable.
//!
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
mod support;

use layerfs_content::{construct_bytes, read_range, ObjectId};
use layerfs_history::HistoryCatalog;
use layerfs_persistence::{KeptNodeId, PostgresHandles};
use layerfs_storage::{port::PackPersistence, Storage};
use layerfs_telemetry::timer::Timing;
use std::sync::Arc;
use support::pg::{
    commit, commit_with, counts, project, project_with_genesis, publish, url, Schema, DEFINITION,
};

const BINDING: &[u8] = b"postgresql-gc-mark";
const CURSOR: [u8; 32] = [76; 32];

/// The eight tables T1.3b appended; the pre-GC Store has none of them.
const GC_TABLES: [&str; 8] = [
    "branch_retired",
    "gc_dead",
    "gc_intent",
    "gc_op",
    "kept_node",
    "layer_stack_retired",
    "path_removed",
    "project_epoch",
];

/// Rows of `kept_node`, in the statement's order.
fn kept_rows(admin: &mut postgres::Client) -> Vec<(Vec<u8>, String, Vec<u8>, i64)> {
    admin
        .query(
            "SELECT layer_stack_id,kind,node_id,serial FROM kept_node \
             ORDER BY layer_stack_id,kind,node_id",
            &[],
        )
        .unwrap()
        .into_iter()
        .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3)))
        .collect()
}

/// Rewrites a fresh Store into the format a release before T1.3b wrote: no GC
/// tables and no position columns/state. Returns the source it replaced.
fn make_pre_gc(schema: &Schema) -> String {
    let fresh: String = schema
        .admin()
        .query_one("SELECT schema_source FROM history_meta", &[])
        .unwrap()
        .get(0);
    let start = fresh
        .find("\nCREATE TABLE branch_retired (")
        .expect("the GC schema starts with branch_retired");
    let acquisition = fresh
        .find("CREATE TABLE init_operation (")
        .expect("the acquisition schema follows the GC tables");
    assert!(start < acquisition);
    let old = format!("{}{}", &fresh[..start], &fresh[acquisition..]);
    assert_ne!(old, fresh);
    schema
        .admin()
        .batch_execute(
            "DROP TABLE gc_dead; DROP TABLE gc_intent; DROP TABLE gc_op; \
             DROP TABLE project_epoch; DROP TABLE path_removed; DROP TABLE kept_node; \
             DROP TABLE branch_retired; DROP TABLE layer_stack_retired; \
             ALTER TABLE history_meta DROP COLUMN next_history_serial; \
             ALTER TABLE \"commit\" DROP COLUMN history_serial; \
             ALTER TABLE layer DROP COLUMN history_serial; \
             ALTER TABLE store_policy DROP COLUMN trace_salt;",
        )
        .unwrap();
    let definition: String = schema.admin().query_one(DEFINITION, &[]).unwrap().get(0);
    schema
        .admin()
        .execute(
            "UPDATE history_meta SET schema_source=$1,schema_definition=$2",
            &[&old, &definition],
        )
        .unwrap();
    fresh
}

/// Rewrites a fresh Store into the format a release before T7.1 wrote: the
/// `kept_node` table is gone from the schema and from the recorded source.
fn make_pre_kept_node(schema: &Schema) -> String {
    let fresh: String = schema
        .admin()
        .query_one("SELECT schema_source FROM history_meta", &[])
        .unwrap()
        .get(0);
    let start = fresh
        .find("CREATE TABLE kept_node (")
        .expect("a fresh Store holds kept_node");
    let end = start + fresh[start..].find(";\n").expect("the table ends") + 2;
    let old = format!("{}{}", &fresh[..start], &fresh[end..]);
    assert_ne!(old, fresh);
    schema
        .admin()
        .batch_execute("DROP TABLE kept_node;")
        .unwrap();
    let definition: String = schema.admin().query_one(DEFINITION, &[]).unwrap().get(0);
    schema
        .admin()
        .execute(
            "UPDATE history_meta SET schema_source=$1,schema_definition=$2",
            &[&old, &definition],
        )
        .unwrap();
    fresh
}

/// One Save of `bytes`; returns its root.
fn save(storage: &Storage, bytes: &[u8]) -> ObjectId {
    let save = storage.begin_save().unwrap();
    let root = Timing::disabled("construct", |scope| {
        construct_bytes(
            storage.policy().construction(),
            &storage.policy().construction().capacities(),
            bytes,
            &mut save.sink(),
            scope.child("content"),
        )
    })
    .0
    .unwrap()
    .root;
    save.finish().unwrap();
    root
}

fn bytes(seed: u32, len: usize) -> Vec<u8> {
    let mut state = seed.wrapping_mul(2654435761) | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state >> 24) as u8
        })
        .collect()
}

#[test]
fn a_pre_gc_store_gains_the_gc_tables_and_its_project_epoch_rows() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "pre_gc");
    let handles = schema.create(BINDING, CURSOR);
    let (shop, work, _genesis) = project(&handles.history, 1);
    drop(handles);
    let fresh = make_pre_gc(&schema);

    let mut before = schema.admin();
    assert_eq!(
        before
            .query_one("SELECT count(*) FROM layer_stack", &[])
            .unwrap()
            .get::<_, i64>(0),
        1
    );
    assert!(before
        .query_one(
            "SELECT count(*) FROM information_schema.tables WHERE table_schema=current_schema() AND table_name='gc_intent'",
            &[],
        )
        .unwrap()
        .get::<_, i64>(0)
        == 0);
    drop(before);

    let _upgraded = schema
        .open(BINDING, CURSOR)
        .expect("a pre-T1.3b Store opens writable");
    let mut admin = schema.admin();
    for table in GC_TABLES {
        assert_eq!(
            admin
                .query_one(
                    "SELECT count(*) FROM information_schema.tables \
                     WHERE table_schema=current_schema() AND table_name=$1 AND table_type='BASE TABLE'",
                    &[&table],
                )
                .unwrap()
                .get::<_, i64>(0),
            1,
            "{table} exists after the additive migration"
        );
    }
    assert_eq!(
        admin
            .query_one("SELECT count(*) FROM project_epoch", &[])
            .unwrap()
            .get::<_, i64>(0),
        1,
        "the existing project gains one epoch row"
    );
    assert_eq!(
        admin
            .query_one(
                "SELECT epoch FROM project_epoch WHERE layer_stack_id=$1",
                &[&shop.as_slice()],
            )
            .unwrap()
            .get::<_, i64>(0),
        0
    );
    let (source, recorded_definition): (String, String) = {
        let row = admin
            .query_one(
                "SELECT schema_source,schema_definition FROM history_meta",
                &[],
            )
            .unwrap();
        (row.get(0), row.get(1))
    };
    assert_eq!(source, fresh, "the migration records the current source");
    assert_eq!(recorded_definition, schema.definition());
    drop(admin);

    // A second writable open changes nothing.
    let again = schema
        .open(BINDING, CURSOR)
        .expect("the migrated Store reopens");
    assert_eq!(
        again
            .history
            .branch(work)
            .unwrap()
            .expect("the Branch survived")
            .head_commit,
        None
    );
}

#[test]
fn a_pre_kept_node_store_gains_the_table_and_a_mark_fills_it() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "pre_kept");
    let handles = schema.create(BINDING, CURSOR);
    let (shop, work, genesis) = project(&handles.history, 1);
    let first = commit(&handles.history, work, 5);
    let layer = publish(&handles.history, shop, work);
    drop(handles);
    let fresh = make_pre_kept_node(&schema);

    let handles = schema
        .open(BINDING, CURSOR)
        .expect("a pre-T7.1 Store opens writable");
    let mut admin = schema.admin();
    assert_eq!(
        admin
            .query_one("SELECT count(*) FROM kept_node", &[])
            .unwrap()
            .get::<_, i64>(0),
        0,
        "the table is empty until a mark writes it"
    );
    let source: String = admin
        .query_one("SELECT schema_source FROM history_meta", &[])
        .unwrap()
        .get(0);
    assert_eq!(source, fresh);
    drop(admin);

    let kept = handles
        .history
        .mark_history_closure()
        .expect("a mark writes the history closure");
    assert!(kept.contains(KeptNodeId::Stack(shop)));
    assert!(kept.contains(KeptNodeId::Branch(work)));
    assert!(kept.contains(KeptNodeId::Commit(first.id)));
    assert!(kept.contains(KeptNodeId::Layer(layer.id)));
    assert!(
        kept.contains(KeptNodeId::Layer(genesis)),
        "the genesis Layer is reached"
    );
    assert_eq!(kept_rows(&mut schema.admin()).len(), kept.len());
}

#[test]
fn a_mark_records_the_history_closure_on_postgresql() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "mark_history");
    let handles = schema.create(BINDING, CURSOR);
    let (shop, work, genesis) = project(&handles.history, 1);
    let first = commit(&handles.history, work, 5);
    let layer = publish(&handles.history, shop, work);

    let kept = handles
        .history
        .mark_history_closure()
        .expect("a mark writes kept_node");
    for id in [
        KeptNodeId::Stack(shop),
        KeptNodeId::Branch(work),
        KeptNodeId::Commit(first.id),
        KeptNodeId::Layer(genesis),
        KeptNodeId::Layer(layer.id),
    ] {
        assert!(kept.contains(id), "{id:?} is retained");
    }
    let rows = kept_rows(&mut schema.admin());
    assert_eq!(rows.len(), kept.len());
    let mut expected: Vec<(Vec<u8>, String, Vec<u8>)> = kept
        .nodes()
        .iter()
        .map(|node| {
            (
                node.stack().as_slice().to_vec(),
                node.id().kind().to_owned(),
                node.id().as_slice().to_vec(),
            )
        })
        .collect();
    expected.sort();
    let mut actual: Vec<(Vec<u8>, String, Vec<u8>)> = rows
        .iter()
        .map(|(stack, kind, id, _)| (stack.clone(), kind.clone(), id.clone()))
        .collect();
    actual.sort();
    assert_eq!(actual, expected);

    // A second mark over unchanged history writes the same rows.
    let again = handles.history.mark_history_closure().unwrap();
    assert_eq!(again, kept);
    assert_eq!(kept_rows(&mut schema.admin()), rows);
}

#[test]
fn a_mark_keeps_the_saved_objects_and_reports_the_live_heads_digest() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "mark_objects");
    let handles: PostgresHandles = schema.create(BINDING, CURSOR);
    let storage = Storage::new(handles.storage.clone() as Arc<dyn PackPersistence>).unwrap();
    let genesis_root = save(&storage, b"genesis tree");
    let (shop, work, genesis) = project_with_genesis(&handles.history, 1, genesis_root);
    let root_id = save(&storage, &bytes(5, 5000));
    let record = commit_with(&handles.history, work, root_id);
    let layer = publish(&handles.history, shop, work);

    let mark = handles.history.mark().expect("a mark writes both closures");
    assert!(
        mark.objects().contains(root_id),
        "the committed root's object is retained: {:?}",
        mark.objects().objects()
    );
    assert!(!mark.objects().packs().is_empty(), "its pack is retained");
    assert!(mark.history().contains(KeptNodeId::Commit(record.id)));
    assert!(mark.history().contains(KeptNodeId::Layer(layer.id)));
    assert!(mark.history().contains(KeptNodeId::Layer(genesis)));
    assert_eq!(kept_rows(&mut schema.admin()).len(), mark.history().len());

    let digest = mark.live_heads_digest();
    assert_ne!(
        digest, [0; 32],
        "the live-heads digest is not a placeholder"
    );
    let again = handles.history.mark().expect("a second mark");
    assert_eq!(again.objects(), mark.objects());
    assert_eq!(again.live_heads_digest(), digest);

    // Nothing was compacted: the object and its pack are still readable.
    let mut bytes_out = Vec::new();
    Timing::disabled("read", |scope| {
        read_range(
            &storage.reader().unwrap(),
            root_id,
            0..5000,
            &mut bytes_out,
            scope.child("read"),
        )
    })
    .0
    .unwrap();
    assert_eq!(bytes_out, bytes(5, 5000));
    let (commits, _layers) = counts(&mut schema.admin());
    assert_eq!(commits, 1);
}
