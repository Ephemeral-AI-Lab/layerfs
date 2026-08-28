use super::*;

pub(super) fn encode_entry(entry: &NativeEntry) -> VfsResult<Vec<u8>> {
    let token_len = u32::try_from(entry.token.len()).map_err(|_| VfsError::InvalidState)?;
    let hard_len = entry
        .hard_link_key
        .as_ref()
        .map(|key| u32::try_from(key.len()).map_err(|_| VfsError::InvalidState))
        .transpose()?;
    let mut bytes = Vec::with_capacity(
        17 + entry.token.len() + entry.hard_link_key.as_ref().map_or(0, Vec::len),
    );
    bytes.push(match entry.kind {
        NativeKind::Directory => 1,
        NativeKind::RegularFile => 2,
        NativeKind::Symlink => 3,
    });
    bytes.extend_from_slice(&entry.link_count.to_be_bytes());
    bytes.extend_from_slice(&token_len.to_be_bytes());
    bytes.extend_from_slice(&entry.token);
    bytes.extend_from_slice(&hard_len.unwrap_or(u32::MAX).to_be_bytes());
    if let Some(key) = &entry.hard_link_key {
        bytes.extend_from_slice(key);
    }
    Ok(bytes)
}

pub(super) fn decode_entry(name: Vec<u8>, bytes: &[u8]) -> VfsResult<NativeEntry> {
    if bytes.len() < 17 {
        return Err(VfsError::InvalidState);
    }
    let kind = match bytes[0] {
        1 => NativeKind::Directory,
        2 => NativeKind::RegularFile,
        3 => NativeKind::Symlink,
        _ => return Err(VfsError::InvalidState),
    };
    let link_count = u64::from_be_bytes(bytes[1..9].try_into().unwrap());
    let token_len = usize::try_from(u32::from_be_bytes(bytes[9..13].try_into().unwrap()))
        .map_err(|_| VfsError::InvalidState)?;
    let hard_offset = 13_usize
        .checked_add(token_len)
        .ok_or(VfsError::InvalidState)?;
    if hard_offset
        .checked_add(4)
        .filter(|end| *end <= bytes.len())
        .is_none()
    {
        return Err(VfsError::InvalidState);
    }
    let hard_len = u32::from_be_bytes(bytes[hard_offset..hard_offset + 4].try_into().unwrap());
    let hard_link_key = if hard_len == u32::MAX {
        if bytes.len() != hard_offset + 4 {
            return Err(VfsError::InvalidState);
        }
        None
    } else {
        let end = (hard_offset + 4)
            .checked_add(usize::try_from(hard_len).map_err(|_| VfsError::InvalidState)?)
            .ok_or(VfsError::InvalidState)?;
        if end != bytes.len() {
            return Err(VfsError::InvalidState);
        }
        Some(bytes[hard_offset + 4..end].to_vec())
    };
    Ok(NativeEntry {
        name,
        kind,
        token: bytes[13..hard_offset].to_vec(),
        hard_link_key,
        link_count,
    })
}
