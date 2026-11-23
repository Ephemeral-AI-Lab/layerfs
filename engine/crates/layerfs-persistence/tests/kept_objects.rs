//! #41 T7.2: the §6.2 object closure, `keptWaste` and the §6.4 live-heads
//! digest. A mark keeps every object reachable from a retained Commit's or
//! Layer's root through logical references and delta bases, records the packs
//! those objects live in, reports the bytes of records in retained packs that
//! nothing reaches, and hashes the live heads. An unreferenced record, a record
//! only kept as a delta base, a deleted project and a moved head each move the
//! answer; nothing is compacted.
#![cfg(any(target_os = "macos", target_os = "linux"))]
mod support;

use layerfs_content::{
    encode_whole_file_payload,
    file::mapping::{self, ExtentNode, ExtentSlice, FileState},
    filesystem::attributes::PortableMetadata,
    inode_leaf::{
        encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeMetadata, InodeValue,
        LEAF_ROW_BYTES,
    },
    FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_history::{
    AddLayerOutcome, AddLayerRequest, BranchId, CommitMeta, CommitOutcome, CommitRecord,
    CommitRequest, CommitVia, ForkRequest, ForkSource, HistoryCatalog, LayerRecord, LayerStackId,
    LayerStackRecord, StackInitialization,
};
use layerfs_persistence::{
    thread_work, CompactPath, CompactRequest, PersistenceConfig, SqliteHandles,
    SqlitePersistenceProfile,
};
use layerfs_storage::{
    encoding::{encode_full, encode_prefix, framed_record, raw_payload, CompressionWorkspace},
    location::{InodeValueRow, ObjectLocation, PackDomain},
    pack::{assemble, build_group, layout, PackLane},
    port::{PackPersistence, Publication, PublishedPack, Reserve},
    save::SaveProfile,
    StorageCapacities,
};
use std::path::Path;
use std::sync::Arc;
use support::doctor::Fixture;
use support::{branch_id, name, root};

fn initialize(handles: &SqliteHandles, tag: u8, genesis: ObjectId) -> LayerStackRecord {
    handles
        .history
        .initialize_layerstack(&StackInitialization {
            logical: 10,
            stack: support::stack(tag),
            name: name(&format!("project-{tag}")),
            scope: root(200),
            profile: root(201),
            genesis_root: genesis,
        })
        .unwrap()
}

fn fork(handles: &SqliteHandles, stack: LayerStackId, branch: BranchId, tag: u8) {
    let record = handles.history.layer_stack(stack).unwrap().unwrap();
    handles
        .history
        .fork(&ForkRequest {
            stack,
            branch,
            name: name(&format!("branch-{tag}")),
            source: ForkSource::Layer(record.head_layer),
        })
        .unwrap();
}

fn commit(handles: &SqliteHandles, branch: BranchId, candidate_root: ObjectId) -> CommitRecord {
    let snapshot = handles.history.branch(branch).unwrap().unwrap();
    let expected_root = match snapshot.head_commit {
        Some(head) => handles.history.commit(head).unwrap().unwrap().root,
        None => {
            handles
                .history
                .layer(snapshot.base_layer)
                .unwrap()
                .unwrap()
                .root
        }
    };
    match handles
        .history
        .commit_branch(&CommitRequest {
            branch,
            expected_head: snapshot.head_commit,
            expected_base: snapshot.base_layer,
            expected_root,
            candidate_root,
            profile: root(201),
            scope: root(200),
            second_parent: None,
            meta: CommitMeta::new(CommitVia::Files),
            force: false,
            required_head: None,
        })
        .unwrap()
    {
        CommitOutcome::Committed { record, .. } => *record,
        other => panic!("commit: {other:?}"),
    }
}

fn publish(handles: &SqliteHandles, stack: LayerStackId, branch: BranchId) -> LayerRecord {
    let branch_record = handles.history.branch(branch).unwrap().unwrap();
    let stack_record = handles.history.layer_stack(stack).unwrap().unwrap();
    match handles
        .history
        .add_layer(&AddLayerRequest {
            stack,
            branch,
            commit: branch_record.head_commit.expect("a committed Branch"),
            expected_stack_head: stack_record.head_layer,
            expected_branch_base: branch_record.base_layer,
        })
        .unwrap()
    {
        AddLayerOutcome::Added { layer } => layer,
        other => panic!("publish: {other:?}"),
    }
}

fn whole_file(f: &Fixture, bytes: &[u8]) -> FinalizedObject {
    f.put(
        ObjectRole::WholeFile,
        encode_whole_file_payload(bytes).unwrap(),
    )
}

fn request(op: u8) -> CompactRequest {
    CompactRequest {
        op_id: [op; 16],
        ..CompactRequest::default()
    }
}

fn reopen(store: &Path) -> SqliteHandles {
    SqliteHandles::open_writable(
        PersistenceConfig::sqlite(store.to_path_buf())
            .with_sqlite_profile(SqlitePersistenceProfile::Normal),
        b"doctor-adversarial",
        [71; 32],
    )
    .unwrap()
}

fn pack_of(f: &Fixture, id: ObjectId) -> i64 {
    f.raw()
        .query_row(
            "SELECT pack_id FROM object_location WHERE object_id=?1",
            rusqlite::params![id.as_bytes().as_slice()],
            |row| row.get(0),
        )
        .unwrap()
}

/// Publishes one pack holding each entry as its own group, so a retained pack
/// can hold both a reached and an unreached record.
fn publish_groups(f: &Fixture, objects: &[FinalizedObject]) -> (i64, Vec<u8>) {
    let capacities = StorageCapacities::from_policy(f.policy).unwrap();
    let mut workspace = CompressionWorkspace::new().unwrap();
    let mut profile = SaveProfile::default();
    let groups: Vec<_> = objects
        .iter()
        .map(|object| {
            let record = encode_full(
                object.canonical(),
                object.role(),
                &capacities,
                &mut workspace,
                &mut profile,
            )
            .unwrap();
            build_group(PackLane::WholeFile, &[record.record], None).unwrap()
        })
        .collect();
    let body = assemble(PackLane::WholeFile, &groups).unwrap();
    let reserved = f
        .handles
        .storage
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })
        .unwrap();
    f.handles
        .storage
        .publish(&Publication {
            packs: vec![PublishedPack::seal(
                reserved.first_pack_id,
                PackDomain::Payload,
                Arc::new(body.clone()),
            )],
            objects: objects
                .iter()
                .enumerate()
                .map(|(group, object)| ObjectLocation {
                    object_id: object.id(),
                    role: object.role(),
                    canonical_length: object.canonical_len(),
                    pack_id: reserved.first_pack_id,
                    group_number: group,
                    record_number: 0,
                })
                .collect(),
            ..Publication::default()
        })
        .unwrap();
    (reserved.first_pack_id, body)
}

/// Publishes one Raw group holding every object as its own record, so a
/// retained group can mix a reached and an unreached record.
fn publish_group_records(f: &Fixture, objects: &[FinalizedObject]) -> (i64, Vec<u8>) {
    let capacities = StorageCapacities::from_policy(f.policy).unwrap();
    let mut workspace = CompressionWorkspace::new().unwrap();
    let mut profile = SaveProfile::default();
    let records: Vec<_> = objects
        .iter()
        .map(|object| {
            encode_full(
                object.canonical(),
                object.role(),
                &capacities,
                &mut workspace,
                &mut profile,
            )
            .unwrap()
            .record
        })
        .collect();
    let group = build_group(PackLane::WholeFile, &records, None).unwrap();
    assert_eq!(group.codec, layout::GroupCodec::Raw);
    let body = assemble(PackLane::WholeFile, &[group]).unwrap();
    let reserved = f
        .handles
        .storage
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })
        .unwrap();
    f.handles
        .storage
        .publish(&Publication {
            packs: vec![PublishedPack::seal(
                reserved.first_pack_id,
                PackDomain::Payload,
                Arc::new(body.clone()),
            )],
            objects: objects
                .iter()
                .enumerate()
                .map(|(record, object)| ObjectLocation {
                    object_id: object.id(),
                    role: object.role(),
                    canonical_length: object.canonical_len(),
                    pack_id: reserved.first_pack_id,
                    group_number: 0,
                    record_number: record,
                })
                .collect(),
            ..Publication::default()
        })
        .unwrap();
    (reserved.first_pack_id, body)
}

#[test]
fn a_dead_record_in_a_retained_raw_group_is_exact_kept_waste() {
    let f = Fixture::new();
    let live = FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(b"the reached record").unwrap(),
    )
    .unwrap();
    let dead = FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(b"the unreached record in the same group").unwrap(),
    )
    .unwrap();
    let (pack, body) = publish_group_records(&f, &[live.clone(), dead.clone()]);
    let header = layout::parse_header(&body).unwrap();
    assert_eq!(header.group_count, 1);
    let view = layout::group_view(&body, header, 0).unwrap();
    let group = &body[view.start..view.end];
    let expected = framed_record(group, 1).unwrap().len() as u64;
    assert!(expected > 0);

    let stack = initialize(&f.handles, 1, live.id());
    fork(&f.handles, stack.id, branch_id(1), 1);

    let mark = f.handles.history.mark().unwrap();
    let objects = mark.objects();
    assert!(objects.contains(live.id()));
    assert!(!objects.contains(dead.id()));
    assert_eq!(objects.packs(), &[pack]);
    assert_eq!(
        objects.waste().dead_in_retained_packs_bytes(),
        expected,
        "only the dead record's framed extent is kept waste"
    );
    // A whole-file record names no other object, so the walk decodes no
    // payload; it still acquires the record's group header, which is body
    // bytes on the wire and is what this metric measures.
    assert!(
        mark.mark_bytes() > 0,
        "the walk reads the record's group extent"
    );
}

/// A rebuild writes a retained pack again without the records nothing
/// reaches. The mark before it attributes the dead record in the retained Raw
/// group and names the pack; the mark after it finds no dead record there.
#[test]
fn a_dead_record_in_a_retained_raw_group_is_dropped_by_a_rebuild() {
    let f = Fixture::new();
    let live = FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(b"the reached record").unwrap(),
    )
    .unwrap();
    let dead = FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(b"the unreached record in the same group").unwrap(),
    )
    .unwrap();
    let (pack, body) = publish_group_records(&f, &[live.clone(), dead.clone()]);
    let header = layout::parse_header(&body).unwrap();
    let view = layout::group_view(&body, header, 0).unwrap();
    let expected = framed_record(&body[view.start..view.end], 1).unwrap().len() as u64;

    let stack = initialize(&f.handles, 1, live.id());
    fork(&f.handles, stack.id, branch_id(1), 1);

    // Unreached whole-file packs make the §3 rebuild threshold.
    for index in 0..12u8 {
        let payload = vec![0xA0u8.wrapping_add(index); 4096];
        let _ = whole_file(&f, &payload);
    }

    let before = f.handles.history.mark().unwrap();
    assert_eq!(
        before.objects().waste().dead_in_retained_packs_bytes(),
        expected,
        "the dead record is kept waste before the rebuild"
    );
    assert_eq!(
        before.objects().partly_dead_packs(),
        &[(pack, expected)],
        "the mark names the pack that holds the dead record"
    );

    let store = f.temp.join("store.sqlite");
    let receipt = f.handles.compact(&request(0x91)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    let reopened = reopen(&store);
    let after = reopened.history.mark().unwrap();
    assert_eq!(
        receipt.kept_waste.dead_in_retained_packs_bytes, 0,
        "the report no longer keeps the dead record"
    );
    assert_eq!(
        after.objects().waste().dead_in_retained_packs_bytes(),
        0,
        "the rebuilt pack holds only the reached record"
    );
    assert!(after.objects().partly_dead_packs().is_empty());
    assert!(
        after.objects().contains(live.id()),
        "the reached record survives the rebuild"
    );
    assert!(
        after.objects().packs().contains(&pack),
        "the retained pack keeps its id"
    );
}

#[test]
fn a_retained_tree_and_inode_are_kept_with_their_packs() {
    let f = Fixture::new();
    let chunk = f
        .put(
            ObjectRole::Chunk,
            mapping::encode_chunk_object(b"tree payload").unwrap(),
        )
        .id();
    let leaf = f
        .put(
            ObjectRole::ExtentLeaf,
            mapping::encode_node(
                &ExtentNode::Leaf {
                    subtree_logical_bytes: 12,
                    extents: vec![ExtentSlice::new(chunk, 0, 12).unwrap()],
                },
                true,
            )
            .unwrap(),
        )
        .id();
    let tree = f
        .put(
            ObjectRole::FileState,
            mapping::encode_file_state(FileState {
                logical_len: 12,
                extent_count: 1,
                tree_level: 0,
                profile_id: mapping::profile_id(),
                mapping_root: leaf,
            })
            .unwrap(),
        )
        .id();
    let value = encode_inode_value(InodeValue {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 1,
        content_root: tree,
        metadata_root: InodeMetadata::Inline(PortableMetadata {
            mode: 0o644,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
        }),
    });
    let inode = f.inode(
        InodeLeaf {
            subtree_bytes: LEAF_ROW_BYTES as u64,
            rows: vec![InodeLeafRow { serial: 7, value }],
        }
        .encode()
        .unwrap(),
        vec![value],
    );

    let stack = initialize(&f.handles, 1, tree);
    let work = branch_id(1);
    fork(&f.handles, stack.id, work, 1);
    commit(&f.handles, work, inode);
    publish(&f.handles, stack.id, work);

    let mark = f.handles.history.mark().unwrap();
    let objects = mark.objects();
    for id in [chunk, leaf, tree, inode] {
        assert!(objects.contains(id), "object {id} is retained");
        assert!(
            objects.packs().contains(&pack_of(&f, id)),
            "the pack of {id} is retained"
        );
    }
    let blocks: i64 = f
        .raw()
        .query_row(
            "SELECT COALESCE(SUM(length(body)),0) FROM inode_value_block",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(objects.waste().inode_blocks_bytes(), blocks as u64);
    assert_eq!(objects.waste().dead_in_retained_packs_bytes(), 0);
    // §11 `markBytes`: the reference-bearing tree pages are decoded from their
    // pack bodies, each live pack counted once. The measurement is stable and
    // bounded by the Store's stored bytes.
    let total: i64 = f
        .raw()
        .query_row("SELECT COALESCE(SUM(length),0) FROM pack", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert!(
        mark.mark_bytes() > 0,
        "the tree walk reads reference-bearing pack bodies"
    );
    assert!(mark.mark_bytes() <= u64::try_from(total).unwrap());
    assert_eq!(
        mark.mark_bytes(),
        f.handles.history.mark().unwrap().mark_bytes(),
        "the mark reads the same body bytes once on a second mark"
    );
}

#[test]
fn an_unreferenced_record_in_a_retained_pack_is_kept_waste() {
    let f = Fixture::new();
    let live = FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(b"the live record").unwrap(),
    )
    .unwrap();
    let dead = FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(b"nobody reaches this record").unwrap(),
    )
    .unwrap();
    let (pack, body) = publish_groups(&f, &[live.clone(), dead.clone()]);
    let header = layout::parse_header(&body).unwrap();
    let dead_view = layout::group_view(&body, header, 1).unwrap();
    let expected = u64::try_from(dead_view.end - dead_view.start).unwrap();

    let stack = initialize(&f.handles, 1, live.id());
    fork(&f.handles, stack.id, branch_id(1), 1);

    let mark = f.handles.history.mark().unwrap();
    let objects = mark.objects();
    assert!(objects.contains(live.id()));
    assert!(!objects.contains(dead.id()));
    assert_eq!(objects.packs(), &[pack]);
    assert_eq!(
        objects.waste().dead_in_retained_packs_bytes(),
        expected,
        "the unreferenced group's stored extent is the kept waste"
    );
    assert!(objects.waste().delta_base_references().is_empty());
}

#[test]
fn the_digest_is_stable_and_moves_with_a_head() {
    let f = Fixture::new();
    let start = whole_file(&f, b"first root").id();
    let stack = initialize(&f.handles, 1, start);
    let work = branch_id(1);
    fork(&f.handles, stack.id, work, 1);

    let first = f.handles.history.mark().unwrap();
    let second = f.handles.history.mark().unwrap();
    assert_eq!(first.live_heads_digest(), second.live_heads_digest());
    assert_eq!(first.objects(), second.objects());

    let next = whole_file(&f, b"second root").id();
    commit(&f.handles, work, next);
    let committed = f.handles.history.mark().unwrap();
    assert_ne!(
        committed.live_heads_digest(),
        first.live_heads_digest(),
        "a moved Branch head changes the digest"
    );

    publish(&f.handles, stack.id, work);
    let published = f.handles.history.mark().unwrap();
    assert_ne!(
        published.live_heads_digest(),
        committed.live_heads_digest(),
        "a moved Stack head changes the digest"
    );
}

#[test]
fn a_deleted_project_is_not_kept_unless_a_retained_root_reaches_it() {
    let f = Fixture::new();
    // Stack one's genesis root is a chunk stack two's tree still references.
    let shared = f
        .put(
            ObjectRole::Chunk,
            mapping::encode_chunk_object(b"shared payload").unwrap(),
        )
        .id();
    let only_one = whole_file(&f, b"only the deleted project").id();
    let leaf = f
        .put(
            ObjectRole::ExtentLeaf,
            mapping::encode_node(
                &ExtentNode::Leaf {
                    subtree_logical_bytes: 14,
                    extents: vec![ExtentSlice::new(shared, 0, 14).unwrap()],
                },
                true,
            )
            .unwrap(),
        )
        .id();
    let other = whole_file(&f, b"the live project").id();

    let one = initialize(&f.handles, 1, shared);
    let first = branch_id(1);
    fork(&f.handles, one.id, first, 1);
    commit(&f.handles, first, only_one);
    publish(&f.handles, one.id, first);

    let two = initialize(&f.handles, 2, other);
    let second = branch_id(2);
    fork(&f.handles, two.id, second, 2);
    commit(&f.handles, second, leaf);
    publish(&f.handles, two.id, second);

    f.handles.history.mark_stack_deleted(one.id, 2_000).unwrap();
    let mark = f.handles.history.mark().unwrap();
    let objects = mark.objects();
    assert!(
        objects.contains(shared),
        "the deleted project's object a retained root still references stays"
    );
    assert!(objects.contains(leaf));
    assert!(objects.contains(other));
    assert!(
        !objects.contains(only_one),
        "an object only the deleted project reached is collected"
    );
    assert!(!objects.packs().contains(&pack_of(&f, only_one)));
}

#[test]
fn a_delta_base_is_enqueued_and_reported() {
    let f = Fixture::new();
    let base = whole_file(&f, b"0123456789abcdef base payload");
    let target = FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(b"0123456789abcdef base payload, extended").unwrap(),
    )
    .unwrap();
    let capacities = StorageCapacities::from_policy(f.policy).unwrap();
    let record = encode_prefix(
        target.canonical(),
        target.role(),
        base.id(),
        raw_payload(base.canonical(), base.role()).unwrap(),
        &capacities,
        &mut CompressionWorkspace::new().unwrap(),
        &mut SaveProfile::default(),
    )
    .unwrap();
    let body = assemble(
        PackLane::WholeFile,
        &[build_group(PackLane::WholeFile, &[record.record], None).unwrap()],
    )
    .unwrap();
    let reserved = f
        .handles
        .storage
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })
        .unwrap();
    f.handles
        .storage
        .publish(&Publication {
            packs: vec![PublishedPack::seal(
                reserved.first_pack_id,
                PackDomain::Payload,
                Arc::new(body),
            )],
            objects: vec![ObjectLocation {
                object_id: target.id(),
                role: target.role(),
                canonical_length: target.canonical_len(),
                pack_id: reserved.first_pack_id,
                group_number: 0,
                record_number: 0,
            }],
            ..Publication::default()
        })
        .unwrap();

    let stack = initialize(&f.handles, 1, target.id());
    fork(&f.handles, stack.id, branch_id(1), 1);

    let mark = f.handles.history.mark().unwrap();
    let objects = mark.objects();
    assert!(objects.contains(target.id()));
    assert!(
        objects.contains(base.id()),
        "the delta base is reached through the stored record"
    );
    assert!(objects.packs().contains(&pack_of(&f, target.id())));
    assert!(
        objects.packs().contains(&pack_of(&f, base.id())),
        "the base's pack is enqueued too"
    );
    assert_eq!(
        objects.waste().delta_base_references(),
        &[base.id()],
        "the base is kept only because the delta names it"
    );
    assert!(objects.waste().delta_bases_bytes() > 0);
}

/// A whole-file object of `length` payload bytes that do not compress, so its
/// stored record is about as wide as its payload.
fn noise_file(seed: u64, length: usize) -> FinalizedObject {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let bytes: Vec<u8> = (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect();
    FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(&bytes).unwrap(),
    )
    .unwrap()
}

/// Publishes one whole-file pack of Raw groups, each holding the given
/// objects as its records, and returns the pack and each group's stored bytes.
fn publish_record_groups(f: &Fixture, groups: &[Vec<FinalizedObject>]) -> (i64, Vec<usize>) {
    let capacities = StorageCapacities::from_policy(f.policy).unwrap();
    let mut workspace = CompressionWorkspace::new().unwrap();
    let mut profile = SaveProfile::default();
    let encoded: Vec<_> = groups
        .iter()
        .map(|objects| {
            let records: Vec<_> = objects
                .iter()
                .map(|object| {
                    encode_full(
                        object.canonical(),
                        object.role(),
                        &capacities,
                        &mut workspace,
                        &mut profile,
                    )
                    .unwrap()
                    .record
                })
                .collect();
            build_group(PackLane::WholeFile, &records, None).unwrap()
        })
        .collect();
    let extents = encoded.iter().map(|group| group.bytes.len()).collect();
    let body = assemble(PackLane::WholeFile, &encoded).unwrap();
    let reserved = f
        .handles
        .storage
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })
        .unwrap();
    f.handles
        .storage
        .publish(&Publication {
            packs: vec![PublishedPack::seal(
                reserved.first_pack_id,
                PackDomain::Payload,
                Arc::new(body),
            )],
            objects: groups
                .iter()
                .enumerate()
                .flat_map(|(group, objects)| {
                    objects
                        .iter()
                        .enumerate()
                        .map(move |(record, object)| ObjectLocation {
                            object_id: object.id(),
                            role: object.role(),
                            canonical_length: object.canonical_len(),
                            pack_id: reserved.first_pack_id,
                            group_number: group,
                            record_number: record,
                        })
                })
                .collect(),
            ..Publication::default()
        })
        .unwrap();
    (reserved.first_pack_id, extents)
}

/// Makes every object of `roots` the root of a retained Commit.
fn retain(f: &Fixture, roots: &[FinalizedObject]) {
    let stack = initialize(&f.handles, 1, roots[0].id());
    fork(&f.handles, stack.id, branch_id(1), 1);
    for root in &roots[1..] {
        commit(&f.handles, branch_id(1), root.id());
    }
}

/// The record headers of a group whose wanted records sit close together are
/// read in one piece, not one read per record: on the real 4.9 GB Store the
/// per-record reads were one disk wait per database page.
#[test]
fn close_records_of_a_group_are_read_in_one_piece() {
    // With `lead`, the group of many records starts past the bytes read as
    // the pack's head, so its record directory is not held when the group is
    // first met. Without it the directory comes with the head.
    for lead in [false, true] {
        let f = Fixture::new();
        let files: Vec<_> = (0..48).map(|seed| noise_file(seed + 1, 600)).collect();
        let mut groups = Vec::new();
        if lead {
            groups.push(vec![noise_file(1000, 1024)]);
        }
        groups.push(files.clone());
        let (pack, extents) = publish_record_groups(&f, &groups);
        let extent = *extents.last().unwrap();
        assert!(extent > 48 * 600, "the group is far wider than the head");
        retain(&f, &files);

        let before = thread_work();
        let mark = f.handles.history.mark().unwrap();
        let work = thread_work().since(&before);
        for file in &files {
            assert!(mark.objects().contains(file.id()));
        }
        assert!(mark.objects().packs().contains(&pack));
        // The pack's head, then the rest of the group in one read. Reading
        // the directory and then each header would be about fifty reads.
        assert_eq!(work.blob_read_calls, 2, "lead group: {lead}");
        assert!(
            work.blob_read_bytes <= (extent + 432 + 512) as u64,
            "no more than the group and the head: {} of {extent}",
            work.blob_read_bytes
        );
        let dead = if lead { extents[0] as u64 } else { 0 };
        assert_eq!(mark.objects().waste().dead_in_retained_packs_bytes(), dead);
    }
}

/// A record far from the other wanted records of its group is still read by
/// its header alone: a group of large records is not read through.
#[test]
fn a_far_record_of_a_group_is_read_by_its_header() {
    let f = Fixture::new();
    let files: Vec<_> = (0..3).map(|seed| noise_file(seed + 1, 18_000)).collect();
    let (pack, extents) = publish_record_groups(&f, &[vec![noise_file(1000, 1024)], files.clone()]);
    assert!(extents[1] > 3 * 18_000);
    retain(&f, &files[2..]);

    let before = thread_work();
    let mark = f.handles.history.mark().unwrap();
    let work = thread_work().since(&before);
    assert!(mark.objects().contains(files[2].id()));
    assert!(!mark.objects().contains(files[0].id()));
    assert!(mark.objects().packs().contains(&pack));
    // The head, the group's directory, the one header, and the directory
    // again for the kept-waste split of the group's two dead records.
    assert!(
        work.blob_read_bytes < 2048,
        "headers only: {} bytes of a {} byte group",
        work.blob_read_bytes,
        extents[1]
    );
    assert!(
        mark.objects().waste().dead_in_retained_packs_bytes() > 2 * 18_000,
        "the two unreached records and the lead group are dead bytes"
    );
}

/// One pooled leaf over `rows`, whose values are the stored `ordinals`; the
/// record stores the values in `stored` with it.
fn pooled_leaf(
    f: &Fixture,
    rows: Vec<InodeLeafRow>,
    ordinals: &[u32],
    stored: Vec<InodeValueRow>,
) -> ObjectId {
    let canonical = InodeLeaf {
        subtree_bytes: (LEAF_ROW_BYTES * rows.len()) as u64,
        rows,
    }
    .encode()
    .unwrap();
    let object = FinalizedObject::new(ObjectRole::InodeLeaf, canonical).unwrap();
    let pooled = layerfs_content::inode_leaf::pooled_body(object.canonical(), ordinals).unwrap();
    let record = layerfs_storage::encoding::pool::leaf::encode_full(&pooled).unwrap();
    f.record(&object, PackLane::Ordinary, record, stored);
    object.id()
}

#[test]
fn leaves_naming_values_of_one_block_cost_one_value_read() {
    let f = Fixture::new();
    let contents: Vec<ObjectId> = (1..=3u64)
        .map(|serial| whole_file(&f, &serial.to_le_bytes()).id())
        .collect();
    let values: Vec<[u8; 73]> = contents
        .iter()
        .map(|content| {
            encode_inode_value(InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 1,
                content_root: *content,
                metadata_root: InodeMetadata::Inline(PortableMetadata {
                    mode: 0o644,
                    mtime_seconds: 0,
                    mtime_nanoseconds: 0,
                }),
            })
        })
        .collect();
    let row = |index: usize| InodeLeafRow {
        serial: index as u64 + 1,
        value: values[index],
    };
    // One leaf stores all three values together, so they share one block.
    let first = f
        .handles
        .storage
        .reserve(Reserve {
            packs: 0,
            ordinals: 3,
        })
        .unwrap()
        .first_ordinal;
    let ordinals = [first, first + 1, first + 2];
    pooled_leaf(
        &f,
        vec![row(0), row(1), row(2)],
        &ordinals,
        ordinals
            .iter()
            .zip(&values)
            .map(|(ordinal, value)| InodeValueRow {
                ordinal: *ordinal,
                value: *value,
            })
            .collect(),
    );
    // Later leaves each keep one of those values by its ordinal, as the leaf
    // of a later Commit keeps every value it did not change.
    let later: Vec<ObjectId> = (0..3)
        .map(|index| pooled_leaf(&f, vec![row(index)], &ordinals[index..=index], vec![]))
        .collect();
    let stack = initialize(&f.handles, 1, later[0]);
    let work = branch_id(1);
    fork(&f.handles, stack.id, work, 1);
    commit(&f.handles, work, later[1]);
    commit(&f.handles, work, later[2]);

    layerfs_persistence::profile_statements();
    let mark = f.handles.history.mark().unwrap();
    let profile = layerfs_persistence::take_statement_profile();
    for id in &later {
        assert!(mark.objects().contains(*id), "leaf {id} is retained");
    }
    for content in &contents {
        assert!(mark.objects().contains(*content), "content {content}");
    }
    // The first leaf's read decodes the block; its other values answer the
    // two leaves that follow without a statement.
    let reads: u64 = profile
        .iter()
        .filter(|(sql, _)| sql.starts_with("SELECT b.first_ordinal,b.last_ordinal,"))
        .map(|(_, cost)| cost.calls)
        .sum();
    assert_eq!(reads, 1, "{profile:?}");
}

/// Publishes one Ordinary pack whose single Raw group holds `objects`, each
/// as its own record, and then `padding` bytes no locator names.
fn publish_tree_pack(f: &Fixture, objects: &[FinalizedObject], padding: usize) -> i64 {
    let capacities = StorageCapacities::from_policy(f.policy).unwrap();
    let mut workspace = CompressionWorkspace::new().unwrap();
    let mut profile = SaveProfile::default();
    let mut records: Vec<_> = objects
        .iter()
        .map(|object| {
            let encoded = encode_full(
                object.canonical(),
                object.role(),
                &capacities,
                &mut workspace,
                &mut profile,
            )
            .unwrap();
            assert_eq!(encoded.lane, PackLane::Ordinary);
            encoded.record
        })
        .collect();
    records.push(vec![0; padding]);
    let group = build_group(PackLane::Ordinary, &records, None).unwrap();
    assert_eq!(group.codec, layout::GroupCodec::Raw);
    let body = assemble(PackLane::Ordinary, &[group]).unwrap();
    let reserved = f
        .handles
        .storage
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })
        .unwrap();
    f.handles
        .storage
        .publish(&Publication {
            packs: vec![PublishedPack::seal(
                reserved.first_pack_id,
                PackDomain::Metadata,
                Arc::new(body),
            )],
            objects: objects
                .iter()
                .enumerate()
                .map(|(record, object)| ObjectLocation {
                    object_id: object.id(),
                    role: object.role(),
                    canonical_length: object.canonical_len(),
                    pack_id: reserved.first_pack_id,
                    group_number: 0,
                    record_number: record,
                })
                .collect(),
            ..Publication::default()
        })
        .unwrap();
    reserved.first_pack_id
}

#[test]
fn a_tree_written_into_one_pack_is_decoded_in_one_visit_of_it() {
    // More metadata packs than the walk's 2 MiB of pack bodies can hold at
    // once, each holding one Commit's file state and the extent leaf below it.
    const PACKS: usize = 80;
    const PADDING: usize = 32_000;
    let f = Fixture::new();
    let chunk = f
        .put(
            ObjectRole::Chunk,
            mapping::encode_chunk_object(&[7u8; 200]).unwrap(),
        )
        .id();
    let mut leaves = Vec::new();
    let mut states = Vec::new();
    for length in 1..=PACKS as u64 {
        let leaf = FinalizedObject::new(
            ObjectRole::ExtentLeaf,
            mapping::encode_node(
                &ExtentNode::Leaf {
                    subtree_logical_bytes: length,
                    extents: vec![ExtentSlice::new(chunk, 0, length as u32).unwrap()],
                },
                true,
            )
            .unwrap(),
        )
        .unwrap();
        let state = FinalizedObject::new(
            ObjectRole::FileState,
            mapping::encode_file_state(FileState {
                logical_len: length,
                extent_count: 1,
                tree_level: 0,
                profile_id: mapping::profile_id(),
                mapping_root: leaf.id(),
            })
            .unwrap(),
        )
        .unwrap();
        publish_tree_pack(&f, &[leaf.clone(), state.clone()], PADDING);
        leaves.push(leaf.id());
        states.push(state.id());
    }
    let stack = initialize(&f.handles, 1, states[0]);
    let work = branch_id(1);
    fork(&f.handles, stack.id, work, 1);
    for state in &states[1..] {
        commit(&f.handles, work, *state);
    }

    layerfs_persistence::profile_statements();
    let mark = f.handles.history.mark().unwrap();
    let profile = layerfs_persistence::take_statement_profile();
    for id in leaves.iter().chain(&states).chain([&chunk]) {
        assert!(mark.objects().contains(*id), "object {id} is retained");
    }
    // A leaf is admitted while its file state's pack is still held, so each
    // pack is acquired once. Walking the leaves as a wave of their own, after
    // every file state, found the first packs evicted and read each again.
    let acquisitions: u64 = profile
        .iter()
        .filter(|(sql, _)| sql.starts_with("SELECT pack_id,domain,digest,length FROM pack WHERE"))
        .map(|(_, cost)| cost.calls)
        .sum();
    assert_eq!(acquisitions, PACKS as u64, "{profile:?}");
}
