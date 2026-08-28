use super::super::session_legacy::{VfsError, VfsResult};
use layerfs_core::metadata::decode_apple_acl;
use layerfs_materialization::driver::*;
use std::io::{Read, SeekFrom, Write};

const MAX_NATIVE_ACL_BYTES: usize = 4_620;
pub(super) const MAX_SPOOLED_METADATA_BYTES: u64 =
    36 + MAX_NATIVE_ACL_BYTES as u64 + 7 * MAX_NATIVE_XATTR_BYTES as u64;

pub(crate) fn write_spooled_metadata<W: Write + ?Sized>(
    metadata: &NativeMetadata,
    spool: &mut W,
) -> VfsResult<u64> {
    let (len, acl_len, count) = spooled_metadata_layout(metadata)?;

    spool.write_all(b"LFSMETA1")?;
    spool.write_all(&metadata.mode.to_be_bytes())?;
    spool.write_all(&metadata.mtime_seconds.to_be_bytes())?;
    spool.write_all(&metadata.mtime_nanoseconds.to_be_bytes())?;
    spool.write_all(&metadata.bsd_flags.to_be_bytes())?;
    spool.write_all(&acl_len.to_be_bytes())?;
    spool.write_all(&count.to_be_bytes())?;
    if let Some(acl) = &metadata.acl {
        spool.write_all(acl)?;
    }
    for (name, value) in &metadata.xattrs {
        spool.write_all(&(name.len() as u16).to_be_bytes())?;
        spool.write_all(&(value.len() as u32).to_be_bytes())?;
        spool.write_all(&name)?;
        spool.write_all(&value)?;
    }
    Ok(len)
}

pub(crate) fn spooled_metadata_len(metadata: &NativeMetadata) -> VfsResult<u64> {
    spooled_metadata_layout(metadata).map(|layout| layout.0)
}

fn spooled_metadata_layout(metadata: &NativeMetadata) -> VfsResult<(u64, u32, u32)> {
    let acl_len = metadata
        .acl
        .as_ref()
        .map(|acl| {
            decode_apple_acl(acl)?;
            u32::try_from(acl.len()).map_err(|_| VfsError::InvalidState)
        })
        .transpose()?
        .unwrap_or(u32::MAX);
    if metadata.xattrs.len() > MAX_NATIVE_XATTR_BYTES {
        return Err(VfsError::InvalidState);
    }
    let count = u32::try_from(metadata.xattrs.len()).map_err(|_| VfsError::InvalidState)?;
    let mut xattr_bytes = 0_usize;
    let mut len = 36_u64
        .checked_add(if acl_len == u32::MAX {
            0
        } else {
            u64::from(acl_len)
        })
        .ok_or(VfsError::InvalidState)?;
    for (name, value) in &metadata.xattrs {
        if name.is_empty() || name.len() > 127 || name.contains(&0) {
            return Err(VfsError::InvalidState);
        }
        let name_len = u16::try_from(name.len()).map_err(|_| VfsError::InvalidState)?;
        let value_len = u32::try_from(value.len()).map_err(|_| VfsError::InvalidState)?;
        xattr_bytes = xattr_bytes
            .checked_add(name.len())
            .and_then(|total| total.checked_add(value.len()))
            .filter(|total| *total <= MAX_NATIVE_XATTR_BYTES)
            .ok_or(VfsError::InvalidState)?;
        len = len
            .checked_add(6)
            .and_then(|total| total.checked_add(u64::from(name_len)))
            .and_then(|total| total.checked_add(u64::from(value_len)))
            .ok_or(VfsError::InvalidState)?;
    }
    if len > MAX_SPOOLED_METADATA_BYTES {
        return Err(VfsError::InvalidState);
    }

    Ok((len, acl_len, count))
}

pub(super) fn load_spooled_metadata(
    spool: &mut dyn OwnedTempHandle,
    offset: u64,
    len: u64,
) -> VfsResult<NativeMetadata> {
    if len > MAX_SPOOLED_METADATA_BYTES {
        return Err(VfsError::InvalidState);
    }
    spool.seek(SeekFrom::Start(offset))?;
    let mut input = spool.take(len);
    let mut magic = [0_u8; 8];
    input.read_exact(&mut magic)?;
    if &magic != b"LFSMETA1" {
        return Err(VfsError::InvalidState);
    }
    let mode = read_u32(&mut input)?;
    let mtime_seconds = read_i64(&mut input)?;
    let mtime_nanoseconds = read_u32(&mut input)?;
    let bsd_flags = read_u32(&mut input)?;
    let acl_len = read_u32(&mut input)?;
    let count = read_u32(&mut input)?;
    if count as usize > MAX_NATIVE_XATTR_BYTES {
        return Err(VfsError::InvalidState);
    }
    let acl = if acl_len == u32::MAX {
        None
    } else {
        let acl = read_vec(&mut input, acl_len as usize, MAX_NATIVE_ACL_BYTES)?;
        decode_apple_acl(&acl)?;
        Some(acl)
    };
    let mut xattrs = layerfs_materialization::driver::NativeXattrs::new();
    let mut xattr_bytes = 0_usize;
    for _ in 0..count {
        let name_len = read_u16(&mut input)? as usize;
        let value_len =
            usize::try_from(read_u32(&mut input)?).map_err(|_| VfsError::InvalidState)?;
        xattr_bytes = xattr_bytes
            .checked_add(name_len)
            .and_then(|total| total.checked_add(value_len))
            .filter(|total| *total <= MAX_NATIVE_XATTR_BYTES)
            .ok_or(VfsError::InvalidState)?;
        let name = read_vec(&mut input, name_len, 127)?;
        if name.is_empty() || name.contains(&0) {
            return Err(VfsError::InvalidState);
        }
        let value = read_vec(&mut input, value_len, MAX_NATIVE_XATTR_BYTES)?;
        xattrs
            .push(&name, &value)
            .map_err(|_| VfsError::InvalidState)?;
    }
    if input.limit() != 0 {
        return Err(VfsError::InvalidState);
    }
    Ok(NativeMetadata {
        mode,
        mtime_seconds,
        mtime_nanoseconds,
        xattrs,
        acl,
        bsd_flags,
    })
}

fn read_u16(input: &mut dyn Read) -> VfsResult<u16> {
    let mut bytes = [0_u8; 2];
    input.read_exact(&mut bytes)?;
    Ok(u16::from_be_bytes(bytes))
}

fn read_u32(input: &mut dyn Read) -> VfsResult<u32> {
    let mut bytes = [0_u8; 4];
    input.read_exact(&mut bytes)?;
    Ok(u32::from_be_bytes(bytes))
}

fn read_i64(input: &mut dyn Read) -> VfsResult<i64> {
    let mut bytes = [0_u8; 8];
    input.read_exact(&mut bytes)?;
    Ok(i64::from_be_bytes(bytes))
}

fn read_vec(input: &mut dyn Read, len: usize, limit: usize) -> VfsResult<Vec<u8>> {
    if len > limit {
        return Err(VfsError::InvalidState);
    }
    let mut bytes = vec![0_u8; len];
    input.read_exact(&mut bytes)?;
    Ok(bytes)
}
