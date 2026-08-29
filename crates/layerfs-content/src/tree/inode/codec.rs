use crate::tree::directory::codec::{
    decode_node_value, exact_value, finish_node, node_count, node_header, node_subtree_bytes,
    node_subtree_count, ordered, validate_node_header, VERSION,
};
use crate::tree::inode::{InodeId, InodeKind, InodeRecordV1};
use crate::{encode_bytes_object, CoreError, CoreResult, ObjectId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InodeTableNodeV1 {
    Leaf(Vec<(InodeId, ObjectId)>),
    Branch {
        level: u8,
        subtree_entry_count: u64,
        children: Vec<(InodeId, ObjectId)>,
    },
}

pub fn encode_inode_table_node(node: &InodeTableNodeV1) -> CoreResult<Vec<u8>> {
    let (role, level, count, subtree_count) = match node {
        InodeTableNodeV1::Leaf(entries) => (7, 0, entries.len(), entries.len() as u64),
        InodeTableNodeV1::Branch {
            level,
            subtree_entry_count,
            children,
        } => (8, *level, children.len(), *subtree_entry_count),
    };
    validate_node_header(level, count)?;
    if count > 127 {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    let mut value = node_header(
        b"LFS4INT\0",
        role,
        level,
        count,
        subtree_count,
        subtree_count
            .checked_mul(64)
            .ok_or(CoreError::LengthOverflow)?,
    )?;
    match node {
        InodeTableNodeV1::Leaf(entries) => {
            ordered(entries.iter().map(|(id, _)| id.as_bytes().as_slice()))?;
            for (id, record) in entries {
                value.extend_from_slice(id.as_bytes());
                value.extend_from_slice(record.as_bytes());
            }
        }
        InodeTableNodeV1::Branch {
            level, children, ..
        } => {
            if *level == 0 {
                return Err(CoreError::InvalidRecord("inode branch level"));
            }
            ordered(children.iter().map(|(id, _)| id.as_bytes().as_slice()))?;
            for (id, child) in children {
                value.extend_from_slice(id.as_bytes());
                value.extend_from_slice(child.as_bytes());
            }
        }
    }
    finish_node(value)
}

pub fn decode_inode_table_node(canonical: &[u8]) -> CoreResult<InodeTableNodeV1> {
    let value = decode_node_value(canonical, b"LFS4INT\0")?;
    let role = value[10];
    let level = value[11];
    let count = node_count(value);
    if count > 127 {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    let subtree_count = node_subtree_count(value);
    let expected_subtree_bytes = subtree_count
        .checked_mul(64)
        .ok_or(CoreError::LengthOverflow)?;
    if node_subtree_bytes(value) != expected_subtree_bytes {
        return Err(CoreError::InvalidRecord("inode subtree encoded bytes"));
    }
    let expected = 31 + count.checked_mul(64).ok_or(CoreError::LengthOverflow)?;
    if value.len() < expected {
        return Err(CoreError::UnexpectedEof);
    }
    if value.len() > expected {
        return Err(CoreError::TrailingBytes);
    }
    let mut entries = Vec::with_capacity(count);
    for entry in value[31..].chunks_exact(64) {
        entries.push((
            InodeId::from_slice(&entry[..32])?,
            ObjectId::from_bytes(&entry[32..])?,
        ));
    }
    ordered(entries.iter().map(|(id, _)| id.as_bytes().as_slice()))?;
    match role {
        7 if level == 0 && subtree_count == count as u64 => Ok(InodeTableNodeV1::Leaf(entries)),
        8 if level > 0 => Ok(InodeTableNodeV1::Branch {
            level,
            subtree_entry_count: subtree_count,
            children: entries,
        }),
        7 | 8 => Err(CoreError::InvalidRecord("inode node level")),
        tag => Err(CoreError::InvalidMappingTag { tag }),
    }
}

pub fn encode_inode_record(value: InodeRecordV1) -> CoreResult<Vec<u8>> {
    let mut bytes = Vec::with_capacity(85);
    bytes.extend_from_slice(b"LFS4INO\0");
    bytes.extend_from_slice(&VERSION.to_be_bytes());
    bytes.extend_from_slice(&[4, 0, value.kind as u8]);
    bytes.extend_from_slice(&value.namespace_ref_count.to_be_bytes());
    bytes.extend_from_slice(value.content_root.as_bytes());
    bytes.extend_from_slice(value.metadata_root.as_bytes());
    encode_bytes_object(&bytes)
}

pub fn decode_inode_record(canonical: &[u8]) -> CoreResult<InodeRecordV1> {
    let bytes = exact_value(canonical, b"LFS4INO\0", 85, 4)?;
    Ok(InodeRecordV1 {
        kind: InodeKind::try_from(bytes[12])?,
        namespace_ref_count: u64::from_be_bytes(bytes[13..21].try_into().unwrap()),
        content_root: ObjectId::from_bytes(&bytes[21..53])?,
        metadata_root: ObjectId::from_bytes(&bytes[53..85])?,
    })
}
