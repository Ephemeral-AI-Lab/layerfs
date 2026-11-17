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
    commit, commit_with, counts, merge_request, project, project_with_genesis, publish, url,
    Schema, DEFINITION,
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

/// The closure statement both engines shared. PostgreSQL now runs its own
/// text, which follows each node's row by primary key instead of joining each
/// round to every history edge.
const SHARED_MARK: &str = include_str!("../sql/sqlite/queries/history/kept_node_mark.sql");

#[test]
fn the_postgresql_closure_keeps_the_nodes_the_shared_statement_keeps() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "mark_variant");
    let handles = schema.create(BINDING, CURSOR);
    // A live project with every edge kind this fixture can make: a published
    // Commit, a fork, a merge whose second parent is a Layer, and a chain of
    // unpublished Commits that only the Branch head reaches.
    let (shop, work, genesis) = project(&handles.history, 1);
    let first = commit(&handles.history, work, 5);
    publish(&handles.history, shop, work);
    handles
        .history
        .merge_branch(&merge_request(
            &handles.history,
            shop,
            work,
            first.id,
            genesis,
            60,
        ))
        .unwrap();
    let chain: Vec<_> = (10..40)
        .map(|tag| commit(&handles.history, work, tag).id)
        .collect();
    // A deleted project: neither text may keep one of its rows.
    let (gone, gone_work, _) = project(&handles.history, 3);
    commit(&handles.history, gone_work, 7);
    publish(&handles.history, gone, gone_work);
    handles.history.mark_stack_deleted(gone, 2_000).unwrap();

    let kept = handles.history.mark_history_closure().unwrap();
    for id in &chain {
        assert!(kept.contains(KeptNodeId::Commit(*id)), "{id:?} is retained");
    }
    let variant = kept_rows(&mut schema.admin());
    assert_eq!(variant.len(), kept.len());
    assert!(variant
        .iter()
        .all(|(stack, ..)| stack.as_slice() != gone.as_slice()));

    let mut admin = schema.admin();
    let mut tx = admin.transaction().unwrap();
    tx.execute("DELETE FROM kept_node", &[]).unwrap();
    tx.execute(SHARED_MARK, &[]).unwrap();
    let shared: Vec<(Vec<u8>, String, Vec<u8>, i64)> = tx
        .query(
            "SELECT layer_stack_id,kind,node_id,serial FROM kept_node \
             ORDER BY layer_stack_id,kind,node_id",
            &[],
        )
        .unwrap()
        .into_iter()
        .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3)))
        .collect();
    tx.rollback().unwrap();
    assert_eq!(variant, shared);
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

/// One Layer's tree in one Save: `files` whole files of 1 KiB under one inode
/// leaf, its directory and its filesystem root. Returns the root and every
/// object id of the Save.
fn save_tree(handles: &PostgresHandles, seed: u32, files: u32) -> (ObjectId, Vec<ObjectId>) {
    use layerfs_content::filesystem::attributes::PortableMetadata;
    use layerfs_content::filesystem::directory::{encode_directory_page, DirectoryPage};
    use layerfs_content::filesystem::inode::{encode_inode_page, InodePage};
    use layerfs_content::filesystem::root::profile_id;
    use layerfs_content::filesystem::{scope_for_seed, FilesystemRoot};
    use layerfs_content::inode_leaf::{InodeKind, InodeMetadata, InodeValue};
    use layerfs_content::{encode_whole_file_payload, FinalizedObject, ObjectRole};

    let metadata = |mode| {
        InodeMetadata::Inline(PortableMetadata {
            mode,
            mtime_seconds: i64::from(seed),
            mtime_nanoseconds: 0,
        })
    };
    let mut objects: Vec<FinalizedObject> = (0..files)
        .map(|index| {
            FinalizedObject::new(
                ObjectRole::WholeFile,
                encode_whole_file_payload(&bytes(seed.wrapping_mul(1000) + index, 1024)).unwrap(),
            )
            .unwrap()
        })
        .collect();
    let directory = FinalizedObject::new(
        ObjectRole::DirectoryLeaf,
        encode_directory_page(&DirectoryPage::Leaf {
            entries: Vec::new(),
        })
        .unwrap(),
    )
    .unwrap();
    let mut entries = vec![(
        1,
        InodeValue {
            kind: InodeKind::Directory,
            namespace_ref_count: 0,
            content_root: directory.id(),
            metadata_root: metadata(0o755),
        },
    )];
    let mut references = vec![directory.id()];
    for (offset, file) in objects.iter().enumerate() {
        entries.push((
            offset as u64 + 2,
            InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 1,
                content_root: file.id(),
                metadata_root: metadata(0o644),
            },
        ));
        references.push(file.id());
    }
    let leaf = FinalizedObject::new(
        ObjectRole::InodeLeaf,
        encode_inode_page(&InodePage::Leaf { entries }).unwrap(),
    )
    .unwrap()
    .with_references(references);
    let scope = scope_for_seed([9; 32]);
    let root = FilesystemRoot::new(profile_id(), scope, 1, leaf.id()).unwrap();
    let root = FinalizedObject::new(ObjectRole::FilesystemRoot, root.encode().unwrap())
        .unwrap()
        .with_references(vec![leaf.id()]);
    let root_id = root.id();
    objects.extend([directory, leaf, root]);
    let ids: Vec<ObjectId> = objects.iter().map(FinalizedObject::id).collect();
    let storage = Storage::new(handles.storage.clone() as Arc<dyn PackPersistence>).unwrap();
    let save = storage.begin_save().unwrap();
    for object in objects {
        save.accept(object).unwrap();
    }
    save.finish().unwrap();
    (root_id, ids)
}

/// A mark reads the heads of several packs in one call and then the record
/// headers of payload packs far larger than a head, so its ranged reads start
/// past the bytes it already holds. Every object of every tree must still be
/// reached: heads handed back in another order, or a ranged read served from
/// the wrong bytes, fail the closure.
#[test]
fn a_mark_reaches_every_record_of_payload_packs_larger_than_its_head() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "mark_ranges");
    let handles: PostgresHandles = schema.create(BINDING, CURSOR);
    let (genesis_root, mut saved) = save_tree(&handles, 1, 99);
    let (shop, work, _genesis) = project_with_genesis(&handles.history, 1, genesis_root);
    for seed in 2..=4u32 {
        let (root, ids) = save_tree(&handles, seed, 99);
        commit_with(&handles.history, work, root);
        publish(&handles.history, shop, work);
        saved.extend(ids);
    }
    let large: i64 = schema
        .admin()
        .query_one(
            "SELECT count(*) FROM pack WHERE domain<>0 AND length>4120",
            &[],
        )
        .unwrap()
        .get(0);
    assert!(
        large >= 4,
        "each tree has a payload pack past the head: {large}"
    );

    let mark = handles
        .history
        .mark()
        .expect("the mark reads past the head");
    for id in &saved {
        assert!(mark.objects().contains(*id), "a saved object is reached");
    }
    let mut distinct = saved.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(mark.objects().len(), distinct.len(), "and nothing else");
}

/// A mark's walk decodes one metadata pack and one inode leaf per Layer. It
/// announces a wave's packs so the server sends them in one statement, and
/// keeps the inode values a read brought along for the next leaves, so twice
/// the Layers cost the same statements. Reading a pack or a leaf's values per
/// round trip again adds statements with every Layer and fails here. Every
/// object must still be reached: a pack row or a value kept for the wrong
/// key fails the closure.
#[test]
fn a_mark_sends_the_same_statements_for_twice_the_layers() {
    let Some(url) = url() else { return };
    let mut statements = Vec::new();
    for (tag, layers) in [("mark_ahead_8", 8u32), ("mark_ahead_16", 16)] {
        let schema = Schema::new(url.clone(), tag);
        let handles: PostgresHandles = schema.create(BINDING, CURSOR);
        let (genesis_root, mut saved) = save_tree(&handles, 1, 20);
        let (shop, work, _genesis) = project_with_genesis(&handles.history, 1, genesis_root);
        for seed in 2..=layers {
            let (root, ids) = save_tree(&handles, seed, 20);
            commit_with(&handles.history, work, root);
            publish(&handles.history, shop, work);
            saved.extend(ids);
        }
        let before = handles.diagnostics().unwrap();
        let mark = handles.history.mark().expect("the mark");
        let sent = handles.diagnostics().unwrap().since(&before);
        for id in &saved {
            assert!(mark.objects().contains(*id), "a saved object is reached");
        }
        let mut distinct = saved.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(mark.objects().len(), distinct.len(), "and nothing else");
        statements.push(sent.statements);
    }
    assert_eq!(
        statements[0], statements[1],
        "a mark's statements do not grow with its Layers: {statements:?}"
    );
}

const POSTGRESQL_MARK: &str = include_str!("../sql/postgresql/queries/history/kept_node_mark.sql");

/// `(from kind, to kind, column)` of every edge the shared text follows: one
/// `SELECT '<from>',x.<id>,'<to>',x.<column> FROM ...` arm each.
fn shared_edges() -> std::collections::BTreeSet<(String, String, String)> {
    let edges = SHARED_MARK
        .split("edges(from_kind,from_id,to_kind,to_id) AS (")
        .nth(1)
        .and_then(|rest| rest.split("closure(kind,node_id) AS (").next())
        .expect("the shared text has an edges list");
    edges
        .lines()
        .filter_map(|line| line.trim().strip_prefix("SELECT '"))
        .map(|arm| {
            let fields: Vec<&str> = arm.split(',').collect();
            let column = fields[3].split_whitespace().next().unwrap();
            (
                fields[0].trim_matches('\'').to_owned(),
                fields[2].trim_matches('\'').to_owned(),
                column.split('.').nth(1).unwrap().to_owned(),
            )
        })
        .collect()
}

/// The same triples from the PostgreSQL text: each `('<to>',x.<column>)` row
/// of a `VALUES` list belongs to the kind its arm's `WHERE k.kind=` names.
fn postgresql_edges() -> std::collections::BTreeSet<(String, String, String)> {
    let mut edges = std::collections::BTreeSet::new();
    let mut rows: Vec<(String, String)> = Vec::new();
    for line in POSTGRESQL_MARK.lines().map(str::trim) {
        if let Some(row) = line.strip_prefix("('") {
            let (kind, rest) = row.split_once("',").unwrap();
            let column = rest.split('.').nth(1).unwrap();
            let column = column.split(')').next().unwrap();
            rows.push((kind.to_owned(), column.to_owned()));
        } else if let Some(arm) = line.strip_prefix("WHERE k.kind='") {
            let from = arm.split('\'').next().unwrap();
            for (to, column) in rows.drain(..) {
                edges.insert((from.to_owned(), to, column));
            }
        }
    }
    assert!(rows.is_empty(), "a VALUES list without its arm: {rows:?}");
    edges
}

/// The PostgreSQL closure is a second text of the shared one, so an edge
/// added to one file and not the other would let a rebuild on that backend
/// drop history the other keeps. The data test above reaches few of the
/// edges; this one compares the texts themselves: the same roots, the same
/// sixteen edges and the same final insert. It needs no server.
#[test]
fn the_postgresql_closure_follows_exactly_the_shared_edges() {
    let shared = shared_edges();
    assert_eq!(shared.len(), 16, "the shared text's edges: {shared:?}");
    assert_eq!(postgresql_edges(), shared);

    let squeeze = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    let roots = |text: &str, next: &str| squeeze(text.split(next).next().unwrap());
    assert_eq!(
        roots(POSTGRESQL_MARK, "closure(kind,node_id) AS ("),
        roots(SHARED_MARK, "edges(from_kind,from_id,to_kind,to_id) AS ("),
        "the roots"
    );
    let insert = |text: &str| squeeze(&text[text.find("INSERT INTO kept_node").unwrap()..]);
    assert_eq!(
        insert(POSTGRESQL_MARK),
        insert(SHARED_MARK),
        "the final insert"
    );
}
