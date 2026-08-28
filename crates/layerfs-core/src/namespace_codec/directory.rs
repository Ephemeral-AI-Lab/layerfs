pub fn encode_directory_node(node: &DirectoryNodeV1) -> CoreResult<Vec<u8>> {
    let (role, level, count, subtree_count, subtree_bytes) = match node {
        DirectoryNodeV1::Leaf {
            subtree_encoded_bytes,
            entries,
        } => (
            1,
            0,
            entries.len(),
            entries.len() as u64,
            *subtree_encoded_bytes,
        ),
        DirectoryNodeV1::Branch {
            level,
            subtree_entry_count,
            subtree_encoded_bytes,
            children,
        } => (
            2,
            *level,
            children.len(),
            *subtree_entry_count,
            *subtree_encoded_bytes,
        ),
    };
    validate_node_header(level, count)?;
    let mut value = node_header(
        b"LFS4NSP\0",
        role,
        level,
        count,
        subtree_count,
        subtree_bytes,
    )?;
    match node {
        DirectoryNodeV1::Leaf { entries, .. } => {
            ordered(entries.iter().map(|(name, _)| name.as_bytes()))?;
            for (name, inode) in entries {
                put_bytes(&mut value, name.as_bytes())?;
                value.extend_from_slice(inode.as_bytes());
            }
        }
        DirectoryNodeV1::Branch { children, .. } => {
            if level == 0 {
                return Err(CoreError::InvalidRecord("directory branch level"));
            }
            ordered(children.iter().map(|(name, _)| name.as_bytes()))?;
            for (name, child) in children {
                put_bytes(&mut value, name.as_bytes())?;
                value.extend_from_slice(child.as_bytes());
            }
        }
    }
    verify_subtree_bytes(node, subtree_bytes)?;
    finish_node(value)
}

pub fn decode_directory_node(canonical: &[u8]) -> CoreResult<DirectoryNodeV1> {
    let value = decode_node_value(canonical, b"LFS4NSP\0")?;
    let role = value[10];
    let level = value[11];
    let count = node_count(value);
    ensure_count_fits(value, count, 35)?;
    let subtree_count = node_subtree_count(value);
    let subtree_bytes = node_subtree_bytes(value);
    let mut cursor = 31;
    let node = match role {
        1 if level == 0 && subtree_count == count as u64 => {
            let mut entries = Vec::with_capacity(count);
            for _ in 0..count {
                let name = CanonicalName::from_bytes(take_bytes(value, &mut cursor, 255)?)?;
                let inode = InodeId::from_slice(take(value, &mut cursor, 32)?)?;
                entries.push((name, inode));
            }
            DirectoryNodeV1::Leaf {
                subtree_encoded_bytes: subtree_bytes,
                entries,
            }
        }
        2 if level > 0 => {
            let mut children = Vec::with_capacity(count);
            for _ in 0..count {
                let name = CanonicalName::from_bytes(take_bytes(value, &mut cursor, 255)?)?;
                let child = ObjectId::from_bytes(take(value, &mut cursor, 32)?)?;
                children.push((name, child));
            }
            DirectoryNodeV1::Branch {
                level,
                subtree_entry_count: subtree_count,
                subtree_encoded_bytes: subtree_bytes,
                children,
            }
        }
        1 | 2 => return Err(CoreError::InvalidRecord("directory node level")),
        tag => return Err(CoreError::InvalidMappingTag { tag }),
    };
    if cursor != value.len() {
        return Err(CoreError::TrailingBytes);
    }
    encode_directory_node(&node)?;
    Ok(node)
}
