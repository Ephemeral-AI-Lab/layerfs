//! Contextual direct-child acceptance through the public Content boundary.

#[path = "support/context.rs"]
mod support;

use layerfs_content::file::mapping::{
    encode_chunk_object, encode_file_state, encode_node, profile_id as mapping_profile,
    ChildDescriptor, ExtentNode, ExtentSlice, FileState,
};
use layerfs_content::filesystem::attributes::codec::{row_bytes, LEAF_ROW_OVERHEAD};
use layerfs_content::filesystem::attributes::{
    encode_attribute_page, AttributeEntry, AttributeKey, AttributePage,
};
use layerfs_content::filesystem::directory::{encode_directory_page, DirectoryPage};
use layerfs_content::filesystem::inode::{encode_inode_page, InodePage};
use layerfs_content::filesystem::{profile_id, FilesystemRoot, PathName, SymlinkTarget};
use layerfs_content::object::{InodeKind, InodeValue};
use layerfs_content::{
    encode_whole_file_payload, ConstructionPolicy, ContentError, FinalizedObject, ObjectId,
    ObjectRole,
};
use layerfs_telemetry::timer::Timing;
use support::{metadata, object, scope, validate, Store};

fn inode(kind: InodeKind, content: ObjectId, metadata: ObjectId, count: u64) -> InodeValue {
    InodeValue {
        kind,
        namespace_ref_count: count,
        content_root: content,
        metadata_root: metadata,
    }
}

fn inode_leaf(entries: Vec<(u64, InodeValue)>) -> FinalizedObject {
    object(
        ObjectRole::InodeLeaf,
        encode_inode_page(&InodePage::Leaf { entries }).unwrap(),
    )
}

fn name(index: u64) -> PathName {
    PathName::new(&format!("{index:04}-{}", "x".repeat(196))).unwrap()
}

fn directory_child(store: &mut Store, start: u64) -> (PathName, FinalizedObject, u64) {
    let entries = (start..start + 16)
        .map(|index| (name(index), 2))
        .collect::<Vec<_>>();
    let bytes = entries
        .iter()
        .map(|(key, _)| key.as_bytes().len() as u64 + 10)
        .sum();
    let maximum = entries.last().unwrap().0.clone();
    let object = store.put(
        ObjectRole::DirectoryLeaf,
        encode_directory_page(&DirectoryPage::Leaf { entries }).unwrap(),
    );
    (maximum, object, bytes)
}

fn attribute_child(
    store: &mut Store,
    start: u64,
    value: ObjectId,
) -> (AttributeKey, FinalizedObject, u64) {
    let entries = (start..start + 16)
        .map(|index| AttributeEntry {
            key: AttributeKey::new(
                "user".into(),
                format!("{index:04}-{}", "x".repeat(196)).into_bytes(),
            )
            .unwrap(),
            value_root: value,
        })
        .collect::<Vec<_>>();
    let bytes = entries
        .iter()
        .map(|entry| row_bytes(&entry.key, LEAF_ROW_OVERHEAD) as u64)
        .sum();
    let maximum = entries.last().unwrap().key.clone();
    let object = store.put(
        ObjectRole::AttributeLeaf,
        encode_attribute_page(&AttributePage::Leaf {
            subtree_bytes: bytes,
            entries,
        })
        .unwrap(),
    );
    (maximum, object, bytes)
}

fn filled_extent(store: &mut Store, chunk: ObjectId) -> FinalizedObject {
    store.put(
        ObjectRole::ExtentLeaf,
        encode_node(
            &ExtentNode::Leaf {
                subtree_logical_bytes: 64,
                extents: (0..64)
                    .map(|index| ExtentSlice::new(chunk, (index % 2) * 2, 1).unwrap())
                    .collect(),
            },
            false,
        )
        .unwrap(),
    )
}

#[test]
fn all_thirteen_roles_validate_with_direct_children_and_scoped_one_id_demands() {
    let mut store = Store::default();
    let whole = store.put(
        ObjectRole::WholeFile,
        encode_whole_file_payload(b"file").unwrap(),
    );
    let chunk = store.put(ObjectRole::Chunk, encode_chunk_object(b"chunk").unwrap());
    let extent = filled_extent(&mut store, chunk.id());
    let branch = store.put(
        ObjectRole::ExtentBranch,
        encode_node(
            &ExtentNode::Branch {
                level: 1,
                subtree_logical_bytes: 128,
                subtree_extent_count: 128,
                children: vec![
                    ChildDescriptor {
                        cumulative_logical_end: 64,
                        cumulative_extent_end: 64,
                        child_object_id: extent.id(),
                    },
                    ChildDescriptor {
                        cumulative_logical_end: 128,
                        cumulative_extent_end: 128,
                        child_object_id: extent.id(),
                    },
                ],
            },
            true,
        )
        .unwrap(),
    );
    store.put(
        ObjectRole::FileState,
        encode_file_state(FileState {
            logical_len: 128,
            extent_count: 128,
            tree_level: 1,
            profile_id: mapping_profile(),
            mapping_root: branch.id(),
        })
        .unwrap(),
    );
    store.put(
        ObjectRole::Symlink,
        SymlinkTarget::new(b"../target".to_vec())
            .unwrap()
            .encode()
            .unwrap(),
    );
    let meta = metadata(&mut store, 0o755);
    let directory = store.put(
        ObjectRole::DirectoryLeaf,
        encode_directory_page(&DirectoryPage::Leaf { entries: vec![] }).unwrap(),
    );
    let root_inode = store.put(
        ObjectRole::InodeLeaf,
        encode_inode_page(&InodePage::Leaf {
            entries: vec![(1, inode(InodeKind::Directory, directory.id(), meta.id(), 0))],
        })
        .unwrap(),
    );
    let file_value = inode(InodeKind::RegularFile, whole.id(), meta.id(), 1);
    let left = store.put(
        ObjectRole::InodeLeaf,
        encode_inode_page(&InodePage::Leaf {
            entries: (10..60).map(|serial| (serial, file_value)).collect(),
        })
        .unwrap(),
    );
    let right = store.put(
        ObjectRole::InodeLeaf,
        encode_inode_page(&InodePage::Leaf {
            entries: (100..150).map(|serial| (serial, file_value)).collect(),
        })
        .unwrap(),
    );
    store.put(
        ObjectRole::InodeBranch,
        encode_inode_page(&InodePage::Branch {
            level: 1,
            subtree_count: 100,
            children: vec![(59, left.id()), (149, right.id())],
        })
        .unwrap(),
    );
    store.put(
        ObjectRole::FilesystemRoot,
        FilesystemRoot::new(profile_id(), scope(), 1, root_inode.id())
            .unwrap()
            .encode()
            .unwrap(),
    );
    let (left_key, left, left_bytes) = directory_child(&mut store, 10);
    let (right_key, right, right_bytes) = directory_child(&mut store, 100);
    store.put(
        ObjectRole::DirectoryBranch,
        encode_directory_page(&DirectoryPage::Branch {
            level: 1,
            subtree_count: 32,
            subtree_bytes: left_bytes + right_bytes,
            children: vec![(left_key, left.id()), (right_key, right.id())],
        })
        .unwrap(),
    );
    let value = support::extent_value(&mut store, b"attribute");
    let (left_key, left, left_bytes) = attribute_child(&mut store, 10, value.id());
    let (right_key, right, right_bytes) = attribute_child(&mut store, 100, value.id());
    store.put(
        ObjectRole::AttributeBranch,
        encode_attribute_page(&AttributePage::Branch {
            level: 1,
            subtree_count: 32,
            subtree_bytes: left_bytes + right_bytes,
            children: vec![(left_key, left.id()), (right_key, right.id())],
        })
        .unwrap(),
    );
    let mut roles = std::collections::BTreeSet::new();
    for object in store.objects() {
        validate(object, &store).unwrap_or_else(|error| panic!("{:?}: {error}", object.role()));
        roles.insert(object.role().code());
    }
    assert_eq!(roles.len(), 13);
    assert!(store.demands.borrow().iter().all(|ids| ids.len() == 1));
    assert_eq!(store.scoped.get(), store.demands.borrow().len());
}

#[test]
fn local_reference_and_identity_context_refusals_precede_all_demands() {
    let mut store = Store::default();
    let state = support::extent_value(&mut store, b"bytes");
    store.clear_demands();
    assert_eq!(
        validate(&state.clone().with_references(vec![]), &store),
        Err(ContentError::InvalidRecord("object reference list"))
    );
    let bytes = encode_directory_page(&DirectoryPage::Leaf {
        entries: vec![(PathName::new("root").unwrap(), 1)],
    })
    .unwrap();
    assert_eq!(
        validate(&object(ObjectRole::DirectoryLeaf, bytes), &store),
        Err(ContentError::InvalidRecord("root directory binding"))
    );
    let bytes = encode_directory_page(&DirectoryPage::Leaf {
        entries: vec![(PathName::new("huge").unwrap(), u64::MAX)],
    })
    .unwrap();
    assert_eq!(
        validate(&object(ObjectRole::DirectoryLeaf, bytes), &store),
        Err(ContentError::InvalidRecord("inode serial"))
    );
    let bytes = encode_inode_page(&InodePage::Leaf {
        entries: vec![(
            u64::MAX,
            inode(InodeKind::RegularFile, state.id(), state.id(), 1),
        )],
    })
    .unwrap();
    assert_eq!(
        validate(&object(ObjectRole::InodeLeaf, bytes), &store),
        Err(ContentError::InvalidRecord("inode serial"))
    );
    let root = object(
        ObjectRole::FilesystemRoot,
        FilesystemRoot::new(profile_id(), scope(), 2, state.id())
            .unwrap()
            .encode()
            .unwrap(),
    );
    assert!(matches!(
        validate(&root, &store),
        Err(ContentError::ScopeMismatch { .. })
    ));
    assert!(store.demands.borrow().is_empty());
}

#[test]
fn extent_payload_kind_slice_and_file_state_actual_summaries_are_checked() {
    let mut store = Store::default();
    let chunk = store.put(ObjectRole::Chunk, encode_chunk_object(b"ab").unwrap());
    let make_leaf = |id, length| {
        object(
            ObjectRole::ExtentLeaf,
            encode_node(
                &ExtentNode::Leaf {
                    subtree_logical_bytes: u64::from(length),
                    extents: vec![ExtentSlice::new(id, 0, length).unwrap()],
                },
                true,
            )
            .unwrap(),
        )
    };
    assert_eq!(
        validate(&make_leaf(chunk.id(), 3), &store),
        Err(ContentError::InvalidRecord("extent payload slice"))
    );
    let whole = store.put(
        ObjectRole::WholeFile,
        encode_whole_file_payload(b"abc").unwrap(),
    );
    assert_eq!(
        validate(&make_leaf(whole.id(), 3), &store),
        Err(ContentError::WrongLogicalRole)
    );
    let leaf = store.put(
        ObjectRole::ExtentLeaf,
        make_leaf(chunk.id(), 2).canonical().to_vec(),
    );
    let state = object(
        ObjectRole::FileState,
        encode_file_state(FileState {
            logical_len: 3,
            extent_count: 1,
            tree_level: 0,
            profile_id: mapping_profile(),
            mapping_root: leaf.id(),
        })
        .unwrap(),
    );
    assert_eq!(
        validate(&state, &store),
        Err(ContentError::InvalidRecord("file state root summary"))
    );
}

#[test]
fn extent_branches_check_nonroot_fill_level_and_exact_cumulative_summaries() {
    let mut store = Store::default();
    let chunk = store.put(ObjectRole::Chunk, encode_chunk_object(b"chunk").unwrap());
    let short = store.put(
        ObjectRole::ExtentLeaf,
        encode_node(
            &ExtentNode::Leaf {
                subtree_logical_bytes: 1,
                extents: vec![ExtentSlice::new(chunk.id(), 0, 1).unwrap()],
            },
            true,
        )
        .unwrap(),
    );
    let branch = |child, level, width, count| {
        object(
            ObjectRole::ExtentBranch,
            encode_node(
                &ExtentNode::Branch {
                    level,
                    subtree_logical_bytes: width * 2,
                    subtree_extent_count: count * 2,
                    children: vec![
                        ChildDescriptor {
                            cumulative_logical_end: width,
                            cumulative_extent_end: count,
                            child_object_id: child,
                        },
                        ChildDescriptor {
                            cumulative_logical_end: width * 2,
                            cumulative_extent_end: count * 2,
                            child_object_id: child,
                        },
                    ],
                },
                true,
            )
            .unwrap(),
        )
    };
    assert_eq!(
        validate(&branch(short.id(), 1, 1, 1), &store),
        Err(ContentError::NonCanonicalPagePartition)
    );
    let filled = filled_extent(&mut store, chunk.id());
    assert_eq!(
        validate(&branch(filled.id(), 2, 64, 64), &store),
        Err(ContentError::InvalidRecord("extent child level"))
    );
    assert_eq!(
        validate(&branch(filled.id(), 1, 65, 64), &store),
        Err(ContentError::InvalidRecord("extent child summary"))
    );
    assert_eq!(
        validate(&branch(filled.id(), 1, 64, 65), &store),
        Err(ContentError::InvalidRecord("extent child summary"))
    );
}

#[test]
fn inode_placement_content_kind_and_portable_metadata_are_contextual() {
    let mut store = Store::default();
    let regular = store.put(
        ObjectRole::WholeFile,
        encode_whole_file_payload(b"file").unwrap(),
    );
    let meta = metadata(&mut store, 0o755);
    let misplaced = inode_leaf(vec![(
        2,
        inode(InodeKind::Directory, regular.id(), meta.id(), 0),
    )]);
    store.clear_demands();
    assert_eq!(
        validate(&misplaced, &store),
        Err(ContentError::InvalidRecord("inode value invariant"))
    );
    assert!(store.demands.borrow().is_empty());
    let wrong_root = inode_leaf(vec![(
        1,
        inode(InodeKind::RegularFile, regular.id(), meta.id(), 1),
    )]);
    assert_eq!(
        validate(&wrong_root, &store),
        Err(ContentError::InvalidRecord("inode value invariant"))
    );
    let wrong_content = inode_leaf(vec![(
        2,
        inode(InodeKind::Directory, regular.id(), meta.id(), 1),
    )]);
    assert!(validate(&wrong_content, &store).is_err());
    let target = store.put(
        ObjectRole::Symlink,
        SymlinkTarget::new(b"target".to_vec())
            .unwrap()
            .encode()
            .unwrap(),
    );
    let wrong_mode = inode_leaf(vec![(
        2,
        inode(InodeKind::Symlink, target.id(), meta.id(), 1),
    )]);
    assert_eq!(
        validate(&wrong_mode, &store),
        Err(ContentError::InvalidRecord("portable metadata"))
    );
    let missing_meta = store.put(
        ObjectRole::AttributeLeaf,
        encode_attribute_page(&AttributePage::Leaf {
            subtree_bytes: 0,
            entries: vec![],
        })
        .unwrap(),
    );
    let missing = inode_leaf(vec![(
        2,
        inode(InodeKind::RegularFile, regular.id(), missing_meta.id(), 1),
    )]);
    assert_eq!(
        validate(&missing, &store),
        Err(ContentError::InvalidRecord("mode missing"))
    );
}

#[test]
fn regular_roots_obey_whole_file_capacity_and_retain_small_file_states() {
    let mut store = Store::default();
    let state = support::extent_value(&mut store, b"small retained state");
    let meta = metadata(&mut store, 0o644);
    let leaf = inode_leaf(vec![(
        2,
        inode(InodeKind::RegularFile, state.id(), meta.id(), 1),
    )]);
    validate(&leaf, &store).unwrap();
    let large = store.put(
        ObjectRole::WholeFile,
        encode_whole_file_payload(&vec![7; 131_072]).unwrap(),
    );
    let leaf = inode_leaf(vec![(
        2,
        inode(InodeKind::RegularFile, large.id(), meta.id(), 1),
    )]);
    let result = Timing::disabled("small-policy", |timing| {
        leaf.validate_context(
            &store,
            ConstructionPolicy::default(),
            scope(),
            1,
            timing.child("object"),
        )
    })
    .0;
    assert!(matches!(
        result,
        Err(ContentError::BoundedCapacityExceeded {
            what: "context.whole_file",
            ..
        })
    ));
}

#[test]
fn filesystem_root_requires_the_expected_zero_count_directory_record() {
    let mut store = Store::default();
    let regular = store.put(
        ObjectRole::WholeFile,
        encode_whole_file_payload(b"file").unwrap(),
    );
    let meta = metadata(&mut store, 0o644);
    let wrong = store.put(
        ObjectRole::InodeLeaf,
        encode_inode_page(&InodePage::Leaf {
            entries: vec![(1, inode(InodeKind::RegularFile, regular.id(), meta.id(), 1))],
        })
        .unwrap(),
    );
    let root = |table| {
        object(
            ObjectRole::FilesystemRoot,
            FilesystemRoot::new(profile_id(), scope(), 1, table)
                .unwrap()
                .encode()
                .unwrap(),
        )
    };
    assert_eq!(
        validate(&root(wrong.id()), &store),
        Err(ContentError::InvalidRecord("inode value invariant"))
    );
    let missing = store.put(
        ObjectRole::InodeLeaf,
        encode_inode_page(&InodePage::Leaf {
            entries: vec![(2, inode(InodeKind::RegularFile, regular.id(), meta.id(), 1))],
        })
        .unwrap(),
    );
    assert_eq!(
        validate(&root(missing.id()), &store),
        Err(ContentError::InvalidRecord("filesystem root inode"))
    );
}

#[test]
fn failures_and_cardinality_end_the_original_demand_without_retry() {
    let mut store = Store::default();
    let state = support::extent_value(&mut store, b"bytes");
    store.clear_demands();
    let original = ContentError::ProviderFailure {
        what: "original same-Save failure",
    };
    store.failure = Some(original.clone());
    assert_eq!(validate(&state, &store), Err(original));
    assert_eq!(store.demands.borrow().len(), 1);
    store.failure = None;
    store.returned = Some(0);
    store.clear_demands();
    assert_eq!(
        validate(&state, &store),
        Err(ContentError::BatchCardinality {
            requested: 1,
            returned: 0
        })
    );
    assert_eq!(store.demands.borrow().len(), 1);
    store.returned = Some(2);
    store.clear_demands();
    assert_eq!(
        validate(&state, &store),
        Err(ContentError::BatchCardinality {
            requested: 1,
            returned: 2
        })
    );
    assert_eq!(store.demands.borrow().len(), 1);
}

#[test]
fn a_hundred_large_inode_roots_do_not_form_one_oversized_provider_batch() {
    let mut store = Store::default();
    let file = store.put(
        ObjectRole::WholeFile,
        encode_whole_file_payload(&vec![7; 1_048_575]).unwrap(),
    );
    let meta = metadata(&mut store, 0o644);
    let leaf = inode_leaf(
        (2..102)
            .map(|serial| {
                (
                    serial,
                    inode(InodeKind::RegularFile, file.id(), meta.id(), 1),
                )
            })
            .collect(),
    );
    store.clear_demands();
    validate(&leaf, &store).unwrap();
    assert!(store.demands.borrow().iter().all(|ids| ids.len() == 1));
    assert_eq!(
        store
            .demands
            .borrow()
            .iter()
            .filter(|ids| ids[0] == file.id())
            .count(),
        100
    );
}

#[test]
fn inode_branches_check_fill_level_maximum_count_and_leaf_ranges() {
    let mut store = Store::default();
    let id = ObjectId::for_bytes(b"not demanded by a direct branch check");
    let value = inode(InodeKind::RegularFile, id, id, 1);
    let left = store.put(
        ObjectRole::InodeLeaf,
        encode_inode_page(&InodePage::Leaf {
            entries: (2..52).map(|serial| (serial, value)).collect(),
        })
        .unwrap(),
    );
    let right = store.put(
        ObjectRole::InodeLeaf,
        encode_inode_page(&InodePage::Leaf {
            entries: (100..150).map(|serial| (serial, value)).collect(),
        })
        .unwrap(),
    );
    let branch = |level, count, left_max, right_max, left, right| {
        object(
            ObjectRole::InodeBranch,
            encode_inode_page(&InodePage::Branch {
                level,
                subtree_count: count,
                children: vec![(left_max, left), (right_max, right)],
            })
            .unwrap(),
        )
    };
    validate(&branch(1, 100, 51, 149, left.id(), right.id()), &store).unwrap();
    assert_eq!(
        validate(&branch(2, 100, 51, 149, left.id(), right.id()), &store),
        Err(ContentError::InvalidRecord("inode child summary"))
    );
    assert_eq!(
        validate(&branch(1, 100, 52, 149, left.id(), right.id()), &store),
        Err(ContentError::InvalidRecord("inode child summary"))
    );
    assert_eq!(
        validate(&branch(1, 99, 51, 149, left.id(), right.id()), &store),
        Err(ContentError::InvalidRecord("inode subtree summary"))
    );
    let short = store.put(
        ObjectRole::InodeLeaf,
        encode_inode_page(&InodePage::Leaf {
            entries: vec![(2, value)],
        })
        .unwrap(),
    );
    assert_eq!(
        validate(&branch(1, 51, 2, 149, short.id(), right.id()), &store),
        Err(ContentError::NonCanonicalPagePartition)
    );
    let overlap = store.put(
        ObjectRole::InodeLeaf,
        encode_inode_page(&InodePage::Leaf {
            entries: (50..100).map(|serial| (serial, value)).collect(),
        })
        .unwrap(),
    );
    assert_eq!(
        validate(&branch(1, 100, 51, 99, left.id(), overlap.id()), &store),
        Err(ContentError::NonCanonicalOrdering)
    );
}

#[test]
fn directory_branches_check_fill_level_maximum_and_exact_counts_and_bytes() {
    let mut store = Store::default();
    let (left_key, left, left_bytes) = directory_child(&mut store, 10);
    let (right_key, right, right_bytes) = directory_child(&mut store, 100);
    let branch = |level, count, bytes, left_key: PathName, right_key: PathName, left, right| {
        object(
            ObjectRole::DirectoryBranch,
            encode_directory_page(&DirectoryPage::Branch {
                level,
                subtree_count: count,
                subtree_bytes: bytes,
                children: vec![(left_key, left), (right_key, right)],
            })
            .unwrap(),
        )
    };
    let bytes = left_bytes + right_bytes;
    validate(
        &branch(
            1,
            32,
            bytes,
            left_key.clone(),
            right_key.clone(),
            left.id(),
            right.id(),
        ),
        &store,
    )
    .unwrap();
    for (level, count, bytes, expected) in [
        (2, 32, bytes, "directory child summary"),
        (1, 31, bytes, "directory subtree summary"),
        (1, 32, bytes + 1, "directory subtree summary"),
    ] {
        assert_eq!(
            validate(
                &branch(
                    level,
                    count,
                    bytes,
                    left_key.clone(),
                    right_key.clone(),
                    left.id(),
                    right.id()
                ),
                &store
            ),
            Err(ContentError::InvalidRecord(expected))
        );
    }
    assert_eq!(
        validate(
            &branch(
                1,
                32,
                bytes,
                name(26),
                right_key.clone(),
                left.id(),
                right.id()
            ),
            &store
        ),
        Err(ContentError::InvalidRecord("directory child summary"))
    );
    let short = store.put(
        ObjectRole::DirectoryLeaf,
        encode_directory_page(&DirectoryPage::Leaf {
            entries: vec![(PathName::new("0000").unwrap(), 2)],
        })
        .unwrap(),
    );
    assert_eq!(
        validate(
            &branch(
                1,
                17,
                right_bytes + 14,
                PathName::new("0000").unwrap(),
                right_key,
                short.id(),
                right.id()
            ),
            &store
        ),
        Err(ContentError::NonCanonicalPagePartition)
    );
    let (overlap_key, overlap, overlap_bytes) = directory_child(&mut store, 20);
    assert_eq!(
        validate(
            &branch(
                1,
                32,
                left_bytes + overlap_bytes,
                left_key,
                overlap_key,
                left.id(),
                overlap.id()
            ),
            &store
        ),
        Err(ContentError::NonCanonicalOrdering)
    );
}

#[test]
fn attribute_branches_check_fill_level_maximum_and_exact_counts_and_bytes() {
    let mut store = Store::default();
    let value = support::extent_value(&mut store, b"attribute");
    let (left_key, left, left_bytes) = attribute_child(&mut store, 10, value.id());
    let (right_key, right, right_bytes) = attribute_child(&mut store, 100, value.id());
    let branch =
        |level, count, bytes, left_key: AttributeKey, right_key: AttributeKey, left, right| {
            object(
                ObjectRole::AttributeBranch,
                encode_attribute_page(&AttributePage::Branch {
                    level,
                    subtree_count: count,
                    subtree_bytes: bytes,
                    children: vec![(left_key, left), (right_key, right)],
                })
                .unwrap(),
            )
        };
    let bytes = left_bytes + right_bytes;
    validate(
        &branch(
            1,
            32,
            bytes,
            left_key.clone(),
            right_key.clone(),
            left.id(),
            right.id(),
        ),
        &store,
    )
    .unwrap();
    for (level, count, bytes, expected) in [
        (2, 32, bytes, "attribute child summary"),
        (1, 31, bytes, "attribute subtree summary"),
        (1, 32, bytes + 1, "attribute subtree summary"),
    ] {
        assert_eq!(
            validate(
                &branch(
                    level,
                    count,
                    bytes,
                    left_key.clone(),
                    right_key.clone(),
                    left.id(),
                    right.id()
                ),
                &store
            ),
            Err(ContentError::InvalidRecord(expected))
        );
    }
    let wrong = AttributeKey::new("user".into(), b"zzzz".to_vec()).unwrap();
    assert_eq!(
        validate(
            &branch(1, 32, bytes, left_key.clone(), wrong, left.id(), right.id()),
            &store
        ),
        Err(ContentError::InvalidRecord("attribute child summary"))
    );
    let key = AttributeKey::new("user".into(), b"0000".to_vec()).unwrap();
    let short_bytes = row_bytes(&key, LEAF_ROW_OVERHEAD) as u64;
    let short = store.put(
        ObjectRole::AttributeLeaf,
        encode_attribute_page(&AttributePage::Leaf {
            subtree_bytes: short_bytes,
            entries: vec![AttributeEntry {
                key: key.clone(),
                value_root: value.id(),
            }],
        })
        .unwrap(),
    );
    assert_eq!(
        validate(
            &branch(
                1,
                17,
                short_bytes + right_bytes,
                key,
                right_key,
                short.id(),
                right.id()
            ),
            &store
        ),
        Err(ContentError::NonCanonicalPagePartition)
    );
    let (overlap_key, overlap, overlap_bytes) = attribute_child(&mut store, 20, value.id());
    assert_eq!(
        validate(
            &branch(
                1,
                32,
                left_bytes + overlap_bytes,
                left_key,
                overlap_key,
                left.id(),
                overlap.id()
            ),
            &store
        ),
        Err(ContentError::NonCanonicalOrdering)
    );
}

#[test]
fn attribute_values_require_extent_roots_and_refuse_oversized_lengths_before_mapping_demand() {
    let mut store = Store::default();
    let whole = store.put(
        ObjectRole::WholeFile,
        encode_whole_file_payload(b"attribute").unwrap(),
    );
    let key = AttributeKey::new("user".into(), b"value".to_vec()).unwrap();
    let leaf = |value_root| {
        object(
            ObjectRole::AttributeLeaf,
            encode_attribute_page(&AttributePage::Leaf {
                subtree_bytes: row_bytes(&key, LEAF_ROW_OVERHEAD) as u64,
                entries: vec![AttributeEntry {
                    key: key.clone(),
                    value_root,
                }],
            })
            .unwrap(),
        )
    };
    assert!(validate(&leaf(whole.id()), &store).is_err());
    let state = store.put(
        ObjectRole::FileState,
        encode_file_state(FileState {
            logical_len: 32_769,
            extent_count: 1,
            tree_level: 0,
            profile_id: mapping_profile(),
            mapping_root: ObjectId::for_bytes(b"never demanded"),
        })
        .unwrap(),
    );
    store.clear_demands();
    assert_eq!(
        validate(&leaf(state.id()), &store),
        Err(ContentError::InvalidRecord("attribute value length"))
    );
    assert_eq!(store.demands.borrow().as_slice(), &[vec![state.id()]]);
}
