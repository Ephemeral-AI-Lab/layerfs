fn exact_value<'a>(
    canonical: &'a [u8],
    magic: &[u8; 8],
    len: usize,
    role: u8,
) -> CoreResult<&'a [u8]> {
    let bytes = decode_bytes_object(canonical)?;
    if bytes.len() < len {
        return Err(CoreError::UnexpectedEof);
    }
    if bytes.len() > len {
        return Err(CoreError::TrailingBytes);
    }
    header(bytes, magic, role)?;
    Ok(bytes)
}

fn header(bytes: &[u8], magic: &[u8; 8], role: u8) -> CoreResult<()> {
    if &bytes[..8] != magic {
        return Err(CoreError::Unsupported);
    }
    let version = u16::from_be_bytes([bytes[8], bytes[9]]);
    if version != VERSION {
        return Err(CoreError::UnsupportedMappingVersion { version });
    }
    if bytes[10] != role {
        return Err(CoreError::WrongLogicalRole);
    }
    if bytes[11] != 0 {
        return Err(CoreError::InvalidRecord("namespace flags"));
    }
    Ok(())
}

fn node_header(
    magic: &[u8; 8],
    role: u8,
    level: u8,
    count: usize,
    subtree_count: u64,
    subtree_bytes: u64,
) -> CoreResult<Vec<u8>> {
    let count = u16::try_from(count).map_err(|_| CoreError::LengthOverflow)?;
    let mut value = Vec::with_capacity(31);
    value.extend_from_slice(magic);
    value.extend_from_slice(&VERSION.to_be_bytes());
    value.extend_from_slice(&[role, level, 0]);
    value.extend_from_slice(&count.to_be_bytes());
    value.extend_from_slice(&subtree_count.to_be_bytes());
    value.extend_from_slice(&subtree_bytes.to_be_bytes());
    Ok(value)
}

fn decode_node_value<'a>(canonical: &'a [u8], magic: &[u8; 8]) -> CoreResult<&'a [u8]> {
    if canonical.len() > NODE_LIMIT {
        return Err(CoreError::ObjectLimitExceeded);
    }
    let value = decode_bytes_object(canonical)?;
    if value.len() < 31 {
        return Err(CoreError::UnexpectedEof);
    }
    if &value[..8] != magic {
        return Err(CoreError::Unsupported);
    }
    let version = u16::from_be_bytes([value[8], value[9]]);
    if version != VERSION {
        return Err(CoreError::UnsupportedMappingVersion { version });
    }
    if value[12] != 0 {
        return Err(CoreError::InvalidRecord("namespace node flags"));
    }
    validate_node_header(value[11], node_count(value))?;
    Ok(value)
}

fn validate_node_header(level: u8, count: usize) -> CoreResult<()> {
    if level > 31 {
        return Err(CoreError::MappingDepthExceeded);
    }
    if count > u16::MAX as usize {
        return Err(CoreError::ObjectLimitExceeded);
    }
    Ok(())
}

fn node_count(value: &[u8]) -> usize {
    usize::from(u16::from_be_bytes([value[13], value[14]]))
}
fn ensure_count_fits(value: &[u8], count: usize, minimum_entry_bytes: usize) -> CoreResult<()> {
    if count > value.len().saturating_sub(31) / minimum_entry_bytes {
        Err(CoreError::UnexpectedEof)
    } else {
        Ok(())
    }
}
fn node_subtree_count(value: &[u8]) -> u64 {
    u64::from_be_bytes(value[15..23].try_into().unwrap())
}
fn node_subtree_bytes(value: &[u8]) -> u64 {
    u64::from_be_bytes(value[23..31].try_into().unwrap())
}

fn finish_node(value: Vec<u8>) -> CoreResult<Vec<u8>> {
    let canonical = encode_bytes_object(&value)?;
    if canonical.len() > NODE_LIMIT {
        Err(CoreError::ObjectLimitExceeded)
    } else {
        Ok(canonical)
    }
}

fn put_bytes(output: &mut Vec<u8>, bytes: &[u8]) -> CoreResult<()> {
    output.extend_from_slice(
        &u16::try_from(bytes.len())
            .map_err(|_| CoreError::LengthOverflow)?
            .to_be_bytes(),
    );
    output.extend_from_slice(bytes);
    Ok(())
}

fn take<'a>(bytes: &'a [u8], cursor: &mut usize, len: usize) -> CoreResult<&'a [u8]> {
    let end = cursor.checked_add(len).ok_or(CoreError::LengthOverflow)?;
    if end > bytes.len() {
        return Err(CoreError::UnexpectedEof);
    }
    let value = &bytes[*cursor..end];
    *cursor = end;
    Ok(value)
}

fn take_bytes<'a>(bytes: &'a [u8], cursor: &mut usize, max: usize) -> CoreResult<&'a [u8]> {
    let len_bytes = take(bytes, cursor, 2)?;
    let len = usize::from(u16::from_be_bytes([len_bytes[0], len_bytes[1]]));
    if len > max {
        return Err(CoreError::ObjectLimitExceeded);
    }
    take(bytes, cursor, len)
}

fn ordered<'a>(values: impl Iterator<Item = &'a [u8]>) -> CoreResult<()> {
    let mut previous: Option<&[u8]> = None;
    for value in values {
        if previous.is_some_and(|prior| prior >= value) {
            return Err(CoreError::NonCanonicalOrdering);
        }
        previous = Some(value);
    }
    Ok(())
}

fn ordered_keys<'a>(values: impl Iterator<Item = &'a MetadataKey>) -> CoreResult<()> {
    let mut previous: Option<&MetadataKey> = None;
    for value in values {
        if previous.is_some_and(|prior| prior >= value) {
            return Err(CoreError::NonCanonicalOrdering);
        }
        previous = Some(value);
    }
    Ok(())
}

fn verify_subtree_bytes(node: &DirectoryNodeV1, expected: u64) -> CoreResult<()> {
    if let DirectoryNodeV1::Leaf { entries, .. } = node {
        let actual = entries.iter().try_fold(0_u64, |sum, (name, _)| {
            sum.checked_add(34 + name.as_bytes().len() as u64)
                .ok_or(CoreError::LengthOverflow)
        })?;
        if actual != expected {
            return Err(CoreError::LengthMismatch { expected, actual });
        }
    }
    Ok(())
}
