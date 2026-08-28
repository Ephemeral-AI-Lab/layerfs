fn count(body: &[u8]) -> CoreResult<usize> {
    let bytes = body.get(..4).ok_or(CoreError::UnexpectedEof)?;
    let count = usize::try_from(u32::from_be_bytes(bytes.try_into().unwrap()))
        .map_err(|_| CoreError::LengthOverflow)?;
    if count > MAX_ENTRIES {
        return Err(CoreError::ObjectLimitExceeded);
    }
    Ok(count)
}

fn exact_len(body: &[u8], header: usize, count: usize, width: usize) -> CoreResult<()> {
    let expected = header
        .checked_add(count.checked_mul(width).ok_or(CoreError::LengthOverflow)?)
        .ok_or(CoreError::LengthOverflow)?;
    if body.len() != expected {
        return Err(if body.len() < expected {
            CoreError::UnexpectedEof
        } else {
            CoreError::TrailingBytes
        });
    }
    Ok(())
}

fn expected_level(references: u64) -> CoreResult<u8> {
    let mut nodes = references.div_ceil(FILE_FANOUT as u64);
    let mut level = 0_u8;
    while nodes > FILE_FANOUT as u64 {
        nodes = nodes.div_ceil(FILE_FANOUT as u64);
        level = level
            .checked_add(1)
            .ok_or(CoreError::MappingDepthExceeded)?;
    }
    Ok(level)
}

fn take_id(body: &[u8], offset: &mut usize) -> CoreResult<ObjectId> {
    let end = offset.checked_add(32).ok_or(CoreError::LengthOverflow)?;
    let id = ObjectId::from_bytes(body.get(*offset..end).ok_or(CoreError::UnexpectedEof)?)?;
    *offset = end;
    Ok(id)
}

fn take_u32(body: &[u8], offset: &mut usize) -> CoreResult<u32> {
    let end = offset.checked_add(4).ok_or(CoreError::LengthOverflow)?;
    let value = u32::from_be_bytes(
        body.get(*offset..end)
            .ok_or(CoreError::UnexpectedEof)?
            .try_into()
            .unwrap(),
    );
    *offset = end;
    Ok(value)
}
