//! The init_entry metadata blob: encoding and checked decoding.

/// The 32-byte init_entry metadata blob: the inline form's field, mode at
/// 0..4, mtime seconds at 4..12, nanoseconds at 12..16 and 16 zero bytes.
/// Rows are written and read only by the live operation owning them, so the
/// column's content stays private to one running binary.
pub(crate) fn metadata_blob(
    metadata: &layerfs_content::filesystem::attributes::PortableMetadata,
) -> [u8; 32] {
    let mut blob = [0_u8; 32];
    blob[0..4].copy_from_slice(&metadata.mode.to_be_bytes());
    blob[4..12].copy_from_slice(&metadata.mtime_seconds.to_be_bytes());
    blob[12..16].copy_from_slice(&metadata.mtime_nanoseconds.to_be_bytes());
    blob
}

/// Decodes one metadata blob, rejecting nonzero padding and values that fail
/// the portable grammar for the row's kind.
pub(crate) fn metadata_parse(
    blob: &[u8],
    kind: layerfs_content::inode_leaf::InodeKind,
) -> Result<
    layerfs_content::filesystem::attributes::PortableMetadata,
    crate::backend::records::BackendError,
> {
    if blob.len() != 32 || blob[16..].iter().any(|byte| *byte != 0) {
        return Err(crate::backend::records::BackendError::Integrity);
    }
    let metadata = layerfs_content::filesystem::attributes::PortableMetadata {
        mode: u32::from_be_bytes(
            blob[0..4]
                .try_into()
                .map_err(|_| crate::backend::records::BackendError::Integrity)?,
        ),
        mtime_seconds: i64::from_be_bytes(
            blob[4..12]
                .try_into()
                .map_err(|_| crate::backend::records::BackendError::Integrity)?,
        ),
        mtime_nanoseconds: u32::from_be_bytes(
            blob[12..16]
                .try_into()
                .map_err(|_| crate::backend::records::BackendError::Integrity)?,
        ),
    };
    metadata
        .validate(kind)
        .map_err(|_| crate::backend::records::BackendError::Integrity)?;
    Ok(metadata)
}
