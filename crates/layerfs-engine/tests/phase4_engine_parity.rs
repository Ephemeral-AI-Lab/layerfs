use layerfs_core::cas::InMemoryCas;
use layerfs_core::content::persistence as file_codec;
use layerfs_core::content::{ChunkReference, LogicalFile};
use layerfs_core::cow::persistence as dir_codec;
use layerfs_core::cow::{Metadata, RootHandle, TreeNode};
use layerfs_core::delta::codec as delta_codec;
use layerfs_core::object::{decode_object, encode_object, DirectoryEntry, Object};
use layerfs_core::{
    chunk_id, validate_identity, CanonicalName, CanonicalPath, CoreError, ObjectId,
};
use layerfs_engine::{DeltaRecord, Engine, RootRecord};

fn canonical_mapping(inner: Vec<u8>) -> Vec<u8> {
    encode_object(&Object::bytes(inner).expect("test bytes")).expect("canonical bytes")
}

fn frozen_hex(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0);
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = char::from(pair[0]).to_digit(16).expect("hex") as u8;
            let low = char::from(pair[1]).to_digit(16).expect("hex") as u8;
            (high << 4) | low
        })
        .collect()
}

#[test]
fn file_mapping_round_trips_at_leaf_boundaries() {
    for count in [0_usize, 1, 63, 64, 65, 101, 256, 257] {
        let references = (0..count)
            .map(|index| {
                let bytes = [u8::try_from(index & 0xff).expect("byte")];
                file_codec::FileReference {
                    raw_id: chunk_id(&bytes),
                    raw_length: 1,
                    object_id: ObjectId::for_bytes(&bytes),
                }
            })
            .collect::<Vec<_>>();
        let inner = file_codec::encode_file_leaf(&references).expect("leaf");
        let canonical = canonical_mapping(inner);
        let payload =
            file_codec::decode_mapping(&canonical, file_codec::FILE_LEAF_TAG).expect("decode");
        assert_eq!(
            file_codec::parse_file_leaf(payload).expect("parse"),
            references
        );
    }
}

#[test]
fn file_root_and_delta_preserve_ordered_identity() {
    let child = ObjectId::from_bytes(&[0x22; 32]).expect("child");
    let second_child = ObjectId::from_bytes(&[0x33; 32]).expect("second child");
    let root_inner = file_codec::encode_file_root(
        0,
        9,
        2,
        0,
        &[
            file_codec::FileChild {
                object_id: child,
                cumulative_end: 4,
            },
            file_codec::FileChild {
                object_id: second_child,
                cumulative_end: 9,
            },
        ],
    )
    .expect("root");
    let root = canonical_mapping(root_inner);
    assert_eq!(
        root,
        frozen_hex(
            "4c46534f0100000078000000744c4653344d415000000101000000000000000000000009000000000000000200000000020000000000000004222222222222222222222222222222222222222222222222222222222222222200000000000000093333333333333333333333333333333333333333333333333333333333333333"
        )
    );
    assert_eq!(
        ObjectId::for_bytes(&root),
        "fa1eb1f6a1e86cc0b589618b44897cceb42df11f172bdcf492cff05e60180d8a"
            .parse()
            .expect("frozen root id")
    );
    let payload =
        file_codec::decode_mapping(&root, file_codec::FILE_ROOT_TAG).expect("root decode");
    let (mode, total, references, level, children) =
        file_codec::parse_file_root(payload).expect("root parse");
    assert_eq!(mode, 0);
    assert_eq!((total, references, level), (9, 2, 0));
    assert_eq!(
        children,
        vec![
            file_codec::FileChild {
                cumulative_end: 4,
                object_id: child,
            },
            file_codec::FileChild {
                cumulative_end: 9,
                object_id: second_child,
            },
        ]
    );
    let delta_inner = delta_codec::encode_change(child, ObjectId::for_bytes(b"next"), 1, &[child])
        .expect("delta");
    let delta = canonical_mapping(delta_inner);
    assert_eq!(
        decode_object(&delta).expect("delta object").kind(),
        layerfs_core::ObjectKind::Bytes
    );
}

#[test]
fn malformed_mapping_and_identity_mismatch_are_rejected() {
    let mut inner = file_codec::encode_file_leaf(&[]).expect("leaf");
    inner[8..10].copy_from_slice(&2_u16.to_be_bytes());
    let canonical = canonical_mapping(inner);
    assert_eq!(
        file_codec::decode_mapping(&canonical, file_codec::FILE_LEAF_TAG),
        Err(CoreError::UnsupportedMappingVersion { version: 2 })
    );

    let valid = canonical_mapping(file_codec::encode_file_leaf(&[]).expect("leaf"));
    let mut trailing = valid.clone();
    trailing.push(0);
    assert!(matches!(
        decode_object(&trailing),
        Err(CoreError::TrailingBytes)
    ));
    assert_eq!(
        validate_identity(&valid, ObjectId::for_bytes(b"wrong")),
        Err(CoreError::IdentityMismatch)
    );
}

#[test]
fn mapping_roles_discriminators_and_zero_length_topology_are_exact() {
    let child = ObjectId::for_bytes(b"zero-length-child");
    let root = canonical_mapping(
        file_codec::encode_file_root(
            0,
            0,
            1,
            0,
            &[file_codec::FileChild {
                object_id: child,
                cumulative_end: 0,
            }],
        )
        .expect("all-zero nonempty root"),
    );
    let payload =
        file_codec::decode_mapping(&root, file_codec::FILE_ROOT_TAG).expect("file-root role");
    assert_eq!(
        file_codec::parse_file_root(payload).expect("all-zero nonempty root"),
        (
            0,
            0,
            1,
            0,
            vec![file_codec::FileChild {
                object_id: child,
                cumulative_end: 0,
            }],
        )
    );
    assert_eq!(
        file_codec::decode_mapping(&root, file_codec::FILE_LEAF_TAG),
        Err(CoreError::WrongLogicalRole)
    );

    let mut bad_parent = vec![2];
    bad_parent.extend_from_slice(child.as_bytes());
    bad_parent.extend_from_slice(&0_u32.to_be_bytes());
    bad_parent.extend_from_slice(&0_u32.to_be_bytes());
    assert_eq!(
        delta_codec::decode_transition(&bad_parent),
        Err(CoreError::InvalidMappingDiscriminator { value: 2 })
    );

    let mut bad_operation = Vec::new();
    bad_operation.extend_from_slice(&1_u32.to_be_bytes());
    bad_operation.push(0);
    bad_operation.extend_from_slice(&1_u32.to_be_bytes());
    bad_operation.push(b'a');
    bad_operation.extend_from_slice(child.as_bytes());
    assert_eq!(
        delta_codec::decode_delta_page(&bad_operation),
        Err(CoreError::InvalidMappingDiscriminator { value: 0 })
    );
}

#[test]
fn directory_pages_and_wrappers_are_canonical() {
    let child = ObjectId::for_bytes(b"child");
    let entries = vec![
        layerfs_core::DirectoryEntry::new(
            CanonicalName::new("00000001").expect("name"),
            layerfs_core::ObjectReference::new(layerfs_core::ObjectKind::Bytes, child),
        ),
        layerfs_core::DirectoryEntry::new(
            CanonicalName::new("00000002").expect("name"),
            layerfs_core::ObjectReference::new(layerfs_core::ObjectKind::Bytes, child),
        ),
    ];
    let page = dir_codec::encode_directory_page(&entries).expect("page");
    let page_id = ObjectId::for_bytes(&page);
    let index_inner = dir_codec::encode_directory_index(
        2,
        &[dir_codec::DirectoryPageRef {
            count: 2,
            first_name: b"00000001".to_vec(),
            object_id: page_id,
        }],
    )
    .expect("index");
    let index = canonical_mapping(index_inner);
    let index_payload =
        file_codec::decode_mapping(&index, file_codec::DIR_INDEX_TAG).expect("index decode");
    assert_eq!(
        dir_codec::parse_directory_index(index_payload)
            .expect("index parse")
            .len(),
        1
    );
    let metadata = canonical_mapping(dir_codec::encode_directory_metadata(0).expect("metadata"));
    let wrapper = dir_codec::encode_directory_wrapper(
        ObjectId::for_bytes(&metadata),
        ObjectId::for_bytes(&index),
    )
    .expect("wrapper");
    assert_eq!(
        decode_object(&wrapper).expect("wrapper decode").kind(),
        layerfs_core::ObjectKind::Directory
    );
}

#[test]
fn directory_index_rejects_more_than_the_direct_reference_limit() {
    let inner = dir_codec::encode_directory_index(
        100_001,
        &[dir_codec::DirectoryPageRef {
            count: 100_001,
            first_name: b"a".to_vec(),
            object_id: ObjectId::for_bytes(b"page"),
        }],
    )
    .expect("oversized index bytes");
    let canonical = canonical_mapping(inner);
    let payload = file_codec::decode_mapping(&canonical, file_codec::DIR_INDEX_TAG)
        .expect("directory-index role");
    assert_eq!(
        dir_codec::parse_directory_index(payload),
        Err(CoreError::ObjectLimitExceeded)
    );
}

#[test]
fn file_partition_rules_and_height_are_checked() {
    let profile = file_codec::FileMappingProfile::new(4, 3);
    let references = (0..4)
        .map(|index| {
            let bytes = [u8::try_from(index).expect("byte")];
            file_codec::FileReference {
                raw_id: chunk_id(&bytes),
                raw_length: 1,
                object_id: ObjectId::for_bytes(&bytes),
            }
        })
        .collect::<Vec<_>>();
    assert!(file_codec::validate_file_leaf(&references, profile, false).is_ok());
    assert_eq!(
        file_codec::validate_file_leaf(&references[..3], profile, false),
        Err(CoreError::NonCanonicalPagePartition)
    );
    assert_eq!(
        file_codec::validate_file_leaf(&references, profile, true).expect("final leaf"),
        (4, 4)
    );

    let children = (1_u8..=3)
        .map(|end| file_codec::FileChild {
            object_id: ObjectId::for_bytes(&[end]),
            cumulative_end: u64::from(end),
        })
        .collect::<Vec<_>>();
    assert!(file_codec::validate_file_children(&children, profile, false).is_ok());
    assert_eq!(
        file_codec::validate_file_children(&children[..2], profile, false),
        Err(CoreError::NonCanonicalPagePartition)
    );
    assert_eq!(
        file_codec::expected_file_level(12, profile).expect("height"),
        0
    );
    assert_eq!(
        file_codec::expected_file_level(13, profile).expect("height"),
        1
    );
    assert_eq!(
        file_codec::validate_file_root_summary(9, 3, 9, 2),
        Err(CoreError::LengthMismatch {
            expected: 3,
            actual: 2,
        })
    );
    assert_eq!(
        file_codec::validate_file_root_summary(8, 2, 9, 2),
        Err(CoreError::LengthMismatch {
            expected: 8,
            actual: 9,
        })
    );

    let empty = file_codec::encode_file_root(u32::MAX, 0, 0, 0, &[]).expect("empty root");
    let empty = canonical_mapping(empty);
    let empty = file_codec::decode_mapping(&empty, file_codec::FILE_ROOT_TAG).expect("decode");
    assert_eq!(
        file_codec::parse_file_root(empty).expect("parse empty root"),
        (u32::MAX, 0, 0, 0, Vec::new())
    );

    let mut children_without_level = Vec::new();
    children_without_level.extend_from_slice(&1_u32.to_be_bytes());
    children[0].encode(&mut children_without_level);
    assert_eq!(
        file_codec::parse_file_children(&children_without_level, false).expect("children"),
        (0, vec![children[0]])
    );
    assert_eq!(
        file_codec::parse_file_children(&children_without_level[..3], false),
        Err(CoreError::UnexpectedEof)
    );
    let mut trailing_children = children_without_level;
    trailing_children.push(0);
    assert_eq!(
        file_codec::parse_file_children(&trailing_children, false),
        Err(CoreError::TrailingBytes)
    );
    assert_eq!(
        file_codec::expected_file_level(2, file_codec::FileMappingProfile::new(1, 1)),
        Err(CoreError::NonCanonicalPagePartition)
    );
}

#[test]
fn sparse_nonfinal_branch_is_rejected_by_shared_validator() {
    let profile = file_codec::FileMappingProfile::new(4, 3);
    let children = (1_u8..=2)
        .map(|end| file_codec::FileChild {
            object_id: ObjectId::from_bytes(&[end; 32]).expect("child"),
            cumulative_end: u64::from(end),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        file_codec::validate_file_children(&children, profile, false),
        Err(CoreError::NonCanonicalPagePartition)
    );
}

#[test]
fn directory_partition_rejects_cross_page_duplicates() {
    let child = ObjectId::for_bytes(b"child");
    let entry = |name: &str| {
        DirectoryEntry::new(
            CanonicalName::new(name).expect("name"),
            layerfs_core::ObjectReference::new(layerfs_core::ObjectKind::Bytes, child),
        )
    };
    let first = vec![entry("0001"), entry("0002")];
    let second = vec![entry("0003"), entry("0004")];
    let first_ref = dir_codec::DirectoryPageRef {
        count: 2,
        first_name: first[0].name().as_bytes().to_vec(),
        object_id: child,
    };
    let second_ref = dir_codec::DirectoryPageRef {
        count: 2,
        first_name: second[0].name().as_bytes().to_vec(),
        object_id: child,
    };
    assert!(dir_codec::validate_directory_partition(
        4,
        &[(&first, &first_ref), (&second, &second_ref)],
        95,
    )
    .is_ok());

    let duplicate = vec![entry("0002"), entry("0004")];
    let duplicate_ref = dir_codec::DirectoryPageRef {
        count: 2,
        first_name: duplicate[0].name().as_bytes().to_vec(),
        object_id: child,
    };
    assert_eq!(
        dir_codec::validate_directory_partition(
            4,
            &[(&first, &first_ref), (&duplicate, &duplicate_ref)],
            95,
        ),
        Err(CoreError::NameCollision)
    );
}

#[test]
fn delta_decode_replays_exact_transition_and_rejects_trailing_bytes() {
    let parent = ObjectId::for_bytes(b"parent");
    let child = ObjectId::for_bytes(b"child");
    let operation = delta_codec::TransitionOperation::Replace {
        path: b"file".to_vec(),
        before: parent,
        after: child,
    };
    let page = canonical_mapping(
        delta_codec::encode_delta_page(std::slice::from_ref(&operation)).expect("delta page"),
    );
    let page_id = ObjectId::for_bytes(&page);
    let inner = delta_codec::encode_change(parent, child, 1, &[page_id]).expect("delta");
    let canonical = canonical_mapping(inner.clone());
    let decoded = delta_codec::decode_mapping_transition(&canonical).expect("decode");
    assert_eq!(decoded.parent, Some(parent));
    assert_eq!(decoded.child, child);
    assert_eq!(decoded.entry_count, 1);
    assert_eq!(decoded.pages, vec![page_id]);
    assert_eq!(
        delta_codec::decode_mapping_delta_page(&page).expect("page decode"),
        vec![operation]
    );
    let mut malformed = inner;
    malformed.push(0);
    let malformed = canonical_mapping(malformed);
    assert_eq!(
        delta_codec::decode_mapping_transition(&malformed),
        Err(CoreError::TrailingBytes)
    );
}

#[test]
fn phase3_delta_replay_applies_to_authenticated_root() {
    let parent = RootHandle::empty();
    let path = CanonicalPath::from_bytes(b"new-file").expect("path");
    let mutation = parent
        .add(path, TreeNode::empty_directory())
        .expect("mutation");
    assert_eq!(
        mutation.delta().apply(&parent).expect("replay"),
        *mutation.root()
    );
}

#[test]
fn durable_delta_page_replays_through_phase3_apply() {
    let parent = RootHandle::empty();
    let path = CanonicalPath::from_bytes(b"new-file").expect("path");
    let node = TreeNode::empty_directory();
    let mutation = parent.add(path.clone(), node.clone()).expect("mutation");
    let durable_id = |node: &TreeNode| Ok(ObjectId::for_bytes(node.identity().as_bytes()));
    let parent_id = durable_id(parent.node()).expect("parent id");
    let node_id = durable_id(&node).expect("node id");
    let child_id = durable_id(mutation.root().node()).expect("child id");
    let operation = delta_codec::TransitionOperation::Add {
        path: path.as_bytes().to_vec(),
        after: node_id,
    };
    let page = canonical_mapping(
        delta_codec::encode_delta_page(std::slice::from_ref(&operation)).expect("delta page"),
    );
    let transition =
        delta_codec::encode_change(parent_id, child_id, 1, &[ObjectId::for_bytes(&page)])
            .expect("delta index");
    let decoded = delta_codec::decode_mapping_transition(&canonical_mapping(transition))
        .expect("delta index decode");
    let operations = delta_codec::decode_mapping_delta_page(&page).expect("delta page decode");
    let delta = delta_codec::replay_durable_transition(
        &decoded,
        &operations,
        &parent,
        parent_id,
        |id| {
            if id == node_id {
                Ok(node.clone())
            } else {
                Err(CoreError::MissingObject)
            }
        },
        durable_id,
    )
    .expect("phase3 delta");
    assert_eq!(delta.apply(&parent).expect("apply"), *mutation.root());

    let tampered = delta_codec::TransitionOperation::Add {
        path: path.as_bytes().to_vec(),
        after: ObjectId::for_bytes(b"wrong-after"),
    };
    assert_eq!(
        delta_codec::replay_durable_transition(
            &decoded,
            &[tampered],
            &parent,
            parent_id,
            |_| Ok(node.clone()),
            durable_id,
        ),
        Err(CoreError::IdentityMismatch)
    );
}

#[test]
fn durable_delta_replays_add_remove_replace_and_metadata_in_sequence() {
    let node = |mode| {
        TreeNode::directory_with_metadata(std::iter::empty(), Metadata::new(mode))
            .expect("directory")
    };
    let removed = node(1);
    let replaced = node(2);
    let metadata_before = node(3);
    let added = node(4);
    let replacement = node(5);
    let parent = RootHandle::from_entries([
        (
            CanonicalName::new("metadata").expect("name"),
            metadata_before.clone(),
        ),
        (CanonicalName::new("remove").expect("name"), removed.clone()),
        (
            CanonicalName::new("replace").expect("name"),
            replaced.clone(),
        ),
    ])
    .expect("parent");
    let path = |value: &str| CanonicalPath::from_bytes(value.as_bytes()).expect("path");
    let after_add = parent
        .add(path("add"), added.clone())
        .expect("add")
        .into_root();
    let after_remove = after_add
        .remove(path("remove"))
        .expect("remove")
        .into_root();
    let after_replace = after_remove
        .replace(path("replace"), replacement.clone())
        .expect("replace")
        .into_root();
    let child = after_replace
        .set_metadata(path("metadata"), Metadata::new(6))
        .expect("metadata")
        .into_root();
    let durable_id = |node: &TreeNode| Ok(ObjectId::for_bytes(node.identity().as_bytes()));
    let metadata_after = metadata_before.with_metadata(Metadata::new(6));
    let operations = vec![
        delta_codec::TransitionOperation::Add {
            path: b"add".to_vec(),
            after: durable_id(&added).expect("added id"),
        },
        delta_codec::TransitionOperation::Remove {
            path: b"remove".to_vec(),
            before: durable_id(&removed).expect("removed id"),
        },
        delta_codec::TransitionOperation::Replace {
            path: b"replace".to_vec(),
            before: durable_id(&replaced).expect("replaced id"),
            after: durable_id(&replacement).expect("replacement id"),
        },
        delta_codec::TransitionOperation::Metadata {
            path: b"metadata".to_vec(),
            before: durable_id(&metadata_before).expect("metadata before id"),
            before_mode: 3,
            after: durable_id(&metadata_after).expect("metadata after id"),
            after_mode: 6,
        },
    ];
    let page = canonical_mapping(delta_codec::encode_delta_page(&operations).expect("page"));
    assert_eq!(
        delta_codec::decode_mapping_delta_page(&page).expect("decode page"),
        operations
    );
    let parent_id = durable_id(parent.node()).expect("parent id");
    let transition = delta_codec::DecodedTransition {
        parent: Some(parent_id),
        child: durable_id(child.node()).expect("child id"),
        entry_count: u32::try_from(operations.len()).expect("entry count"),
        pages: vec![ObjectId::for_bytes(&page)],
    };
    let loaded = [added.clone(), replacement.clone()];
    let replay = delta_codec::replay_durable_transition(
        &transition,
        &operations,
        &parent,
        parent_id,
        |id| {
            loaded
                .iter()
                .find(|node| durable_id(node) == Ok(id))
                .cloned()
                .ok_or(CoreError::MissingObject)
        },
        durable_id,
    )
    .expect("replay");
    assert_eq!(replay.apply(&parent).expect("apply"), child);

    let mut conflicting = operations.clone();
    if let delta_codec::TransitionOperation::Remove { before, .. } = &mut conflicting[1] {
        *before = ObjectId::for_bytes(b"wrong-before");
    }
    assert_eq!(
        delta_codec::replay_durable_transition(
            &transition,
            &conflicting,
            &parent,
            parent_id,
            |id| {
                loaded
                    .iter()
                    .find(|node| durable_id(node) == Ok(id))
                    .cloned()
                    .ok_or(CoreError::MissingObject)
            },
            durable_id,
        ),
        Err(CoreError::DeltaConflict)
    );
    assert_eq!(
        delta_codec::replay_durable_transition(
            &transition,
            &operations,
            &parent,
            ObjectId::for_bytes(b"wrong-parent"),
            |_| Err(CoreError::MissingObject),
            durable_id,
        ),
        Err(CoreError::DeltaParentMismatch {
            expected: ObjectId::for_bytes(b"wrong-parent"),
            actual: parent_id,
        })
    );
}

#[test]
fn memory_and_sqlite_execute_the_same_authenticated_range() {
    let raw = b"memory-and-sqlite-parity";
    let mut cas = InMemoryCas::new();
    let (chunk, _) = cas.put_chunk(raw).expect("memory put");
    let logical =
        LogicalFile::from_chunks(&cas, vec![ChunkReference::new(chunk, raw.len() as u64)])
            .expect("memory file");
    let memory_range = logical.read_range(&cas, 3..17).expect("memory range");

    let path = std::env::temp_dir().join(format!(
        "layerfs-parity-{}.sqlite",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let canonical = encode_object(&Object::bytes(raw.to_vec()).expect("object")).expect("encode");
    let object_id = ObjectId::for_bytes(&canonical);
    let directory = encode_object(&Object::directory(Vec::new()).expect("directory"))
        .expect("directory encode");
    let directory_id = ObjectId::for_bytes(&directory);
    {
        let engine = Engine::open(&path).expect("engine open");
        assert!(matches!(
            engine.put_object_if_absent(object_id, &canonical),
            Ok(layerfs_engine::PutOutcome::Created)
        ));
        assert!(matches!(
            engine.put_object_if_absent(directory_id, &directory),
            Ok(layerfs_engine::PutOutcome::Created)
        ));
        let mut capture = engine.begin_capture(None).expect("capture");
        let delta = DeltaRecord::new(None, directory_id, Vec::new());
        capture.write_delta(&delta).expect("delta");
        capture
            .commit_root(RootRecord {
                id: directory_id,
                directory_object: directory_id,
                parent: None,
            })
            .expect("commit");
        let decoded = decode_object(
            &engine
                .read_object_range(object_id, 0..canonical.len() as u64)
                .expect("sqlite read"),
        )
        .expect("sqlite decode");
        let Object::Bytes(sqlite_bytes) = decoded else {
            panic!("expected bytes object")
        };
        assert_eq!(&sqlite_bytes[3..17], memory_range.bytes());
        assert_eq!(
            engine.load_visible_root().expect("visible"),
            Some(directory_id)
        );
    }
    let _ = std::fs::remove_file(path);
}
