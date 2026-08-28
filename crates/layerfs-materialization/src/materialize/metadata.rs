use super::*;

pub(crate) fn metadata(
    engine: &impl ObjectRead,
    root: ObjectId,
    counters: &mut OperationCounters,
) -> VfsResult<NativeMetadata> {
    let mut mode = None;
    let mut seconds = None;
    let mut nanos = None;
    let mut xattrs = crate::driver::NativeXattrs::new();
    let mut acl = None;
    let mut flags = 0;
    visit_metadata_entries(engine, root, |entries| {
        for entry in entries {
            let file_root = FileStateRoot(entry.value_file_root);
            let mut value = Vec::new();
            let rope = read_all_bounded(engine, file_root, 1024 * 1024, &mut value)?;
            counters.add_metadata_rope(rope)?;
            match (entry.key.domain.as_str(), entry.key.key.as_slice()) {
                ("portable", b"mode") if value.len() == 4 => {
                    mode = Some(u32::from_be_bytes(value.try_into().unwrap()))
                }
                ("portable", b"mtime") if value.len() == 12 => {
                    seconds = Some(i64::from_be_bytes(value[..8].try_into().unwrap()));
                    nanos = Some(u32::from_be_bytes(value[8..].try_into().unwrap()));
                }
                ("apple.xattr", name) => {
                    xattrs.push(name, &value).map_err(|_| {
                        layerfs_core::CoreError::InvalidRecord("metadata xattr bytes")
                    })?;
                }
                ("apple.acl", b"") => {
                    decode_apple_acl(&value)?;
                    acl = Some(value);
                }
                ("apple.bsd-flags", b"") if value.len() == 4 => {
                    flags = u32::from_be_bytes(value.try_into().unwrap());
                    if flags & !SUPPORTED_BSD_FLAGS != 0 {
                        return Err(layerfs_core::CoreError::InvalidRecord("BSD flags"));
                    }
                }
                _ => return Err(layerfs_core::CoreError::InvalidRecord("metadata value")),
            }
        }
        Ok(())
    })?;
    let metadata = NativeMetadata {
        mode: mode.ok_or(VfsError::InvalidState)?,
        mtime_seconds: seconds.ok_or(VfsError::InvalidState)?,
        mtime_nanoseconds: nanos.ok_or(VfsError::InvalidState)?,
        xattrs,
        acl,
        bsd_flags: flags,
    };
    crate::capture::spooled_metadata_len(&metadata)?;
    Ok(metadata)
}
