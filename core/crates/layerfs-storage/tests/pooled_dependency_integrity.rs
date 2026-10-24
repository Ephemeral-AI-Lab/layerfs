//! A valid dependent must not hide a corrupt canonical pooled base.
#[path = "support/memory_metadata.rs"]
mod metadata;
use layerfs_content::{inode_leaf::*, FinalizedObject, ObjectId, ObjectRole};
use layerfs_storage::{
    encoding::pool::leaf,
    location::{InodeValueRow, ObjectLocation, PackDomain, PackInfo},
    pack::{assemble, build_group, layout::PackLane},
    port::*,
    Storage,
};
use std::sync::Arc;

#[test]
fn literal_delta_cannot_hide_a_wrong_canonical_base() {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    let values: Vec<_> = (1u8..=2)
        .map(|n| {
            encode_inode_value(InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 1,
                content_root: ObjectId::for_bytes(&[n]),
                metadata_root: layerfs_content::object::inode_leaf::InodeMetadata::Root(
                    ObjectId::for_bytes(b"metadata"),
                ),
            })
        })
        .collect();
    let objects: Vec<_> = values
        .iter()
        .map(|value| {
            FinalizedObject::new(
                ObjectRole::InodeLeaf,
                InodeLeaf {
                    subtree_bytes: LEAF_ROW_BYTES as u64,
                    rows: vec![InodeLeafRow {
                        serial: 1,
                        value: *value,
                    }],
                }
                .encode()
                .unwrap(),
            )
            .unwrap()
        })
        .collect();
    let target_body = pooled_body(objects[1].canonical(), &[2]).unwrap();
    // FULL base has valid framing but the wrong value for objects[0]'s CID.
    let base_record = leaf::encode_full(&target_body).unwrap();
    // Dependent INSERT replaces every byte; its final canonical CID is correct.
    let mut delta = vec![leaf::POOLED_DELTA_TAG];
    delta.extend_from_slice(objects[0].id().as_bytes());
    delta.extend_from_slice(&(target_body.len() as u32).to_le_bytes());
    delta.extend_from_slice(&1u32.to_le_bytes());
    delta.push(1);
    delta.extend_from_slice(&(target_body.len() as u32).to_le_bytes());
    delta.extend_from_slice(&target_body);
    let bodies = [
        assemble(
            PackLane::Ordinary,
            &[build_group(PackLane::Ordinary, &[base_record], None).unwrap()],
        )
        .unwrap(),
        assemble(
            PackLane::Ordinary,
            &[build_group(PackLane::Ordinary, &[delta], None).unwrap()],
        )
        .unwrap(),
    ];
    metadata
        .publish(&Publication {
            packs: bodies
                .into_iter()
                .enumerate()
                .map(|(i, body)| PublishedPack {
                    info: PackInfo {
                        pack_id: i as i64 + 1,
                        domain: PackDomain::Metadata,
                        length: body.len(),
                        key: ObjectKey::for_bytes(&body),
                    },
                    body: Arc::new(body),
                })
                .collect(),
            objects: objects
                .iter()
                .enumerate()
                .map(|(i, object)| ObjectLocation {
                    object_id: object.id(),
                    role: object.role(),
                    canonical_length: object.canonical_len(),
                    pack_id: i as i64 + 1,
                    group_number: 0,
                    record_number: 0,
                })
                .collect(),
            inode_values: values
                .iter()
                .zip(1u32..)
                .map(|(value, ordinal)| InodeValueRow {
                    ordinal,
                    value: *value,
                })
                .collect(),
            ..Publication::default()
        })
        .unwrap();
    let storage = Storage::new(metadata).unwrap();
    let error = storage
        .reader()
        .unwrap()
        .read_objects(&[objects[1].id()])
        .unwrap_err();
    assert!(
        error.to_string().contains("pooled canonical identity"),
        "{error}"
    );
}
