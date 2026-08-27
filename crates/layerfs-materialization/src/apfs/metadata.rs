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
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SERIAL: AtomicU64 = AtomicU64::new(0);

    fn test_file() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "layerfs-apple-metadata-{}-{}",
            std::process::id(),
            TEST_SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        File::create(&path).unwrap();
        path
    }

    #[test]
    fn only_exact_provenance_is_environmental() {
        assert!(is_environmental(b"com.apple.provenance"));
        assert!(!is_environmental(b"com.apple.provenance.extra"));
        assert!(!is_environmental(b"com.apple.layerfs-test"));
        for name in EXCLUDED {
            assert!(!is_environmental(name));
            assert!(is_unsupported(name));
        }
        assert_eq!(
            EXCLUDED,
            &[
                b"com.apple.decmpfs".as_slice(),
                b"com.apple.quarantine".as_slice(),
                b"com.apple.macl".as_slice(),
                b"com.apple.rootless".as_slice(),
                b"com.layerfs.projection-owner-v1".as_slice(),
            ]
        );
    }

    #[test]
    fn provenance_value_is_not_canonical_input_and_exclusions_stay_fatal() {
        let path = test_file();
        let file = File::open(&path).unwrap();
        assert!(super::super::ffi::list_xattrs_file(&file)
            .unwrap()
            .iter()
            .any(is_environmental));
        let before = read(&file).unwrap();
        let _ = super::super::ffi::set_xattr_file(&file, b"com.apple.provenance", &[9, 8, 7]);
        let after = read(&file).unwrap();
        assert_eq!(after, before);
        assert!(!after.xattrs.iter().any(|(name, _)| is_environmental(&name)));

        for name in EXCLUDED {
            let mut invalid = before.clone();
            invalid.xattrs.push(name, b"forbidden").unwrap();
            assert!(matches!(
                write(&file, &invalid),
                Err(DriverError::Unsupported)
            ));
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn provenance_is_filtered_while_supported_xattrs_round_trip() {
        let path = test_file();
        let supported = b"com.layerfs.test-user-xattr";
        let file = File::open(&path).unwrap();
        super::super::ffi::set_xattr_file(&file, supported, b"exact-value").unwrap();
        let first = read(&file).unwrap();
        assert!(!first
            .xattrs
            .iter()
            .any(|(name, _)| name == b"com.apple.provenance"));
        assert!(first
            .xattrs
            .iter()
            .any(|(name, value)| name == supported && value == b"exact-value"));

        write(&file, &first).unwrap();
        let second = read(&file).unwrap();
        assert_eq!(second.xattrs, first.xattrs);
        assert!(!second
            .xattrs
            .iter()
            .any(|(name, _)| name == b"com.apple.provenance"));

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn refusal_happens_before_any_metadata_mutation() {
        let path = test_file();
        let file = File::open(&path).unwrap();
        let supported = b"com.layerfs.preflight";
        super::super::ffi::set_xattr_file(&file, supported, b"before").unwrap();
        let before = read(&file).unwrap();
        let mut refused = before.clone();
        refused.mode = 0o600;
        let mut xattrs = before.xattrs.iter().collect::<Vec<_>>();
        xattrs.push((b"com.apple.quarantine".to_vec(), b"blocked".to_vec()));
        xattrs.sort_by(|left, right| left.0.cmp(&right.0));
        refused.xattrs = crate::driver::NativeXattrs::from_entries(xattrs).unwrap();
        assert!(matches!(
            write(&file, &refused),
            Err(DriverError::Unsupported)
        ));
        assert_eq!(read(&file).unwrap(), before);
        assert_eq!(
            super::super::ffi::get_xattr_file(&file, supported).unwrap(),
            b"before"
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn ordered_extended_acl_round_trips_exactly() {
        let source = test_file();
        let target = test_file();
        assert!(Command::new("chmod")
            .args(["+a", "everyone deny delete"])
            .arg(&source)
            .status()
            .unwrap()
            .success());
        let source_file = File::open(&source).unwrap();
        let target_file = File::open(&target).unwrap();
        let expected = read(&source_file).unwrap();
        assert!(expected.acl.is_some());
        fs::set_permissions(&target, fs::Permissions::from_mode(expected.mode)).unwrap();
        File::open(&target)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new()
                    .set_modified(super::super::workspace::modified_time(&expected).unwrap()),
            )
            .unwrap();
        write(&target_file, &expected).unwrap();
        assert_eq!(read(&target_file).unwrap().acl, expected.acl);
        super::super::ffi::set_acl_file(&source_file, None).unwrap();
        super::super::ffi::set_acl_file(&target_file, None).unwrap();
        std::fs::remove_file(source).unwrap();
        std::fs::remove_file(target).unwrap();
    }

    #[test]
    fn setuid_and_setgid_are_rejected_before_mode_projection() {
        let path = test_file();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o6755)).unwrap();
        let file = File::open(&path).unwrap();
        assert!(matches!(read(&file), Err(DriverError::Unsupported)));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn metadata_read_stays_bound_to_the_open_descriptor_after_substitution() {
        let path = test_file();
        let moved = path.with_extension("moved");
        let file = File::open(&path).unwrap();
        let expected = read(&file).unwrap();
        fs::rename(&path, &moved).unwrap();
        let substitute = File::create(&path).unwrap();
        super::super::ffi::set_xattr_file(&substitute, b"com.layerfs.substitute", b"wrong")
            .unwrap();
        assert_eq!(read(&file).unwrap(), expected);
        assert_ne!(read(&substitute).unwrap(), expected);
        fs::remove_file(path).unwrap();
        fs::remove_file(moved).unwrap();
    }

    #[test]
    fn many_xattrs_have_one_aggregate_memory_ceiling() {
        let mut total = 0;
        for _ in 0..1024 {
            total = account_xattr_bytes(total, 16, 1008).unwrap();
        }
        assert_eq!(total, MAX_NATIVE_XATTR_BYTES);
        assert!(matches!(
            account_xattr_bytes(total, 1, 0),
            Err(DriverError::Unsupported)
        ));
    }
}
