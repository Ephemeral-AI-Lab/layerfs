pub fn read_mapping(store: &impl ObjectRead, id: ObjectId) -> CoreResult<LegacyMapping> {
    store.with_authenticated_canonical(id, decode_mapping)
}

pub fn decode_mapping(canonical: &[u8]) -> CoreResult<LegacyMapping> {
    let payload = decode_bytes_object(canonical)?;
    if payload.len() < 11 {
        return Err(CoreError::UnexpectedEof);
    }
    if &payload[..8] != MAGIC {
        return Err(CoreError::InvalidMappingTag { tag: 0 });
    }
    let version = match u16::from_be_bytes([payload[8], payload[9]]) {
        1 => MappingVersion::V1,
        2 => MappingVersion::V2,
        version => return Err(CoreError::UnsupportedMappingVersion { version }),
    };
    let body = &payload[11..];
    match payload[10] {
        0x01 => Ok(LegacyMapping::FileRoot(version, decode_file_root(body)?)),
        0x02 => match version {
            MappingVersion::V1 => Ok(LegacyMapping::FileLeafV1(decode_leaf_v1(body)?)),
            MappingVersion::V2 => Ok(LegacyMapping::FileLeafV2(decode_leaf_v2(body)?)),
        },
        0x03 => {
            let (total, pages) = decode_directory_index(body)?;
            Ok(LegacyMapping::DirectoryIndex(version, total, pages))
        }
        0x04 if body.len() == 4 => Ok(LegacyMapping::DirectoryMetadata(
            version,
            u32::from_be_bytes(body.try_into().map_err(|_| CoreError::UnexpectedEof)?),
        )),
        0x04 => Err(if body.len() < 4 {
            CoreError::UnexpectedEof
        } else {
            CoreError::TrailingBytes
        }),
        0x05 => Ok(LegacyMapping::DeltaIndex(version, decode_transition(body)?)),
        0x06 => Ok(LegacyMapping::DeltaPage(version, decode_delta_page(body)?)),
        0x07 => {
            let (level, children) = decode_children(body, true)?;
            if level == 0 {
                return Err(CoreError::NonCanonicalPagePartition);
            }
            Ok(LegacyMapping::FileBranch(version, level, children))
        }
        tag => Err(CoreError::InvalidMappingTag { tag }),
    }
}

fn decode_leaf_v1(body: &[u8]) -> CoreResult<Vec<FileReferenceV1>> {
    let count = count(body)?;
    exact_len(body, 4, count, 68)?;
    body[4..]
        .chunks_exact(68)
        .map(|bytes| {
            Ok(FileReferenceV1 {
                raw_id: ObjectId::from_bytes(&bytes[..32])?,
                raw_length: bounded_chunk_length(&bytes[32..36])?,
                object_id: ObjectId::from_bytes(&bytes[36..])?,
            })
        })
        .collect()
}

fn decode_leaf_v2(body: &[u8]) -> CoreResult<Vec<FileReferenceV2>> {
    let count = count(body)?;
    exact_len(body, 4, count, 36)?;
    body[4..]
        .chunks_exact(36)
        .map(|bytes| {
            Ok(FileReferenceV2 {
                raw_length: bounded_chunk_length(&bytes[..4])?,
                object_id: ObjectId::from_bytes(&bytes[4..])?,
            })
        })
        .collect()
}

fn bounded_chunk_length(bytes: &[u8]) -> CoreResult<u32> {
    let length = u32::from_be_bytes(bytes.try_into().map_err(|_| CoreError::UnexpectedEof)?);
    if usize::try_from(length).map_err(|_| CoreError::LengthOverflow)?
        > crate::cdc::MAXIMUM_CHUNK_BYTES
    {
        return Err(CoreError::ObjectLimitExceeded);
    }
    Ok(length)
}

fn decode_file_root(body: &[u8]) -> CoreResult<FileRoot> {
    if body.len() < 25 {
        return Err(CoreError::UnexpectedEof);
    }
    let mode = u32::from_be_bytes(body[..4].try_into().unwrap());
    let total_raw = u64::from_be_bytes(body[4..12].try_into().unwrap());
    let reference_count = u64::from_be_bytes(body[12..20].try_into().unwrap());
    let level = body[20];
    let (_, children) = decode_children(&body[20..], true)?;
    if reference_count == 0 {
        if total_raw != 0 || level != 0 || !children.is_empty() {
            return Err(CoreError::NonCanonicalPagePartition);
        }
    } else if children.is_empty()
        || children.last().map(|child| child.cumulative_end) != Some(total_raw)
        || expected_level(reference_count)? != level
    {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    Ok(FileRoot {
        mode,
        total_raw,
        reference_count,
        level,
        children,
    })
}

fn decode_children(body: &[u8], with_level: bool) -> CoreResult<(u8, Vec<FileChild>)> {
    let header = if with_level { 5 } else { 4 };
    if body.len() < header {
        return Err(CoreError::UnexpectedEof);
    }
    let level = if with_level { body[0] } else { 0 };
    let offset = usize::from(with_level);
    let count = usize::try_from(u32::from_be_bytes(
        body[offset..offset + 4].try_into().unwrap(),
    ))
    .map_err(|_| CoreError::LengthOverflow)?;
    if count > FILE_FANOUT {
        return Err(CoreError::ObjectLimitExceeded);
    }
    exact_len(body, header, count, DESCRIPTOR_BYTES)?;
    let children = body[header..]
        .chunks_exact(DESCRIPTOR_BYTES)
        .map(|bytes| {
            Ok(FileChild {
                cumulative_end: u64::from_be_bytes(bytes[..8].try_into().unwrap()),
                object_id: ObjectId::from_bytes(&bytes[8..])?,
            })
        })
        .collect::<CoreResult<Vec<_>>>()?;
    if children
        .windows(2)
        .any(|pair| pair[0].cumulative_end >= pair[1].cumulative_end)
    {
        return Err(CoreError::NonCanonicalOrdering);
    }
    Ok((level, children))
}

fn decode_directory_index(body: &[u8]) -> CoreResult<(u32, Vec<DirectoryPageRef>)> {
    if body.len() < 8 {
        return Err(CoreError::UnexpectedEof);
    }
    let total = u32::from_be_bytes(body[..4].try_into().unwrap());
    let count = usize::try_from(u32::from_be_bytes(body[4..8].try_into().unwrap()))
        .map_err(|_| CoreError::LengthOverflow)?;
    if count > MAX_ENTRIES {
        return Err(CoreError::ObjectLimitExceeded);
    }
    let mut offset = 8_usize;
    let mut pages = Vec::with_capacity(count.min(1024));
    let mut observed = 0_u64;
    for _ in 0..count {
        let fixed = offset.checked_add(6).ok_or(CoreError::LengthOverflow)?;
        if fixed > body.len() {
            return Err(CoreError::UnexpectedEof);
        }
        let page_count = u32::from_be_bytes(body[offset..offset + 4].try_into().unwrap());
        let name_len = usize::from(u16::from_be_bytes(
            body[offset + 4..fixed].try_into().unwrap(),
        ));
        offset = fixed;
        let end = offset
            .checked_add(name_len)
            .and_then(|value| value.checked_add(32))
            .ok_or(CoreError::LengthOverflow)?;
        if end > body.len() || page_count == 0 {
            return Err(CoreError::UnexpectedEof);
        }
        pages.push(DirectoryPageRef {
            count: page_count,
            first_name: CanonicalName::from_bytes(&body[offset..offset + name_len])?,
            object_id: ObjectId::from_bytes(&body[offset + name_len..end])?,
        });
        observed = observed
            .checked_add(u64::from(page_count))
            .ok_or(CoreError::LengthOverflow)?;
        offset = end;
    }
    if offset != body.len() || observed != u64::from(total) {
        return Err(if offset < body.len() {
            CoreError::TrailingBytes
        } else {
            CoreError::LengthMismatch {
                expected: u64::from(total),
                actual: observed,
            }
        });
    }
    if pages
        .windows(2)
        .any(|pair| pair[0].first_name >= pair[1].first_name)
    {
        return Err(CoreError::NonCanonicalOrdering);
    }
    Ok((total, pages))
}
