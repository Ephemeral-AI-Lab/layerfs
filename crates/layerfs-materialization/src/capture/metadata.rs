use super::*;

pub(crate) fn put_metadata_observed(
    publication: &mut impl CaptureStore,
    kind: InodeKind,
    native: &NativeMetadata,
    counters: &mut OperationCounters,
) -> VfsResult<layerfs_core::ObjectId> {
    spooled_metadata_len(native)?;
    let portable = PortableMetadataV1 {
        permission_mode: native.mode,
        mtime_seconds: native.mtime_seconds,
        mtime_nanoseconds: native.mtime_nanoseconds,
    };
    portable.validate(kind)?;
    let mut tree = MetadataTreeBuilder::new();
    if let Some(acl) = &native.acl {
        decode_apple_acl(acl)?;
        let entry = metadata_value(
            publication,
            MetadataKey::new("apple.acl".into(), Vec::new())?,
            acl,
            counters,
        )?;
        tree.push(publication, entry)?;
    }
    if let Some(flags) = encode_bsd_flags(native.bsd_flags)? {
        let entry = metadata_value(
            publication,
            MetadataKey::new("apple.bsd-flags".into(), Vec::new())?,
            &flags,
            counters,
        )?;
        tree.push(publication, entry)?;
    }
    for (name, value) in &native.xattrs {
        let entry = metadata_value(
            publication,
            MetadataKey::new("apple.xattr".into(), name)?,
            &value,
            counters,
        )?;
        tree.push(publication, entry)?;
    }
    let mode = portable.mode_bytes(kind)?;
    let entry = metadata_value(
        publication,
        MetadataKey::new("portable".into(), b"mode".to_vec())?,
        &mode,
        counters,
    )?;
    tree.push(publication, entry)?;
    let mtime = portable.mtime_bytes()?;
    let entry = metadata_value(
        publication,
        MetadataKey::new("portable".into(), b"mtime".to_vec())?,
        &mtime,
        counters,
    )?;
    tree.push(publication, entry)?;
    Ok(tree.finish(publication)?)
}

fn metadata_value(
    publication: &mut impl CaptureStore,
    key: MetadataKey,
    value: &[u8],
    counters: &mut OperationCounters,
) -> VfsResult<MetadataEntryV1> {
    let (root, rope) = build(publication, Cursor::new(value))?;
    counters.add_metadata_rope(rope)?;
    Ok(MetadataEntryV1 {
        key,
        value_file_root: root.0,
    })
}

pub(crate) fn spooled_metadata_len(metadata: &NativeMetadata) -> VfsResult<u64> {
    let acl = metadata
        .acl
        .as_deref()
        .map(|acl| {
            decode_apple_acl(acl)?;
            if acl.len() > 4_620 {
                return Err(VfsError::InvalidState);
            }
            Ok(acl.len() as u64)
        })
        .transpose()?
        .unwrap_or(0);
    let mut len = 36_u64.checked_add(acl).ok_or(VfsError::InvalidState)?;
    for (name, value) in &metadata.xattrs {
        len = len
            .checked_add(6)
            .and_then(|total| total.checked_add(name.len() as u64))
            .and_then(|total| total.checked_add(value.len() as u64))
            .ok_or(VfsError::InvalidState)?;
    }
    let maximum = 36_u64 + 4_620 + 7 * MAX_NATIVE_XATTR_BYTES as u64;
    if len > maximum {
        return Err(VfsError::InvalidState);
    }
    Ok(len)
}
