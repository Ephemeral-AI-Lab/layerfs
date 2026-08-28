use crate::driver::{DriverError, NativeMetadata, Result, MAX_NATIVE_XATTR_BYTES};
use std::fs::File;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

const EXCLUDED: &[&[u8]] = &[
    b"com.apple.decmpfs",
    b"com.apple.quarantine",
    b"com.apple.macl",
    b"com.apple.rootless",
    b"com.layerfs.projection-owner-v1",
];
const ENVIRONMENTAL: &[&[u8]] = &[b"com.apple.provenance"];

fn is_environmental(name: &[u8]) -> bool {
    ENVIRONMENTAL.contains(&name)
}

fn is_unsupported(name: &[u8]) -> bool {
    EXCLUDED.contains(&name)
}

pub fn read(file: &File) -> Result<NativeMetadata> {
    let metadata = file.metadata()?;
    let nanos = u32::try_from(metadata.mtime_nsec()).map_err(|_| DriverError::Unsupported)?;
    let native_mode = metadata.permissions().mode() & 0o7777;
    let mode = if metadata.file_type().is_symlink() {
        0o777
    } else if metadata.is_dir() {
        if native_mode & !0o1777 != 0 {
            return Err(DriverError::Unsupported);
        }
        native_mode
    } else {
        if native_mode & !0o777 != 0 {
            return Err(DriverError::Unsupported);
        }
        native_mode
    };
    let mut names = super::ffi::list_xattrs_file(file)?;
    names.sort();
    let mut xattrs = crate::driver::NativeXattrs::new();
    let mut xattr_bytes = 0_usize;
    for name in names.iter() {
        if is_environmental(name) {
            continue;
        }
        if name.len() > 127 || is_unsupported(name) {
            return Err(DriverError::Unsupported);
        }
        let value = super::ffi::get_xattr_file(file, name)?;
        xattr_bytes = account_xattr_bytes(xattr_bytes, name.len(), value.len())?;
        xattrs.push(name, &value)?;
    }
    let bsd_flags = super::ffi::flags_file(file)?;
    if bsd_flags & !0x0000_800f != 0 {
        return Err(DriverError::Unsupported);
    }
    Ok(NativeMetadata {
        mode,
        mtime_seconds: metadata.mtime(),
        mtime_nanoseconds: nanos,
        xattrs,
        acl: super::ffi::acl_file(file)?,
        bsd_flags,
    })
}

fn account_xattr_bytes(current: usize, name: usize, value: usize) -> Result<usize> {
    current
        .checked_add(name)
        .and_then(|total| total.checked_add(value))
        .filter(|total| *total <= MAX_NATIVE_XATTR_BYTES)
        .ok_or(DriverError::Unsupported)
}

pub fn write(file: &File, metadata: &NativeMetadata) -> Result<()> {
    let current = preflight_inner(file, metadata)?;
    super::ffi::set_acl_file(file, metadata.acl.as_deref())?;
    let mut expected = metadata.xattrs.names().peekable();
    for name in current.iter() {
        if is_environmental(name) {
            continue;
        }
        if is_unsupported(name) {
            return Err(DriverError::Unsupported);
        }
        while expected
            .peek()
            .is_some_and(|candidate| candidate.as_slice() < name)
        {
            expected.next();
        }
        if expected
            .peek()
            .is_some_and(|candidate| candidate.as_slice() == name)
        {
            expected.next();
        } else {
            super::ffi::remove_xattr_file(file, name)?;
        }
    }
    for (name, value) in metadata.xattrs.iter() {
        if name.len() > 127 || is_environmental(&name) || is_unsupported(&name) {
            return Err(DriverError::Unsupported);
        }
        super::ffi::set_xattr_file(file, &name, &value)?;
    }
    let actual = read(file)?;
    if actual.xattrs != metadata.xattrs || actual.acl != metadata.acl {
        return Err(DriverError::Conflict);
    }
    Ok(())
}

pub fn preflight(file: &File, metadata: &NativeMetadata) -> Result<()> {
    preflight_inner(file, metadata).map(drop)
}

pub fn preflight_symlink(metadata: &NativeMetadata) -> Result<()> {
    validate_requested(metadata, 0o777, true)
}

fn preflight_inner(file: &File, metadata: &NativeMetadata) -> Result<super::ffi::XattrNames> {
    let native = file.metadata()?;
    let (mask, exact_mode) = if native.file_type().is_symlink() {
        (0o777, true)
    } else if native.is_dir() {
        (0o1777, false)
    } else {
        (0o777, false)
    };
    validate_requested(metadata, mask, exact_mode)?;
    let mut current = super::ffi::list_xattrs_file(file)?;
    for name in current.iter() {
        if !is_environmental(name) && (name.len() > 127 || is_unsupported(name)) {
            return Err(DriverError::Unsupported);
        }
    }
    current.sort();
    Ok(current)
}

fn validate_requested(metadata: &NativeMetadata, mask: u32, exact_mode: bool) -> Result<()> {
    if (exact_mode && metadata.mode != mask)
        || metadata.mode & !mask != 0
        || metadata.mtime_nanoseconds > 999_999_999
        || metadata.bsd_flags & !0x0000_800f != 0
    {
        return Err(DriverError::Unsupported);
    }
    super::ffi::validate_acl(metadata.acl.as_deref())?;
    let mut total = 0_usize;
    for (name, value) in metadata.xattrs.iter() {
        if name.len() > 127 || is_environmental(&name) || is_unsupported(&name) {
            return Err(DriverError::Unsupported);
        }
        total = account_xattr_bytes(total, name.len(), value.len())?;
    }
    Ok(())
}

pub fn finish(file: &File, expected: &NativeMetadata) -> Result<()> {
    super::ffi::set_flags_file(file, expected.bsd_flags)?;
    verify(file, expected)
}

pub fn verify_before_install(file: &File, expected: &NativeMetadata) -> Result<()> {
    let actual = read(file)?;
    let mut expected = expected.clone();
    expected.bsd_flags = 0;
    if actual != expected {
        return Err(DriverError::Conflict);
    }
    Ok(())
}

pub fn verify(file: &File, expected: &NativeMetadata) -> Result<()> {
    if read(file)? != *expected {
        return Err(DriverError::Conflict);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
