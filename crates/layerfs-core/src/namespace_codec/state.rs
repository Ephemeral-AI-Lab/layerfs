pub fn encode_directory_state(value: DirectoryStateV1) -> CoreResult<Vec<u8>> {
    if value.profile_id != profile_id() || value.tree_level > 31 {
        return Err(CoreError::ProfileMismatch);
    }
    let mut bytes = Vec::with_capacity(84);
    bytes.extend_from_slice(b"LFS4DIR\0");
    bytes.extend_from_slice(&VERSION.to_be_bytes());
    bytes.extend_from_slice(&[3, 0]);
    bytes.extend_from_slice(&value.entry_count.to_be_bytes());
    bytes.push(value.tree_level);
    bytes.extend_from_slice(value.profile_id.as_bytes());
    bytes.extend_from_slice(value.mapping_root.as_bytes());
    encode_bytes_object(&bytes)
}

pub fn decode_directory_state(canonical: &[u8]) -> CoreResult<DirectoryStateV1> {
    let bytes = exact_value(canonical, b"LFS4DIR\0", 85, 3)?;
    let value = DirectoryStateV1 {
        entry_count: u64::from_be_bytes(bytes[12..20].try_into().unwrap()),
        tree_level: bytes[20],
        profile_id: ObjectId::from_bytes(&bytes[21..53])?,
        mapping_root: ObjectId::from_bytes(&bytes[53..85])?,
    };
    if value.profile_id != profile_id() {
        return Err(CoreError::ProfileMismatch);
    }
    if value.tree_level > 31 {
        return Err(CoreError::MappingDepthExceeded);
    }
    Ok(value)
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

pub fn encode_symlink(value: &SymlinkStateV1) -> CoreResult<Vec<u8>> {
    let value = SymlinkStateV1::new(value.target.clone())?;
    let mut bytes = Vec::with_capacity(14 + value.target.len());
    bytes.extend_from_slice(b"LFS4LNK\0");
    bytes.extend_from_slice(&VERSION.to_be_bytes());
    bytes.extend_from_slice(&[5, 0]);
    bytes.extend_from_slice(&(value.target.len() as u16).to_be_bytes());
    bytes.extend_from_slice(&value.target);
    encode_bytes_object(&bytes)
}

pub fn decode_symlink(canonical: &[u8]) -> CoreResult<SymlinkStateV1> {
    let bytes = decode_bytes_object(canonical)?;
    if bytes.len() < 14 {
        return Err(CoreError::UnexpectedEof);
    }
    header(bytes, b"LFS4LNK\0", 5)?;
    let len = usize::from(u16::from_be_bytes([bytes[12], bytes[13]]));
    if bytes.len() < 14 + len {
        return Err(CoreError::UnexpectedEof);
    }
    if bytes.len() > 14 + len {
        return Err(CoreError::TrailingBytes);
    }
    SymlinkStateV1::new(bytes[14..].to_vec())
}

pub fn encode_namespace_root(value: NamespaceRootV1) -> CoreResult<Vec<u8>> {
    if value.profile_id != profile_id() {
        return Err(CoreError::ProfileMismatch);
    }
    let mut bytes = Vec::with_capacity(108);
    bytes.extend_from_slice(b"LFS4FSR\0");
    bytes.extend_from_slice(&VERSION.to_be_bytes());
    bytes.extend_from_slice(&[6, 0]);
    bytes.extend_from_slice(value.profile_id.as_bytes());
    bytes.extend_from_slice(value.root_directory_inode.as_bytes());
    bytes.extend_from_slice(value.inode_table_root.as_bytes());
    encode_bytes_object(&bytes)
}

pub fn decode_namespace_root(canonical: &[u8]) -> CoreResult<NamespaceRootV1> {
    let bytes = exact_value(canonical, b"LFS4FSR\0", 108, 6)?;
    let value = NamespaceRootV1 {
        profile_id: ObjectId::from_bytes(&bytes[12..44])?,
        root_directory_inode: InodeId::from_slice(&bytes[44..76])?,
        inode_table_root: ObjectId::from_bytes(&bytes[76..108])?,
    };
    if value.profile_id != profile_id() {
        return Err(CoreError::ProfileMismatch);
    }
    Ok(value)
}

pub fn profile_id() -> ObjectId {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"layerfs/namespace-profile/bplus/v1\0");
    bytes.extend_from_slice(&1_u16.to_be_bytes());
    bytes.extend_from_slice(&8192_u32.to_be_bytes());
    for value in [13_u16, 31, 2, 5] {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    bytes.push(31);
    for value in [255_u16, 4096, 64, 255, 127, 64, 127, 128, 64, 64, 2] {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    bytes.extend_from_slice(&[
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 1, 2, 3, 1, 1, 1, 2, 4, 8, 8, 32, 32, 1,
    ]);
    ObjectId::from_bytes(blake3::hash(&bytes).as_bytes()).expect("BLAKE3 digest width")
}
