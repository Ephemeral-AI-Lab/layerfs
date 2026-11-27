//! PostgreSQL runtime evidence for the GC additions that shipped without it:
//! T7.1's `kept_node` table and history closure, and T7.2's object closure
//! and live-heads digest.
//!
//! Each test creates one disposable schema and drives it through
//! `PostgresHandles`.
//!
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
use crate::support;

use layerfs_content::{construct_bytes, read_range, ObjectId};
use layerfs_history::HistoryCatalog;
use layerfs_persistence::internals::{postgresql_reads, PostgresRead};
use layerfs_persistence::{KeptNodeId, PostgresHandles};
use layerfs_storage::port::{PackRange, PersistenceError};
use layerfs_storage::{port::PackPersistence, Storage};
use layerfs_telemetry::timer::Timing;
use std::sync::Arc;
use support::pg::{
    commit, commit_with, counts, merge_request, project, project_with_genesis, publish, url, Schema,
};

const BINDING: &[u8] = b"postgresql-gc-mark";
const CURSOR: [u8; 32] = [76; 32];

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
const SHARED_MARK: &str = include_str!("../support/fixtures/kept_node_one_statement.sql");

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
    save_tree_with(handles, seed, files, 0)
}

/// [`save_tree`] with `pages` more directories, each one full page of long
/// names, so the Save's metadata pack is as long as the test needs.
fn save_tree_with(
    handles: &PostgresHandles,
    seed: u32,
    files: u32,
    pages: u32,
) -> (ObjectId, Vec<ObjectId>) {
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
    let wide: Vec<FinalizedObject> = (0..pages)
        .map(|page| {
            let mut rows: Vec<(layerfs_content::filesystem::path::PathName, u64)> = (0..30u32)
                .map(|index| {
                    let mut name = String::new();
                    for byte in bytes(seed.wrapping_mul(7919) + page * 31 + index, 100) {
                        name.push_str(&format!("{byte:02x}"));
                    }
                    (
                        layerfs_content::filesystem::path::PathName::new(&name).unwrap(),
                        u64::from(index) + 2,
                    )
                })
                .collect();
            rows.sort();
            FinalizedObject::new(
                ObjectRole::DirectoryLeaf,
                encode_directory_page(&DirectoryPage::Leaf { entries: rows }).unwrap(),
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
    // The directories' serials follow the files', so the rows stay in key order.
    for (offset, page) in wide.iter().enumerate() {
        entries.push((
            offset as u64 + 100_000,
            InodeValue {
                kind: InodeKind::Directory,
                namespace_ref_count: 1,
                content_root: page.id(),
                metadata_root: metadata(0o755),
            },
        ));
        references.push(page.id());
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
    objects.extend(wide);
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

/// Ranged reads return exactly the bytes asked for, whatever the server
/// fetches to produce them: ranges that touch, ranges a few kilobytes apart
/// (cut from one fetched span) and ranges far apart (their own spans), over
/// two packs in one statement. A range past its body, ranges out of order and
/// an absent pack are refused as before.
#[test]
fn ranged_reads_return_exactly_the_ranges_near_or_far_apart() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "mark_spans");
    let handles: PostgresHandles = schema.create(BINDING, CURSOR);
    save_tree(&handles, 1, 99);
    save_tree(&handles, 2, 99);
    let packs: Vec<(i64, Vec<u8>)> = schema
        .admin()
        .query(
            "SELECT pack_id,body FROM pack WHERE domain<>0 AND length>60000 ORDER BY pack_id LIMIT 2",
            &[],
        )
        .unwrap()
        .into_iter()
        .map(|row| (row.get(0), row.get(1)))
        .collect();
    assert_eq!(packs.len(), 2, "two payload packs past 60 KB");
    let range = |offset, length| PackRange { offset, length };
    let end = packs[0].1.len();
    let asked = vec![
        (
            packs[0].0,
            vec![
                range(0, 16),
                range(16, 1),
                range(100, 41),
                range(5_000, 10),
                range(13_009, 7),
                range(40_000, 100),
                range(end - 1, 1),
            ],
        ),
        (packs[1].0, vec![range(432, 50), range(50_000, 3)]),
    ];
    let outcome = postgresql_reads(&handles, &[PostgresRead::Ranges(asked.clone())]).unwrap();
    assert_eq!(outcome[0].statements, 1, "one statement for every range");
    let expected: Vec<Vec<u8>> = asked
        .iter()
        .zip(&packs)
        .flat_map(|((_, ranges), (_, body))| {
            ranges
                .iter()
                .map(|r| body[r.offset..r.offset + r.length].to_vec())
        })
        .collect();
    assert_eq!(outcome[0].bytes, expected);

    let refused = |ranges: Vec<(i64, Vec<PackRange>)>| {
        postgresql_reads(&handles, &[PostgresRead::Ranges(ranges)]).unwrap_err()
    };
    assert_eq!(
        refused(vec![(packs[0].0, vec![range(10, 4), range(end - 4, 8)])]),
        PersistenceError::Malformed,
        "a range past the body"
    );
    assert_eq!(
        refused(vec![(packs[0].0, vec![range(100, 4), range(50, 4)])]),
        PersistenceError::Malformed,
        "ranges out of order"
    );
    assert_eq!(
        refused(vec![(i64::MAX - 7, vec![range(0, 4)])]),
        PersistenceError::Missing,
        "an absent pack"
    );
}

/// A mark announces a wave's metadata packs and the server sends them in one
/// statement whatever their length, up to a bound in bytes. With metadata
/// packs past 32 KiB each, twice the Layers still cost the same statements;
/// reading every such pack by its own statement adds one per Layer.
#[test]
fn a_mark_reads_metadata_packs_longer_than_32_kib_ahead() {
    let Some(url) = url() else { return };
    let mut statements = Vec::new();
    for (tag, layers) in [("mark_wide_3", 3u32), ("mark_wide_6", 6)] {
        let schema = Schema::new(url.clone(), tag);
        let handles: PostgresHandles = schema.create(BINDING, CURSOR);
        let (genesis_root, mut saved) = save_tree_with(&handles, 1, 20, 16);
        let (shop, work, _genesis) = project_with_genesis(&handles.history, 1, genesis_root);
        for seed in 2..=layers {
            let (root, ids) = save_tree_with(&handles, seed, 20, 16);
            commit_with(&handles.history, work, root);
            publish(&handles.history, shop, work);
            saved.extend(ids);
        }
        let shortest: i64 = schema
            .admin()
            .query_one("SELECT min(length) FROM pack WHERE domain=0", &[])
            .unwrap()
            .get(0);
        assert!(
            shortest > 32 * 1024,
            "every metadata pack is long: {shortest}"
        );
        let before = handles.diagnostics().unwrap();
        let mark = handles.history.mark().expect("the mark");
        let sent = handles.diagnostics().unwrap().since(&before);
        for id in &saved {
            assert!(mark.objects().contains(*id), "a saved object is reached");
        }
        statements.push(sent.statements);
    }
    assert_eq!(
        statements[0], statements[1],
        "long metadata packs are read ahead too: {statements:?}"
    );
}

/// Once a walk has announced its reads, a pack row fetched for it answers
/// every later scoped read of that pack in the transaction: a walk that comes
/// back to a pack for a delta base pays no second round trip. Before any
/// announcement nothing is kept.
#[test]
fn an_announced_walk_fetches_each_pack_row_once() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "mark_held_packs");
    let handles: PostgresHandles = schema.create(BINDING, CURSOR);
    for seed in 1..=3 {
        save_tree(&handles, seed, 20);
    }
    let packs: Vec<(i64, Vec<u8>)> = schema
        .admin()
        .query(
            "SELECT pack_id,body FROM pack WHERE domain=0 ORDER BY pack_id",
            &[],
        )
        .unwrap()
        .into_iter()
        .map(|row| (row.get(0), row.get(1)))
        .collect();
    assert!(packs.len() >= 3, "three metadata packs: {}", packs.len());
    let (a, b, c) = (packs[0].0, packs[1].0, packs[2].0);
    let steps = [
        PostgresRead::Pack(a),
        PostgresRead::Pack(a),
        PostgresRead::Expect(vec![a, b]),
        PostgresRead::Pack(a),
        PostgresRead::Pack(b),
        PostgresRead::Pack(a),
        PostgresRead::Pack(c),
        PostgresRead::Pack(c),
        PostgresRead::Expect(vec![a, b, c]),
    ];
    let outcome = postgresql_reads(&handles, &steps).unwrap();
    let sent: Vec<u64> = outcome.iter().map(|step| step.statements).collect();
    assert_eq!(
        sent,
        [1, 1, 1, 0, 0, 0, 1, 0, 0],
        "unannounced reads fetch every time; an announced walk fetches a pack once"
    );
    for (step, body) in [(0, 0), (1, 0), (3, 0), (4, 1), (5, 0), (6, 2), (7, 2)] {
        assert_eq!(outcome[step].bytes, [packs[body].1.clone()], "step {step}");
    }
}

/// A transaction reads each inode value block once. A block asked for again
/// costs no statement; a call fetches only the blocks it lacks; blocks read
/// ahead answer later calls and are not handed back before they are asked
/// for. The values are those a plain read returns.
#[test]
fn a_transaction_reads_each_value_block_once() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "mark_held_values");
    let handles: PostgresHandles = schema.create(BINDING, CURSOR);
    for seed in 1..=4 {
        save_tree(&handles, seed, 20);
    }
    let blocks: Vec<(u32, u32)> = schema
        .admin()
        .query(
            "SELECT first_ordinal,last_ordinal FROM inode_value_block ORDER BY first_ordinal",
            &[],
        )
        .unwrap()
        .into_iter()
        .map(|row| (row.get::<_, i64>(0) as u32, row.get::<_, i64>(1) as u32))
        .collect();
    assert!(blocks.len() >= 4, "four value blocks: {blocks:?}");
    let last = blocks[blocks.len() - 1];
    let steps = [
        PostgresRead::Values(vec![blocks[0].0], 0),
        PostgresRead::Values(vec![blocks[0].1], 0),
        PostgresRead::Values(vec![blocks[0].1, blocks[2].0], 0),
        PostgresRead::Values(vec![blocks[1].0], u32::MAX),
        PostgresRead::Values(vec![last.1], 0),
        PostgresRead::Values(vec![blocks[1].0, blocks[2].0, last.0], u32::MAX),
    ];
    let outcome = postgresql_reads(&handles, &steps).unwrap();
    let sent: Vec<u64> = outcome.iter().map(|step| step.statements).collect();
    assert_eq!(sent, [1, 0, 1, 1, 0, 0], "each block is fetched once");
    let handed =
        |step: usize| -> Vec<u32> { outcome[step].values.iter().map(|row| row.ordinal).collect() };
    let block = |index: usize| -> Vec<u32> { (blocks[index].0..=blocks[index].1).collect() };
    assert_eq!(handed(0), block(0));
    assert_eq!(handed(1), block(0));
    assert_eq!(handed(2), [block(0), block(2)].concat());
    assert_eq!(
        handed(3),
        block(1),
        "blocks read ahead are kept, not handed back"
    );
    assert_eq!(handed(4), (last.0..=last.1).collect::<Vec<u32>>());
    // Every value equals the one a plain read of its ordinal returns.
    let all: Vec<u32> = blocks
        .iter()
        .flat_map(|(first, end)| *first..=*end)
        .collect();
    let mut plain = Vec::new();
    PackPersistence::inode_values(handles.storage.as_ref(), &all, &mut plain).unwrap();
    assert_eq!(plain.len(), all.len());
    for step in &outcome {
        for row in &step.values {
            let at = plain
                .binary_search_by_key(&row.ordinal, |known| known.ordinal)
                .expect("a stored ordinal");
            assert_eq!(row.value, plain[at].value, "ordinal {}", row.ordinal);
        }
    }
}

/// A round that installs nothing decides from the live packs alone: it
/// sends the server neither the visited object ids nor a dead set. The bytes
/// it binds beyond the mark's own stay far below the ids' width, and the
/// Store carries no mark state afterwards.
#[test]
fn a_none_round_sends_no_visited_ids() {
    use layerfs_persistence::{CompactPath, CompactRequest};
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "none_no_visited");
    // Its own catalog identity: the compaction's advisory key derives from it.
    let handles: PostgresHandles = schema.create(b"postgresql-gc-mark-none", CURSOR);
    let (genesis_root, mut saved) = save_tree(&handles, 1, 99);
    let (shop, work, _genesis) = project_with_genesis(&handles.history, 1, genesis_root);
    for seed in 2..=4u32 {
        let (root, ids) = save_tree(&handles, seed, 99);
        commit_with(&handles.history, work, root);
        publish(&handles.history, shop, work);
        saved.extend(ids);
    }
    saved.sort();
    saved.dedup();

    let before = handles.diagnostics().unwrap();
    let mark = handles.history.mark().expect("the mark");
    let marked = handles.diagnostics().unwrap().since(&before);
    assert_eq!(mark.objects().len(), saved.len());
    let before = handles.diagnostics().unwrap();
    let receipt = handles
        .compact(&CompactRequest {
            op_id: [7; 16],
            ..CompactRequest::default()
        })
        .expect("the round");
    let round = handles.diagnostics().unwrap().since(&before);
    assert_eq!(receipt.path, CompactPath::None);

    let ids = 36 * saved.len() as u64;
    let beyond = round.bound_bytes - marked.bound_bytes;
    assert!(
        beyond < ids / 2,
        "a none round binds {beyond} bytes beyond its mark; the visited ids alone are {ids}"
    );
    let mut admin = schema.admin();
    for table in ["gc_dead", "gc_op", "kept_node"] {
        let rows: i64 = admin
            .query_one(&format!("SELECT count(*) FROM {table}"), &[])
            .unwrap()
            .get(0);
        assert_eq!(rows, 0, "{table} is empty after a none round");
    }
}

/// A rebuild's visited ids are listed from the mark's reached set, a block
/// of the Store's locators at a time, and sent a bounded piece per
/// statement. With more live objects than one statement carries, and more
/// than one read of locators returns, the rebuilt Store must hold exactly
/// the objects the mark reached: an identity lost or repeated between two
/// pieces fails the copy's count or the mark that follows.
#[test]
fn a_rebuild_visits_more_objects_than_one_statement_carries() {
    use layerfs_persistence::{CompactPath, CompactRequest};
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "rebuild_visited_pieces");
    // Its own catalog identity: the compaction's advisory key derives from it.
    let handles: PostgresHandles = schema.create(b"postgresql-gc-mark-pieces", CURSOR);
    let (genesis_root, mut saved) = save_tree(&handles, 1, 99);
    let (shop, work, _genesis) = project_with_genesis(&handles.history, 1, genesis_root);
    for seed in 2..=92u32 {
        let (root, ids) = save_tree(&handles, seed, 99);
        commit_with(&handles.history, work, root);
        publish(&handles.history, shop, work);
        saved.extend(ids);
    }
    saved.sort();
    saved.dedup();
    assert!(saved.len() > 8_192 + 512, "live objects: {}", saved.len());
    // Unreferenced trees: wholly dead packs worth more than a tenth of the Store.
    let mut dead = Vec::new();
    for seed in 1_000..1_016u32 {
        dead.extend(save_tree(&handles, seed, 99).1);
    }
    dead.retain(|id| saved.binary_search(id).is_err());
    assert!(dead.len() > 1_000, "dead objects: {}", dead.len());

    let receipt = handles
        .compact(&CompactRequest {
            op_id: [9; 16],
            ..CompactRequest::default()
        })
        .expect("the round");
    assert_eq!(receipt.path, CompactPath::Rebuild);

    let mut admin = schema.admin();
    let located: i64 = admin
        .query_one("SELECT count(*) FROM object_location", &[])
        .unwrap()
        .get(0);
    assert_eq!(located, saved.len() as i64, "exactly the live objects");
    let gone: i64 = admin
        .query_one(
            "SELECT count(*) FROM object_location WHERE object_id = ANY($1::bytea[])",
            &[&dead
                .iter()
                .map(|id| id.as_bytes().as_slice())
                .collect::<Vec<_>>()],
        )
        .unwrap()
        .get(0);
    assert_eq!(gone, 0, "no dead object is located");
    let mark = handles
        .history
        .mark()
        .expect("the mark of the rebuilt Store");
    assert_eq!(mark.objects().len(), saved.len());
    for id in &saved {
        assert!(mark.objects().contains(*id), "a live object is reached");
    }
}

/// Objects the inode-table writer emits, in order.
#[derive(Default)]
struct Emitted(Vec<layerfs_content::FinalizedObject>);

impl layerfs_content::FinalizedConsumer for Emitted {
    fn accept(
        &mut self,
        object: layerfs_content::FinalizedObject,
    ) -> layerfs_content::ContentResult<()> {
        self.0.push(object);
        Ok(())
    }
}

/// Inodes of the table [`save_table`] writes: many leaves under one branch.
const TABLE_INODES: u64 = 20_000;

/// One Layer's tree in one Save: an inode table of [`TABLE_INODES`]
/// directories that all name one empty directory page, under a filesystem
/// root. With a `base` table only the inodes of `changes` are written anew,
/// at `version`: the product's writer emits the leaves they sit in, each
/// naming the leaf it replaces as its predecessor, and the branch above
/// them. Returns the table's root, the filesystem root and how many leaves
/// the Save stored as deltas.
fn save_table(
    handles: &PostgresHandles,
    base: Option<ObjectId>,
    changes: &[u64],
    version: u32,
) -> (ObjectId, ObjectId, u64) {
    use layerfs_content::filesystem::attributes::PortableMetadata;
    use layerfs_content::filesystem::directory::{encode_directory_page, DirectoryPage};
    use layerfs_content::filesystem::inode::apply_inode_values;
    use layerfs_content::filesystem::root::profile_id;
    use layerfs_content::filesystem::{
        scope_for_seed, FilesystemObjects, FilesystemRoot, MAXIMUM_SCRATCH_BYTES,
    };
    use layerfs_content::inode_leaf::{InodeKind, InodeMetadata, InodeValue};
    use layerfs_content::{FinalizedObject, ObjectRole};

    let directory = FinalizedObject::new(
        ObjectRole::DirectoryLeaf,
        encode_directory_page(&DirectoryPage::Leaf {
            entries: Vec::new(),
        })
        .unwrap(),
    )
    .unwrap();
    let value = InodeValue {
        kind: InodeKind::Directory,
        namespace_ref_count: 1,
        content_root: directory.id(),
        metadata_root: InodeMetadata::Inline(PortableMetadata {
            mode: 0o755,
            mtime_seconds: i64::from(version),
            mtime_nanoseconds: 0,
        }),
    };
    let storage = Storage::new(handles.storage.clone() as Arc<dyn PackPersistence>).unwrap();
    let reader = storage.reader().unwrap();
    let mut emitted = Emitted::default();
    let (table, _) = {
        let mut objects = FilesystemObjects::new(&reader, &mut emitted);
        apply_inode_values(
            &mut objects,
            base,
            changes.iter().map(|serial| Ok((*serial, Some(value)))),
            MAXIMUM_SCRATCH_BYTES,
        )
        .unwrap()
    };
    let root = FilesystemRoot::new(profile_id(), scope_for_seed([9; 32]), 1, table).unwrap();
    let root = FinalizedObject::new(ObjectRole::FilesystemRoot, root.encode().unwrap())
        .unwrap()
        .with_references(vec![table]);
    let root_id = root.id();
    let save = storage.begin_save().unwrap();
    save.accept(directory).unwrap();
    for object in emitted.0 {
        save.accept(object).unwrap();
    }
    save.accept(root).unwrap();
    let outcome = save.finish().unwrap();
    (table, root_id, outcome.pool.delta_leaves)
}

/// A mark rebuilds every inode leaf of a Layer, and a leaf stored as a delta
/// needs the locator of its base, which an earlier Layer's pack holds. The
/// lookups of one pack go to the server together: Layers that each rewrite
/// eight times the leaves cost a mark far fewer statements than one more per
/// leaf. One lookup per base again adds a statement for every rewritten
/// leaf of every Layer and fails here.
#[test]
fn a_mark_looks_up_the_delta_bases_of_a_pack_together() {
    let Some(url) = url() else { return };
    const VERSIONS: u32 = 6;
    let all: Vec<u64> = (1..=TABLE_INODES).collect();
    let mut statements = Vec::new();
    for (tag, leaves) in [("mark_bases_2", 2u64), ("mark_bases_16", 16)] {
        let schema = Schema::new(url.clone(), tag);
        let handles: PostgresHandles = schema.create(BINDING, CURSOR);
        let (mut table, genesis_root, _) = save_table(&handles, None, &all, 1);
        let (shop, work, _genesis) = project_with_genesis(&handles.history, 1, genesis_root);
        // Inodes far enough apart to sit in different leaves.
        let changes: Vec<u64> = (0..leaves)
            .map(|leaf| 1 + leaf * (TABLE_INODES / 16))
            .collect();
        for version in 2..=VERSIONS {
            let (next, root, deltas) = save_table(&handles, Some(table), &changes, version);
            assert_eq!(
                deltas, leaves,
                "each change rewrites its own leaf as a delta on the leaf before"
            );
            commit_with(&handles.history, work, root);
            publish(&handles.history, shop, work);
            table = next;
        }
        let before = handles.diagnostics().unwrap();
        let mark = handles.history.mark().expect("the mark");
        let sent = handles.diagnostics().unwrap().since(&before);
        assert!(mark.objects().contains(table), "the last table is reached");
        statements.push(sent.statements);
    }
    let more_bases = (16 - 2) * u64::from(VERSIONS - 1);
    assert!(
        statements[1] < statements[0] + more_bases / 2,
        "{more_bases} more delta bases cost a statement each: {statements:?}"
    );
}

const POSTGRESQL_MARK: &str = include_str!("../../sql/postgresql/gc/kept_node_mark.sql");

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
