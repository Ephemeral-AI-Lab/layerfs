fn decode_transition(body: &[u8]) -> CoreResult<LegacyTransition> {
    if body.len() < 41 {
        return Err(CoreError::UnexpectedEof);
    }
    let mut offset = 1_usize;
    let parent = match body[0] {
        0 => None,
        1 => {
            let end = offset + 32;
            let id = ObjectId::from_bytes(body.get(offset..end).ok_or(CoreError::UnexpectedEof)?)?;
            offset = end;
            Some(id)
        }
        value => return Err(CoreError::InvalidMappingDiscriminator { value }),
    };
    let child_end = offset.checked_add(32).ok_or(CoreError::LengthOverflow)?;
    let child = ObjectId::from_bytes(
        body.get(offset..child_end)
            .ok_or(CoreError::UnexpectedEof)?,
    )?;
    offset = child_end;
    let fields_end = offset.checked_add(8).ok_or(CoreError::LengthOverflow)?;
    let fields = body
        .get(offset..fields_end)
        .ok_or(CoreError::UnexpectedEof)?;
    let entry_count = u32::from_be_bytes(fields[..4].try_into().unwrap());
    let page_count = usize::try_from(u32::from_be_bytes(fields[4..].try_into().unwrap()))
        .map_err(|_| CoreError::LengthOverflow)?;
    if page_count > MAX_ENTRIES
        || usize::try_from(entry_count).unwrap_or(MAX_ENTRIES + 1) > MAX_ENTRIES
    {
        return Err(CoreError::ObjectLimitExceeded);
    }
    if (entry_count == 0) != (page_count == 0)
        || parent.is_none() && (entry_count != 0 || page_count != 0)
    {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    offset = fields_end;
    exact_len(body, offset, page_count, 32)?;
    let pages = body[offset..]
        .chunks_exact(32)
        .map(ObjectId::from_bytes)
        .collect::<CoreResult<Vec<_>>>()?;
    Ok(LegacyTransition {
        parent,
        child,
        entry_count,
        pages,
    })
}

fn decode_delta_page(body: &[u8]) -> CoreResult<Vec<TransitionOperation>> {
    let count = count(body)?;
    if count == 0 || count > body.len().saturating_sub(4) {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    let mut offset = 4_usize;
    let mut entries = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        let kind = *body.get(offset).ok_or(CoreError::UnexpectedEof)?;
        offset += 1;
        let length_end = offset.checked_add(4).ok_or(CoreError::LengthOverflow)?;
        let path_len = usize::try_from(u32::from_be_bytes(
            body.get(offset..length_end)
                .ok_or(CoreError::UnexpectedEof)?
                .try_into()
                .unwrap(),
        ))
        .map_err(|_| CoreError::LengthOverflow)?;
        offset = length_end;
        let path_end = offset
            .checked_add(path_len)
            .ok_or(CoreError::LengthOverflow)?;
        let path =
            CanonicalPath::from_bytes(body.get(offset..path_end).ok_or(CoreError::UnexpectedEof)?)?;
        offset = path_end;
        let entry = match kind {
            1 => TransitionOperation::Add {
                path,
                after: take_id(body, &mut offset)?,
            },
            2 => TransitionOperation::Remove {
                path,
                before: take_id(body, &mut offset)?,
            },
            3 => TransitionOperation::Replace {
                path,
                before: take_id(body, &mut offset)?,
                after: take_id(body, &mut offset)?,
            },
            4 => TransitionOperation::Metadata {
                path,
                before: take_id(body, &mut offset)?,
                before_mode: take_u32(body, &mut offset)?,
                after: take_id(body, &mut offset)?,
                after_mode: take_u32(body, &mut offset)?,
            },
            value => return Err(CoreError::InvalidMappingDiscriminator { value }),
        };
        entries.push(entry);
    }
    if offset != body.len() {
        return Err(CoreError::TrailingBytes);
    }
    Ok(entries)
}
