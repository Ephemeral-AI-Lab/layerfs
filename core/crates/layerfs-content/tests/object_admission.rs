//! Runtime-facing semantic admission, through public canonical codecs.

use layerfs_content::file::mapping::{
    encode_chunk_object, encode_file_state, encode_node, profile_id as mapping_profile,
    ChildDescriptor, ExtentNode, ExtentSlice, FileState,
};
use layerfs_content::filesystem::attributes::codec::row_bytes;
use layerfs_content::filesystem::attributes::{
    encode_attribute_page, AttributeEntry, AttributeKey, AttributePage,
};
use layerfs_content::filesystem::directory::{encode_directory_page, DirectoryPage};
use layerfs_content::filesystem::inode::{encode_inode_page, InodePage};
use layerfs_content::filesystem::{
    profile_id, scope_for_seed, FilesystemRoot, PathName, SymlinkTarget,
};
use layerfs_content::object::{encode_bytes_object, InodeKind, InodeValue};
use layerfs_content::{
    encode_whole_file_payload, ConstructionPolicy, ContentError, ContentResult, FinalizedObject,
    ObjectId, ObjectRole,
};
use layerfs_telemetry::timer::Timing;

type Case = (ObjectRole, Vec<u8>, Vec<ObjectId>);

fn id(byte: u8) -> ObjectId {
    ObjectId::from_bytes(&[byte; 32]).unwrap()
}

fn cases() -> Vec<Case> {
    let a = id(1);
    let b = id(2);
    let key = AttributeKey::new("portable".into(), b"mode".to_vec()).unwrap();
    let second = AttributeKey::new("portable".into(), b"mtime".to_vec()).unwrap();
    vec![
        (
            ObjectRole::WholeFile,
            encode_whole_file_payload(b"file").unwrap(),
            vec![],
        ),
        (
            ObjectRole::Chunk,
            encode_chunk_object(b"chunk").unwrap(),
            vec![],
        ),
        (
            ObjectRole::ExtentLeaf,
            encode_node(
                &ExtentNode::Leaf {
                    subtree_logical_bytes: 3,
                    extents: vec![
                        ExtentSlice::new(a, 0, 1).unwrap(),
                        ExtentSlice::new(a, 2, 2).unwrap(),
                    ],
                },
                true,
            )
            .unwrap(),
            vec![a, a],
        ),
        (
            ObjectRole::ExtentBranch,
            encode_node(
                &ExtentNode::Branch {
                    level: 1,
                    subtree_logical_bytes: 2,
                    subtree_extent_count: 2,
                    children: vec![
                        ChildDescriptor {
                            cumulative_logical_end: 1,
                            cumulative_extent_end: 1,
                            child_object_id: a,
                        },
                        ChildDescriptor {
                            cumulative_logical_end: 2,
                            cumulative_extent_end: 2,
                            child_object_id: b,
                        },
                    ],
                },
                true,
            )
            .unwrap(),
            vec![a, b],
        ),
        (
            ObjectRole::FileState,
            encode_file_state(FileState {
                logical_len: 2,
                extent_count: 2,
                tree_level: 1,
                profile_id: mapping_profile(),
                mapping_root: a,
            })
            .unwrap(),
            vec![a],
        ),
        (
            ObjectRole::InodeLeaf,
            encode_inode_page(&InodePage::Leaf {
                entries: vec![
                    (
                        1,
                        InodeValue {
                            kind: InodeKind::Directory,
                            namespace_ref_count: 0,
                            content_root: a,
                            metadata_root: b,
                        },
                    ),
                    (
                        2,
                        InodeValue {
                            kind: InodeKind::RegularFile,
                            namespace_ref_count: 2,
                            content_root: b,
                            metadata_root: a,
                        },
                    ),
                ],
            })
            .unwrap(),
            vec![a, b, b, a],
        ),
        (
            ObjectRole::DirectoryLeaf,
            encode_directory_page(&DirectoryPage::Leaf {
                entries: vec![(PathName::new("name").unwrap(), 2)],
            })
            .unwrap(),
            vec![],
        ),
        (
            ObjectRole::DirectoryBranch,
            encode_directory_page(&DirectoryPage::Branch {
                level: 1,
                subtree_count: 2,
                subtree_bytes: 22,
                children: vec![
                    (PathName::new("a").unwrap(), a),
                    (PathName::new("b").unwrap(), b),
                ],
            })
            .unwrap(),
            vec![a, b],
        ),
        (
            ObjectRole::InodeBranch,
            encode_inode_page(&InodePage::Branch {
                level: 1,
                subtree_count: 2,
                children: vec![(1, a), (2, b)],
            })
            .unwrap(),
            vec![a, b],
        ),
        (
            ObjectRole::FilesystemRoot,
            FilesystemRoot::new(profile_id(), scope_for_seed([5; 32]), 1, a)
                .unwrap()
                .encode()
                .unwrap(),
            vec![a],
        ),
        (
            ObjectRole::AttributeLeaf,
            encode_attribute_page(&AttributePage::Leaf {
                subtree_bytes: row_bytes(&key, 37) as u64,
                entries: vec![AttributeEntry {
                    key: key.clone(),
                    value_root: a,
                }],
            })
            .unwrap(),
            vec![a],
        ),
        (
            ObjectRole::AttributeBranch,
            encode_attribute_page(&AttributePage::Branch {
                level: 1,
                subtree_count: 2,
                subtree_bytes: 100,
                children: vec![(key, a), (second, b)],
            })
            .unwrap(),
            vec![a, b],
        ),
        (
            ObjectRole::Symlink,
            SymlinkTarget::new(b"../target".to_vec())
                .unwrap()
                .encode()
                .unwrap(),
            vec![],
        ),
    ]
}

fn admit(role: ObjectRole, bytes: Vec<u8>) -> ContentResult<FinalizedObject> {
    let claimed = ObjectId::for_bytes(&bytes);
    Timing::disabled("admission", |scope| {
        FinalizedObject::admit(
            claimed,
            role,
            bytes,
            ConstructionPolicy::default(),
            scope_for_seed([5; 32]),
            scope.child("object"),
        )
    })
    .0
}

#[test]
fn all_thirteen_roles_derive_exact_references_and_move_the_canonical_allocation() {
    for (role, bytes, refs) in cases() {
        let allocation = bytes.as_ptr();
        let expected = ObjectId::for_bytes(&bytes);
        let admitted = admit(role, bytes).unwrap();
        assert_eq!(admitted.id(), expected);
        assert_eq!(admitted.role(), role);
        assert_eq!(admitted.references(), refs);
        assert!(admitted.predecessors().is_empty());
        assert_eq!(admitted.canonical().as_ptr(), allocation);
    }
}

#[test]
fn role_confusion_truncation_and_generic_envelopes_are_refused() {
    let cases = cases();
    for (actual, canonical, _) in &cases {
        for (claimed, _, _) in &cases {
            if actual != claimed {
                assert!(
                    admit(*claimed, canonical.clone()).is_err(),
                    "{actual:?} as {claimed:?}"
                );
            }
        }
        let mut truncated = canonical.clone();
        truncated.pop();
        assert!(admit(*actual, truncated).is_err());
        let mut trailing = canonical.clone();
        trailing.push(0);
        assert_eq!(
            admit(*actual, trailing).unwrap_err(),
            ContentError::TrailingBytes
        );
        assert!(admit(
            *actual,
            encode_bytes_object(b"valid envelope, no role").unwrap()
        )
        .is_err());
    }
}

#[test]
fn domain_identity_scope_and_selected_store_policy_are_required() {
    let bytes = encode_whole_file_payload(b"file").unwrap();
    let wrong = Timing::disabled("identity", |scope| {
        FinalizedObject::admit(
            id(99),
            ObjectRole::WholeFile,
            bytes,
            ConstructionPolicy::default(),
            scope_for_seed([5; 32]),
            scope.child("object"),
        )
    })
    .0;
    assert_eq!(wrong.unwrap_err(), ContentError::IdentityMismatch);
    let root = FilesystemRoot::new(profile_id(), scope_for_seed([6; 32]), 1, id(1)).unwrap();
    assert!(matches!(
        admit(ObjectRole::FilesystemRoot, root.encode().unwrap()),
        Err(ContentError::ScopeMismatch { .. })
    ));
    assert!(admit(
        ObjectRole::WholeFile,
        encode_whole_file_payload(b"").unwrap()
    )
    .is_err());
    let bytes = encode_whole_file_payload(&vec![9; 131_072]).unwrap();
    assert!(admit(ObjectRole::WholeFile, bytes.clone()).is_err());
    let larger = Timing::disabled("policy", |scope| {
        FinalizedObject::admit(
            ObjectId::for_bytes(&bytes),
            ObjectRole::WholeFile,
            bytes,
            ConstructionPolicy::new(262_144, 8, 4),
            scope_for_seed([5; 32]),
            scope.child("object"),
        )
    })
    .0
    .unwrap();
    assert!(larger.references().is_empty());
}

#[test]
fn rehashed_impossible_counts_summaries_flags_and_values_are_refused() {
    const VALUE: usize = 13;
    for role in [
        ObjectRole::ExtentLeaf,
        ObjectRole::ExtentBranch,
        ObjectRole::InodeLeaf,
        ObjectRole::InodeBranch,
        ObjectRole::DirectoryLeaf,
        ObjectRole::DirectoryBranch,
        ObjectRole::AttributeLeaf,
        ObjectRole::AttributeBranch,
    ] {
        let original = cases().into_iter().find(|case| case.0 == role).unwrap().1;
        let mut count = original.clone();
        count[VALUE + 13..VALUE + 15].copy_from_slice(&u16::MAX.to_be_bytes());
        assert!(admit(role, count).is_err(), "impossible count {role:?}");
        let mut flags = original;
        flags[VALUE + 12] = 1;
        assert!(admit(role, flags).is_err(), "reserved flag {role:?}");
    }
    let state = encode_file_state(FileState {
        logical_len: 0,
        extent_count: 1,
        tree_level: 0,
        profile_id: mapping_profile(),
        mapping_root: id(1),
    })
    .unwrap();
    assert_eq!(
        admit(ObjectRole::FileState, state).unwrap_err(),
        ContentError::InvalidRecord("file state summary")
    );
    let leaf = encode_inode_page(&InodePage::Leaf {
        entries: vec![(
            2,
            InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 0,
                content_root: id(1),
                metadata_root: id(2),
            },
        )],
    })
    .unwrap();
    assert!(admit(ObjectRole::InodeLeaf, leaf).is_err());
    for role in [ObjectRole::DirectoryBranch, ObjectRole::AttributeBranch] {
        let mut bytes = cases().into_iter().find(|case| case.0 == role).unwrap().1;
        bytes[VALUE + 15..VALUE + 23].copy_from_slice(&0_u64.to_be_bytes());
        assert!(admit(role, bytes).is_err());
    }
}
