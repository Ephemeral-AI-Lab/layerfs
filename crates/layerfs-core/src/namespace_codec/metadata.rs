pub fn encode_metadata_node(node: &MetadataNodeV1) -> CoreResult<Vec<u8>> {
    let (role, level, count, subtree_count, subtree_bytes) = match node {
        MetadataNodeV1::Leaf {
            subtree_encoded_bytes,
            entries,
        } => (
            9,
            0,
            entries.len(),
            entries.len() as u64,
            *subtree_encoded_bytes,
        ),
        MetadataNodeV1::Branch {
            level,
            subtree_entry_count,
            subtree_encoded_bytes,
            children,
        } => (
            10,
            *level,
            children.len(),
            *subtree_entry_count,
            *subtree_encoded_bytes,
        ),
    };
    validate_node_header(level, count)?;
    let mut value = node_header(
        b"LFS4MET\0",
        role,
        level,
        count,
        subtree_count,
        subtree_bytes,
    )?;
    match node {
        MetadataNodeV1::Leaf { entries, .. } => {
            ordered_keys(entries.iter().map(|entry| &entry.key))?;
            for entry in entries {
                put_bytes(&mut value, entry.key.domain.as_bytes())?;
                put_bytes(&mut value, &entry.key.key)?;
                value.push(1);
                value.extend_from_slice(entry.value_file_root.as_bytes());
            }
        }
        MetadataNodeV1::Branch {
            level, children, ..
        } => {
            if *level == 0 {
                return Err(CoreError::InvalidRecord("metadata branch level"));
            }
            ordered_keys(children.iter().map(|(key, _)| key))?;
            for (key, child) in children {
                put_bytes(&mut value, key.domain.as_bytes())?;
                put_bytes(&mut value, &key.key)?;
                value.extend_from_slice(child.as_bytes());
            }
        }
    }
    if let MetadataNodeV1::Leaf { entries, .. } = node {
        let actual = entries.iter().try_fold(0_u64, |sum, entry| {
            sum.checked_add(37 + entry.key.domain.len() as u64 + entry.key.key.len() as u64)
                .ok_or(CoreError::LengthOverflow)
        })?;
        if actual != subtree_bytes {
            return Err(CoreError::LengthMismatch {
                expected: subtree_bytes,
                actual,
            });
        }
    }
    finish_node(value)
}

pub fn decode_metadata_node(canonical: &[u8]) -> CoreResult<MetadataNodeV1> {
    let value = decode_node_value(canonical, b"LFS4MET\0")?;
    let role = value[10];
    let level = value[11];
    let count = node_count(value);
    ensure_count_fits(value, count, 37)?;
    let subtree_count = node_subtree_count(value);
    let subtree_bytes = node_subtree_bytes(value);
    let mut cursor = 31;
    let node = match role {
        9 if level == 0 && subtree_count == count as u64 => {
            let mut entries = Vec::with_capacity(count);
            for _ in 0..count {
                let domain = std::str::from_utf8(take_bytes(value, &mut cursor, 64)?)
                    .map_err(|_| CoreError::InvalidUtf8)?
                    .to_owned();
                let key = take_bytes(value, &mut cursor, 255)?.to_vec();
                if take(value, &mut cursor, 1)? != [1] {
                    return Err(CoreError::InvalidRecord("metadata required flag"));
                }
                let root = ObjectId::from_bytes(take(value, &mut cursor, 32)?)?;
                entries.push(MetadataEntryV1 {
                    key: MetadataKey::new(domain, key)?,
                    value_file_root: root,
                });
            }
            MetadataNodeV1::Leaf {
                subtree_encoded_bytes: subtree_bytes,
                entries,
            }
        }
        10 if level > 0 => {
            let mut children = Vec::with_capacity(count);
            for _ in 0..count {
                let domain = std::str::from_utf8(take_bytes(value, &mut cursor, 64)?)
                    .map_err(|_| CoreError::InvalidUtf8)?
                    .to_owned();
                let key = take_bytes(value, &mut cursor, 255)?.to_vec();
                let child = ObjectId::from_bytes(take(value, &mut cursor, 32)?)?;
                children.push((MetadataKey::new(domain, key)?, child));
            }
            MetadataNodeV1::Branch {
                level,
                subtree_entry_count: subtree_count,
                subtree_encoded_bytes: subtree_bytes,
                children,
            }
        }
        9 | 10 => return Err(CoreError::InvalidRecord("metadata node level")),
        tag => return Err(CoreError::InvalidMappingTag { tag }),
    };
    if cursor != value.len() {
        return Err(CoreError::TrailingBytes);
    }
    encode_metadata_node(&node)?;
    Ok(node)
}
