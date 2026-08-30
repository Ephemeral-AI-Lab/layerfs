use crate::file::extent::ExtentNodeV3;
use crate::file::extent_codec::{decode_file_state, decode_node};
use crate::tree::directory::codec::{
    decode_directory_node, decode_directory_state, decode_namespace_root, DirectoryNodeV1,
};
use crate::tree::inode::codec::{decode_inode_record, decode_inode_table_node, InodeTableNodeV1};
use crate::tree::metadata::codec::{decode_metadata_node, MetadataNodeV1};
use crate::{CoreResult, Object, ObjectId};

pub fn referenced_objects(canonical: &[u8]) -> CoreResult<Vec<ObjectId>> {
    let object = crate::decode_object(canonical)?;
    let Object::Bytes(value) = object else {
        let Object::Directory(entries) = object else {
            unreachable!()
        };
        return Ok(entries
            .into_iter()
            .map(|entry| entry.reference().id())
            .collect());
    };
    let magic = value.get(..8).unwrap_or_default();
    Ok(match magic {
        b"LFS4FSR\0" => vec![decode_namespace_root(canonical)?.inode_table_root],
        b"LFS4INT\0" => match decode_inode_table_node(canonical)? {
            InodeTableNodeV1::Leaf(entries) => entries.into_iter().map(|(_, id)| id).collect(),
            InodeTableNodeV1::Branch { children, .. } => {
                children.into_iter().map(|(_, id)| id).collect()
            }
        },
        b"LFS4INO\0" => {
            let record = decode_inode_record(canonical)?;
            vec![record.content_root, record.metadata_root]
        }
        b"LFS4DIR\0" => vec![decode_directory_state(canonical)?.mapping_root],
        b"LFS4NSP\0" => match decode_directory_node(canonical)? {
            DirectoryNodeV1::Leaf { .. } => Vec::new(),
            DirectoryNodeV1::Branch { children, .. } => {
                children.into_iter().map(|(_, id)| id).collect()
            }
        },
        b"LFS4MET\0" => match decode_metadata_node(canonical)? {
            MetadataNodeV1::Leaf { entries, .. } => entries
                .into_iter()
                .map(|entry| entry.value_file_root)
                .collect(),
            MetadataNodeV1::Branch { children, .. } => {
                children.into_iter().map(|(_, id)| id).collect()
            }
        },
        b"LFS4MAP\0" => {
            if let Ok(state) = decode_file_state(canonical) {
                vec![state.mapping_root]
            } else {
                match decode_node(canonical)? {
                    ExtentNodeV3::Leaf { extents, .. } => extents
                        .into_iter()
                        .map(|extent| extent.payload_object_id)
                        .collect(),
                    ExtentNodeV3::Branch { children, .. } => children
                        .into_iter()
                        .map(|child| child.child_object_id)
                        .collect(),
                }
            }
        }
        b"LFS4CHK\0" => Vec::new(),
        b"LFS4LNK\0" => Vec::new(),
        _ => Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::extent_codec::{decode_chunk_payload, CHUNK_MAGIC};
    use crate::file::rope::{build, read_all, ObjectStore};
    use crate::{decode_bytes_object, CoreError, CoreResult};
    use std::collections::{BTreeMap, BTreeSet};

    #[derive(Default)]
    struct MemoryStore(BTreeMap<ObjectId, Vec<u8>>);

    impl ObjectStore for MemoryStore {
        fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
            self.0.get(&id).cloned().ok_or(CoreError::MissingObject)
        }

        fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
            let id = ObjectId::for_bytes(canonical);
            self.0.insert(id, canonical.to_vec());
            Ok(id)
        }
    }

    #[test]
    fn every_internal_magic_is_safe_as_user_file_prefix_and_transfer_leaf() {
        for magic in [
            &b"LFS4FSR\0"[..],
            &b"LFS4INT\0"[..],
            &b"LFS4INO\0"[..],
            &b"LFS4DIR\0"[..],
            &b"LFS4NSP\0"[..],
            &b"LFS4MET\0"[..],
            &b"LFS4MAP\0"[..],
            &b"LFS4LNK\0"[..],
            &b"LFS4ACL\0"[..],
            &b"LFS4CHK\0"[..],
        ] {
            let mut input = magic.to_vec();
            input.extend_from_slice(b"arbitrary-user-bytes");
            let mut store = MemoryStore::default();
            let (root, _) = build(&mut store, input.as_slice()).unwrap();

            let mut round_trip = Vec::new();
            read_all(&store, root, &mut round_trip).unwrap();
            assert_eq!(round_trip, input);

            let mut pending = vec![root.0];
            let mut seen = BTreeSet::new();
            let mut chunks = 0;
            while let Some(id) = pending.pop() {
                if !seen.insert(id) {
                    continue;
                }
                let canonical = store.0.get(&id).unwrap();
                let references = referenced_objects(canonical).unwrap();
                if decode_bytes_object(canonical)
                    .unwrap()
                    .starts_with(CHUNK_MAGIC)
                {
                    assert_eq!(
                        decode_chunk_payload(decode_bytes_object(canonical).unwrap()).unwrap(),
                        input
                    );
                    assert!(references.is_empty());
                    chunks += 1;
                }
                pending.extend(references);
            }
            assert_eq!(chunks, 1, "magic={magic:?}");
        }
    }
}
