use layerfs_bridge::contract::{Operation, Root};

pub fn fresh(length: u64) -> Operation {
    Operation::SaveFile {
        base: None,
        base_length: 0,
        length,
        extents: u64::from(length != 0),
        replacement: length,
    }
}

pub fn body(extents: &[(u64, u64, u64)], bytes: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(extents.len() * 24 + bytes.len());
    for (kind, offset, length) in extents {
        result.extend_from_slice(&kind.to_be_bytes());
        result.extend_from_slice(&offset.to_be_bytes());
        result.extend_from_slice(&length.to_be_bytes());
    }
    result.extend_from_slice(bytes);
    result
}

pub fn fresh_body(bytes: &[u8]) -> Vec<u8> {
    if bytes.is_empty() {
        Vec::new()
    } else {
        body(&[(1, 0, bytes.len() as u64)], bytes)
    }
}

pub fn existing(
    base: Root,
    base_length: u64,
    length: u64,
    extents: u64,
    replacement: u64,
) -> Operation {
    Operation::SaveFile {
        base: Some(base),
        base_length,
        length,
        extents,
        replacement,
    }
}
