//! Golden-byte codec coverage for every filesystem-tree grammar.
//!
//! Each fixture here was encoded by the pinned reference implementation, so the
//! replacement must produce exactly those bytes for exactly those inputs, and must
//! reject every malformed, trailing, reserved or overflowing variant of them.

#![allow(dead_code)]

mod support;

use layerfs_content::filesystem::attributes::codec::{
    decode_attribute_page, encode_attribute_page, AttributeEntry, AttributePage,
};
use layerfs_content::filesystem::attributes::keys::AttributeKey;
use layerfs_content::filesystem::directory::codec::{
    decode_directory_page, encode_directory_page, DirectoryPage,
};
use layerfs_content::filesystem::inode::codec::{decode_inode_page, encode_inode_page, InodePage};
use layerfs_content::filesystem::path::PathName;
use layerfs_content::filesystem::root::{FilesystemRoot, ROOT_VALUE_BYTES};
use layerfs_content::filesystem::symlink::SymlinkTarget;
use layerfs_content::filesystem::{profile_id, scope_for_seed, InodeScope};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{ContentError, ObjectId, ObjectRole};
use support::filesystem::synthetic;

mod manifest {
    include!("fixtures/filesystem/manifest.rs");
}

fn id(text: &str) -> ObjectId {
    text.parse().expect("fixture identity")
}

fn codec_case(case: (&str, u8, u8, u64, &[u8])) -> (String, ObjectId, u8, u8, u64, Vec<u8>) {
    (
        case.0.to_owned(),
        id(case.0),
        case.1,
        case.2,
        case.3,
        case.4.to_vec(),
    )
}

fn value(kind: InodeKind, count: u64, content: &str, metadata: &str) -> InodeValue {
    InodeValue {
        kind,
        namespace_ref_count: count,
        content_root: synthetic(content),
        metadata_root: synthetic(metadata),
    }
}

#[test]
fn inode_pages_match_the_reference_bytes() {
    let (_, expected_id, _, _, _, bytes) = codec_case(manifest::CODEC_INODE_LEAF);
    assert_eq!(ObjectId::for_bytes(&bytes), expected_id);
    let page = decode_inode_page(&bytes).expect("decodes");
    assert_eq!(
        page,
        InodePage::Leaf {
            entries: vec![
                (
                    7,
                    value(InodeKind::RegularFile, 1, "codec/content-a", "codec/meta-a")
                ),
                (
                    9,
                    value(InodeKind::Directory, 1, "codec/content-b", "codec/meta-b")
                ),
            ],
        }
    );
    let observed = encode_inode_page(&page).expect("encodes");
    assert_eq!(observed, bytes, "inode leaf bytes");

    let (_, branch_id, _, _, _, branch_bytes) = codec_case(manifest::CODEC_INODE_BRANCH);
    assert_eq!(ObjectId::for_bytes(&branch_bytes), branch_id);
    let branch = decode_inode_page(&branch_bytes).expect("decodes");
    assert_eq!(
        branch,
        InodePage::Branch {
            level: 1,
            subtree_count: 128,
            children: vec![
                (50, synthetic("codec/child-a")),
                (128, synthetic("codec/child-b")),
            ],
        }
    );
    assert_eq!(encode_inode_page(&branch).expect("encodes"), branch_bytes);
}

#[test]
fn directory_pages_match_the_reference_bytes() {
    let (_, leaf_id, _, _, _, leaf_bytes) = codec_case(manifest::CODEC_DIRECTORY_LEAF);
    assert_eq!(ObjectId::for_bytes(&leaf_bytes), leaf_id);
    let leaf = decode_directory_page(&leaf_bytes).expect("decodes");
    assert_eq!(
        leaf,
        DirectoryPage::Leaf {
            entries: vec![
                (PathName::new("alpha").unwrap(), 3),
                (PathName::new("beta").unwrap(), 11),
            ],
        }
    );
    assert_eq!(encode_directory_page(&leaf).expect("encodes"), leaf_bytes);

    let (_, branch_id, _, _, _, branch_bytes) = codec_case(manifest::CODEC_DIRECTORY_BRANCH);
    assert_eq!(ObjectId::for_bytes(&branch_bytes), branch_id);
    let branch = decode_directory_page(&branch_bytes).expect("decodes");
    assert_eq!(
        branch,
        DirectoryPage::Branch {
            level: 1,
            subtree_count: 129,
            subtree_bytes: 4096,
            children: vec![
                (PathName::new("m").unwrap(), synthetic("codec/dir-a")),
                (PathName::new("z").unwrap(), synthetic("codec/dir-b")),
            ],
        }
    );
    assert_eq!(
        encode_directory_page(&branch).expect("encodes"),
        branch_bytes
    );
}

#[test]
fn roots_and_symlinks_match_the_reference_bytes() {
    let (_, root_id, _, _, _, root_bytes) = codec_case(manifest::CODEC_ROOT);
    assert_eq!(ObjectId::for_bytes(&root_bytes), root_id);
    let root = FilesystemRoot::decode(&root_bytes).expect("decodes");
    assert_eq!(root.profile(), profile_id());
    assert_eq!(root.scope(), scope_for_seed([0x5a; 32]));
    assert_eq!(root.root_inode().serial(), 1);
    assert_eq!(root.inode_table(), synthetic("codec/inode-table"));
    assert_eq!(root.encode().expect("encodes"), root_bytes);

    let (_, symlink_id, _, _, _, symlink_bytes) = codec_case(manifest::CODEC_SYMLINK);
    assert_eq!(ObjectId::for_bytes(&symlink_bytes), symlink_id);
    let target = SymlinkTarget::decode(&symlink_bytes).expect("decodes");
    assert_eq!(target.as_bytes(), b"target/path");
    assert_eq!(target.encode().expect("encodes"), symlink_bytes);
}

#[test]
fn attribute_pages_match_the_reference_bytes() {
    let (_, attr_id, _, _, _, attr_bytes) = codec_case(manifest::CODEC_METADATA_LEAF);
    assert_eq!(ObjectId::for_bytes(&attr_bytes), attr_id);
    let expected_bytes = 37 + 8 + 4 + 37 + 9 + 4;
    let page = decode_attribute_page(&attr_bytes).expect("decodes");
    assert_eq!(
        page,
        AttributePage::Leaf {
            subtree_bytes: expected_bytes,
            entries: vec![
                AttributeEntry {
                    key: AttributeKey::new("portable".to_owned(), b"mode".to_vec()).unwrap(),
                    value_root: synthetic("codec/mode-value"),
                },
                AttributeEntry {
                    key: AttributeKey::new("portable".to_owned(), b"mtime".to_vec()).unwrap(),
                    value_root: synthetic("codec/mtime-value"),
                },
            ],
        }
    );
    assert_eq!(encode_attribute_page(&page).expect("encodes"), attr_bytes);
}

#[test]
fn malformed_and_reserved_input_is_rejected_once() {
    let (_, _, _, _, _, leaf_bytes) = codec_case(manifest::CODEC_INODE_LEAF);
    for length in 0..leaf_bytes.len() {
        assert!(
            decode_inode_page(&leaf_bytes[..length]).is_err(),
            "truncated inode leaf of {length} bytes was accepted"
        );
    }
    let mut trailing = leaf_bytes.clone();
    trailing.push(0);
    assert!(matches!(
        decode_inode_page(&trailing),
        Err(ContentError::TrailingBytes)
    ));
    let mut reserved = leaf_bytes.clone();
    reserved[13 + 12] = 1;
    assert!(matches!(
        decode_inode_page(&reserved),
        Err(ContentError::InvalidRecord("node flags"))
    ));
    let mut version = leaf_bytes.clone();
    version[13 + 9] = 9;
    assert!(matches!(
        decode_inode_page(&version),
        Err(ContentError::UnsupportedMappingVersion { .. })
    ));
    let mut role = leaf_bytes.clone();
    role[13 + 10] = 3;
    assert!(matches!(
        decode_inode_page(&role),
        Err(ContentError::InvalidRecord("inode role/level"))
    ));
    // The recorded subtree byte total is the encoded row width; a leaf that
    // claims the value width alone is rejected.
    let mut total = leaf_bytes.clone();
    let count = u64::from(u16::from_be_bytes([total[13 + 13], total[13 + 14]]));
    total[13 + 23..13 + 31].copy_from_slice(&(count * 73).to_be_bytes());
    assert!(decode_inode_page(&total).is_err());

    let (_, _, _, _, _, root_bytes) = codec_case(manifest::CODEC_ROOT);
    let mut foreign = root_bytes.clone();
    foreign[13 + 12] = 0x7f;
    assert!(matches!(
        FilesystemRoot::decode(&foreign),
        Err(ContentError::UnsupportedProfile { .. })
    ));
    assert_eq!(root_bytes.len(), 13 + ROOT_VALUE_BYTES);
    let mut padded = root_bytes.clone();
    padded.push(0);
    assert!(FilesystemRoot::decode(&padded).is_err());

    // A symlink target with a declared length past the payload is rejected.
    let (_, _, _, _, _, symlink_bytes) = codec_case(manifest::CODEC_SYMLINK);
    let mut long = symlink_bytes.clone();
    long[13 + 12..13 + 14].copy_from_slice(&4095_u16.to_be_bytes());
    assert!(SymlinkTarget::decode(&long).is_err());

    // An attribute page whose declared entry count cannot fit is rejected.
    let (_, _, _, _, _, attr_bytes) = codec_case(manifest::CODEC_METADATA_LEAF);
    let mut overfull = attr_bytes.clone();
    overfull[13 + 13..13 + 15].copy_from_slice(&4095_u16.to_be_bytes());
    assert!(decode_attribute_page(&overfull).is_err());
}

#[test]
fn roles_and_identity_are_not_interchangeable() {
    let (_, _, _, _, _, leaf_bytes) = codec_case(manifest::CODEC_INODE_LEAF);
    assert_ne!(
        ObjectId::for_bytes(&leaf_bytes),
        id(manifest::CODEC_DIRECTORY_LEAF.0),
        "different grammars must not share an identity"
    );
    assert_eq!(
        InodeScope::from_object(synthetic("scope")).object(),
        synthetic("scope")
    );
    assert_eq!(
        ObjectRole::InodeLeaf.code(),
        6,
        "persisted role codes are part of the stored contract"
    );
}

#[test]
fn a_directory_leaf_never_exceeds_the_page_ceiling() {
    // A leaf page is a fixed header plus one row per entry. Two bounds apply and
    // they are different questions: the row count may never exceed what the
    // shortest possible row allows, and a particular set of rows may fit only if
    // its encoded size does. The reference's threshold added one row to the count
    // quotient, which is one row more than a page can ever hold, so a count that
    // passed its partition check could still fail its size check and be reported
    // as a size error.
    let ceiling = layerfs_content::filesystem::limits::MAXIMUM_DIRECTORY_LEAF_ROWS;
    let page_bytes = layerfs_content::filesystem::limits::MAXIMUM_PAGE_BYTES;
    let empty = layerfs_content::filesystem::limits::EMPTY_PAGE_BYTES;
    assert_eq!(ceiling, (page_bytes - empty) / (2 + 1 + 8));
    let rows = |count: usize, name_bytes: usize| {
        (0..count)
            .map(|index| {
                let name = format!("{index:0width$}", width = name_bytes);
                (
                    PathName::new(&name[..name_bytes.min(name.len())]).expect("name"),
                    index as u64 + 2,
                )
            })
            .collect::<Vec<_>>()
    };
    let encode = |count: usize, name_bytes: usize| {
        encode_directory_page(&DirectoryPage::Leaf {
            entries: rows(count, name_bytes),
        })
    };
    // One row past the count quotient is a partition refusal.
    assert!(
        matches!(
            encode(ceiling + 1, 6),
            Err(ContentError::NonCanonicalPagePartition)
        ),
        "one row over the count quotient is a partition refusal"
    );
    // Within the count quotient the encoded size decides, and it decides as a
    // partition refusal too: 64 rows of the longest names cannot fit.
    assert!(
        matches!(
            encode(64, 255),
            Err(ContentError::NonCanonicalPagePartition)
        ),
        "a page whose rows do not fit is a partition refusal, not a size error"
    );
    // Thirty of the longest names do fit, and short names fit far more than that.
    let longest = encode(30, 255).expect("30 long rows fit");
    assert!(longest.len() <= page_bytes);
    let short = encode(200, 6).expect("200 short rows fit");
    assert!(short.len() <= page_bytes);
}
