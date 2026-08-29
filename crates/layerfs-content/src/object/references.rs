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
        b"LFS4LNK\0" => Vec::new(),
        _ => Vec::new(),
    })
}
