//! B2 canonical-hash-valid corruptions persisted through real pack mechanics.
mod support;
use layerfs_content::{
    encode_whole_file_payload,
    file::mapping::{self, ChildDescriptor, ExtentNode, ExtentSlice, FileState},
    filesystem::{
        attributes::{
            encode_attribute_page, AttributeEntry, AttributeKey, AttributePage, PortableMetadata,
        },
        symlink::SymlinkTarget,
    },
    inode_leaf::{
        encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeMetadata, InodeValue,
        LEAF_ROW_BYTES,
    },
    ObjectId, ObjectRole,
};
use support::doctor::{clean, failure, Fixture};
fn chunk(f: &Fixture, bytes: &[u8]) -> ObjectId {
    f.put(
        ObjectRole::Chunk,
        mapping::encode_chunk_object(bytes).unwrap(),
    )
    .id()
}
fn mapping_file(f: &Fixture, bytes: &[u8]) -> ObjectId {
    let payload = chunk(f, bytes);
    let leaf = f
        .put(
            ObjectRole::ExtentLeaf,
            mapping::encode_node(
                &ExtentNode::Leaf {
                    subtree_logical_bytes: bytes.len() as u64,
                    extents: vec![ExtentSlice::new(payload, 0, bytes.len() as u32).unwrap()],
                },
                true,
            )
            .unwrap(),
        )
        .id();
    f.put(
        ObjectRole::FileState,
        mapping::encode_file_state(FileState {
            logical_len: bytes.len() as u64,
            extent_count: 1,
            tree_level: 0,
            profile_id: mapping::profile_id(),
            mapping_root: leaf,
        })
        .unwrap(),
    )
    .id()
}
#[test]
fn c06_extent_payload_bounds_and_file_lengths_are_semantic_checks() {
    for broken in [false, true] {
        let f = Fixture::new();
        let payload = chunk(&f, b"abcd");
        let leaf = f
            .put(
                ObjectRole::ExtentLeaf,
                mapping::encode_node(
                    &ExtentNode::Leaf {
                        subtree_logical_bytes: 4,
                        extents: vec![
                            ExtentSlice::new(payload, if broken { 2 } else { 0 }, 4).unwrap()
                        ],
                    },
                    true,
                )
                .unwrap(),
            )
            .id();
        f.put(
            ObjectRole::FileState,
            mapping::encode_file_state(FileState {
                logical_len: 4,
                extent_count: 1,
                tree_level: 0,
                profile_id: mapping::profile_id(),
                mapping_root: leaf,
            })
            .unwrap(),
        );
        let out = f.audit(false);
        if broken {
            failure(&out, "store.objects");
        } else {
            clean(&out);
        }
    }
    for broken in [false, true] {
        let f = Fixture::new();
        let root = mapping_file(&f, b"small mapped value");
        let raw = f.raw();
        let bytes: Vec<u8> = layerfs_storage::Storage::new(f.handles.storage.clone())
            .unwrap()
            .reader()
            .unwrap()
            .read_objects(&[root])
            .unwrap()
            .remove(0);
        let mut state = mapping::decode_file_state(&bytes).unwrap();
        state.logical_len += u64::from(broken);
        f.put(
            ObjectRole::FileState,
            mapping::encode_file_state(state).unwrap(),
        );
        let out = f.audit(false);
        if broken {
            failure(&out, "store.objects");
        } else {
            clean(&out);
        }
        drop(raw);
    }
}
#[test]
fn c06_branch_child_summaries_are_checked_beyond_valid_hashes() {
    for broken in [false, true] {
        let f = Fixture::new();
        let payload = chunk(&f, b"abcd");
        let leaf = f
            .put(
                ObjectRole::ExtentLeaf,
                mapping::encode_node(
                    &ExtentNode::Leaf {
                        subtree_logical_bytes: 256,
                        extents: vec![ExtentSlice::new(payload, 0, 4).unwrap(); 64],
                    },
                    false,
                )
                .unwrap(),
            )
            .id();
        f.put(
            ObjectRole::ExtentBranch,
            mapping::encode_node(
                &ExtentNode::Branch {
                    level: 1,
                    subtree_logical_bytes: 512,
                    subtree_extent_count: 128,
                    children: vec![
                        ChildDescriptor {
                            cumulative_logical_end: if broken { 255 } else { 256 },
                            cumulative_extent_end: 64,
                            child_object_id: leaf,
                        },
                        ChildDescriptor {
                            cumulative_logical_end: 512,
                            cumulative_extent_end: 128,
                            child_object_id: leaf,
                        },
                    ],
                },
                true,
            )
            .unwrap(),
        );
        let out = f.audit(false);
        if broken {
            failure(&out, "store.objects");
        } else {
            clean(&out);
        }
    }
}
#[test]
fn c07_symlink_syntax_is_checked_without_resolving_targets() {
    for broken in [false, true] {
        let f = Fixture::new();
        let mut bytes = SymlinkTarget::new(b"/does/not/exist/../dangling".to_vec())
            .unwrap()
            .encode()
            .unwrap();
        if broken {
            *bytes.last_mut().unwrap() = 0;
        }
        let object = f.put(ObjectRole::Symlink, bytes);
        let out = f.audit(false);
        if broken {
            failure(&out, "store.objects");
            assert!(out
                .iter()
                .any(|o| o.message.contains(&object.id().to_string())));
        } else {
            clean(&out);
        }
    }
}
#[test]
fn c07_nonportable_attribute_syntax_and_value_kind_are_checked() {
    for mutation in 0..3 {
        let f = Fixture::new();
        let root = if mutation == 2 {
            f.put(
                ObjectRole::WholeFile,
                encode_whole_file_payload(b"wrong kind").unwrap(),
            )
            .id()
        } else {
            mapping_file(&f, b"opaque attribute")
        };
        let key = AttributeKey::new("user".to_owned(), b"note".to_vec()).unwrap();
        let mut bytes = encode_attribute_page(&AttributePage::Leaf {
            subtree_bytes: (4 + 4 + 37) as u64,
            entries: vec![AttributeEntry {
                key,
                value_root: root,
            }],
        })
        .unwrap();
        if mutation == 1 {
            let at = bytes.windows(4).position(|b| b == b"user").unwrap();
            bytes[at + 1] = 0;
        }
        f.put(ObjectRole::AttributeLeaf, bytes);
        let out = f.audit(false);
        if mutation == 0 {
            clean(&out);
        } else {
            failure(&out, "store.objects");
        }
    }
}
#[test]
fn c07_portable_metadata_is_validated_in_retained_inode_context() {
    for broken in [false, true] {
        let f = Fixture::new();
        let mode = mapping_file(
            &f,
            &(if broken { 0o10000u32 } else { 0o644u32 }).to_be_bytes(),
        );
        let mtime = mapping_file(&f, &[0u8; 12]);
        let entries = vec![
            AttributeEntry {
                key: AttributeKey::new("portable".to_owned(), b"mode".to_vec()).unwrap(),
                value_root: mode,
            },
            AttributeEntry {
                key: AttributeKey::new("portable".to_owned(), b"mtime".to_vec()).unwrap(),
                value_root: mtime,
            },
        ];
        let attributes = f
            .put(
                ObjectRole::AttributeLeaf,
                encode_attribute_page(&AttributePage::Leaf {
                    subtree_bytes: (2 * (8 + 4 + 37) + 1) as u64,
                    entries,
                })
                .unwrap(),
            )
            .id();
        let content = f
            .put(
                ObjectRole::WholeFile,
                encode_whole_file_payload(b"retained data").unwrap(),
            )
            .id();
        let value = encode_inode_value(InodeValue {
            kind: InodeKind::RegularFile,
            namespace_ref_count: 1,
            content_root: content,
            metadata_root: InodeMetadata::Root(attributes),
        });
        f.inode(
            InodeLeaf {
                subtree_bytes: LEAF_ROW_BYTES as u64,
                rows: vec![InodeLeafRow { serial: 7, value }],
            }
            .encode()
            .unwrap(),
            vec![value],
        );
        let out = f.audit(false);
        if broken {
            failure(&out, "store.objects");
        } else {
            clean(&out);
        }
    }
}
#[test]
fn c05_missing_pooled_ordinal_and_corrupt_unused_blocks_are_not_absence() {
    let f = Fixture::new();
    let content = f
        .put(
            ObjectRole::WholeFile,
            encode_whole_file_payload(b"file").unwrap(),
        )
        .id();
    let value = encode_inode_value(InodeValue {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 1,
        content_root: content,
        metadata_root: InodeMetadata::Inline(PortableMetadata {
            mode: 0o644,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
        }),
    });
    let id = f.inode(
        InodeLeaf {
            subtree_bytes: LEAF_ROW_BYTES as u64,
            rows: vec![InodeLeafRow { serial: 7, value }],
        }
        .encode()
        .unwrap(),
        vec![value],
    );
    clean(&f.audit(false));
    f.raw()
        .execute_batch("DELETE FROM inode_value_block")
        .unwrap();
    let out = f.audit(false);
    failure(&out, "store.objects");
    assert!(out.iter().any(|o| o.message.contains(&id.to_string())));
}

#[test]
fn c06_valid_sparse_file_uses_real_constructor_and_save() {
    struct Runs(u8);
    impl layerfs_content::FileRuns for Runs {
        fn read_run(
            &mut self,
            out: &mut [u8],
        ) -> layerfs_content::ContentResult<layerfs_content::FileRun> {
            let n = self.0;
            self.0 += 1;
            Ok(match n {
                0 => {
                    out[..4].copy_from_slice(b"data");
                    layerfs_content::FileRun::Data(4)
                }
                1 => layerfs_content::FileRun::Zero(8 * 1024 * 1024),
                2 => {
                    out[..4].copy_from_slice(b"tail");
                    layerfs_content::FileRun::Data(4)
                }
                _ => layerfs_content::FileRun::End,
            })
        }
    }
    let f = Fixture::new();
    let storage = layerfs_storage::Storage::new(f.handles.storage.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    let policy = layerfs_content::ConstructionPolicy::frozen_default();
    let result = layerfs_telemetry::timer::Timing::disabled("doctor.sparse", |scope| {
        layerfs_content::construct_runs(
            policy,
            &policy.capacities(),
            &mut Runs(0),
            &mut save.sink(),
            scope.child("construct"),
        )
    })
    .0
    .unwrap();
    assert_eq!(result.zero_bytes, 8 * 1024 * 1024);
    save.finish().unwrap();
    clean(&f.audit(false));
}
#[test]
fn c05_crc_damage_is_checked_even_after_all_pooled_objects_are_unbound() {
    for unused in [false, true] {
        let f = Fixture::new();
        let content = f
            .put(
                ObjectRole::WholeFile,
                encode_whole_file_payload(b"ordinal data").unwrap(),
            )
            .id();
        let value = encode_inode_value(InodeValue {
            kind: InodeKind::RegularFile,
            namespace_ref_count: 1,
            content_root: content,
            metadata_root: InodeMetadata::Inline(PortableMetadata {
                mode: 0o644,
                mtime_seconds: 0,
                mtime_nanoseconds: 0,
            }),
        });
        let id = f.inode(
            InodeLeaf {
                subtree_bytes: LEAF_ROW_BYTES as u64,
                rows: vec![InodeLeafRow { serial: 7, value }],
            }
            .encode()
            .unwrap(),
            vec![value],
        );
        clean(&f.audit(false));
        let raw = f.raw();
        if unused {
            let length: i64 = raw
                .query_row(
                    "SELECT canonical_length FROM object_location WHERE object_id=?1",
                    [id.as_bytes().as_slice()],
                    |r| r.get(0),
                )
                .unwrap();
            raw.execute(
                "DELETE FROM object_location WHERE object_id=?1",
                [id.as_bytes().as_slice()],
            )
            .unwrap();
            raw.execute("UPDATE store_totals SET canonical=canonical-?1", [length])
                .unwrap();
            clean(&f.audit(false));
        }
        let mut body: Vec<u8> = raw
            .query_row("SELECT body FROM inode_value_block", [], |r| r.get(0))
            .unwrap();
        *body.last_mut().unwrap() ^= 1;
        raw.execute("UPDATE inode_value_block SET body=?1", [body])
            .unwrap();
        failure(&f.audit(false), "store.ordinal_blocks");
    }
}
