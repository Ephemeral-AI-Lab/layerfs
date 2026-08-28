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
