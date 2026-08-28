use super::*;

pub(super) fn dir(handle: &dyn DirectoryHandle) -> Result<&Dir> {
    handle
        .as_any()
        .downcast_ref::<Dir>()
        .ok_or(DriverError::Conflict)
}
pub(super) fn optional_token(parent: &File, name: &[u8]) -> Result<Option<Vec<u8>>> {
    match super::ffi::stable_token_at(parent, name) {
        Ok(token) => Ok(Some(token)),
        Err(error) if error.raw_os_error() == Some(libc::ENOENT) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[allow(clippy::boxed_local)]
pub(super) fn atomic_replace_temp(
    temp: Box<Temp>,
    parent: &Dir,
    name: &[u8],
    required_prior: Option<Option<&[u8]>>,
    directory_durability: DirectoryDurability,
    facts: &Recorder,
) -> Result<DirectoryDurability> {
    sync_file(&temp.file, facts, FileSyncOwner::ContentTemp)?;
    let requested = super::ffi::file_stable_token(&temp.file)?;
    let prior = optional_token(&parent.file, name)?;
    if required_prior.is_some_and(|expected| prior.as_deref() != expected) {
        return Err(DriverError::Conflict);
    }
    let replace_start = Instant::now();
    let replaced = match super::ffi::replace_at(&temp.staging, &temp.name, &parent.file, name) {
        Ok(()) => (|| {
            if optional_token(&parent.file, name)?.as_deref() != Some(requested.as_slice())
                || optional_token(&temp.staging, &temp.name)?.is_some()
            {
                Err(DriverError::VisibilityAmbiguous)
            } else {
                Ok(())
            }
        })(),
        Err(error) => reconcile_replace(&parent.file, name, prior.clone(), &requested, error),
    };
    finish_replace(facts, replace_start, prior.is_some(), &replaced);
    replaced?;
    let entry = super::ffi::open_entry_at(&parent.file, name)?;
    let expected = temp
        .expected_metadata
        .lock()
        .map_err(|_| DriverError::Conflict)?
        .clone()
        .ok_or(DriverError::Conflict)?;
    let restrictive_flags = expected.bsd_flags != 0;
    let finalized = observed_call(
        facts,
        |facts| &mut facts.metadata_postinstall_verify,
        || {
            if restrictive_flags {
                super::metadata::finish(&entry, &expected)
            } else {
                super::metadata::verify(&entry, &expected)
            }
        },
    );
    if finalized.is_err() {
        return Err(DriverError::VisibilityAmbiguous);
    }
    if restrictive_flags && sync_file(&entry, facts, FileSyncOwner::PostHardLink).is_err() {
        return Err(DriverError::VisibilityAmbiguous);
    }
    if directory_durability == DirectoryDurability::DeferredToIncompleteTreeBoundary {
        return Ok(DirectoryDurability::DeferredToIncompleteTreeBoundary);
    }
    let outcome =
        match sync_directory_file_io(&parent.file, facts, DirectorySyncOwner::InstallParent) {
            Ok(()) => Ok(DirectoryDurability::ImmediateDirectoryDurability),
            Err(error) => reconcile_replace(&parent.file, name, prior, &requested, error)
                .map(|()| DirectoryDurability::ImmediateDirectoryDurability),
        };
    record_replace_durability_ambiguity(facts, &outcome);
    outcome
}

pub(super) fn validate_expected(file: &File, expected: Option<&[u8]>) -> Result<()> {
    if let Some(expected) = expected {
        if super::ffi::file_token(file)? != expected {
            return Err(DriverError::Conflict);
        }
    }
    Ok(())
}

pub(super) fn validate_entry_expected(
    parent: &File,
    name: &[u8],
    expected: Option<&[u8]>,
) -> Result<()> {
    if let Some(expected) = expected {
        if super::ffi::token_at(parent, name)? != expected {
            return Err(DriverError::Conflict);
        }
    }
    Ok(())
}

pub(super) fn reconcile_replace(
    parent: &File,
    name: &[u8],
    prior: Option<Vec<u8>>,
    requested: &[u8],
    error: std::io::Error,
) -> Result<()> {
    match optional_token(parent, name)? {
        Some(actual) if actual == requested => Err(DriverError::DurabilityAmbiguous),
        actual if actual == prior => Err(DriverError::Io(error)),
        _ => Err(DriverError::VisibilityAmbiguous),
    }
}

pub(super) fn reconcile_rename(
    source_parent: &File,
    source: &[u8],
    target_parent: &File,
    target: &[u8],
    requested: &[u8],
    error: std::io::Error,
) -> Result<()> {
    let source = optional_token(source_parent, source)?;
    let target = optional_token(target_parent, target)?;
    if source.is_none() && target.as_deref() == Some(requested) {
        Err(DriverError::DurabilityAmbiguous)
    } else if source.as_deref() == Some(requested) && target.is_none() {
        Err(DriverError::Io(error))
    } else {
        Err(DriverError::VisibilityAmbiguous)
    }
}

pub(super) fn remove_entry(
    parent: &File,
    name: &[u8],
    expected: &[u8],
    directory: bool,
    facts: &Recorder,
) -> Result<()> {
    if super::ffi::stable_token_at(parent, name)? != expected {
        return Err(DriverError::Conflict);
    }
    let removed = if directory {
        super::ffi::remove_directory_if_identity_at(parent, name, expected)
    } else {
        super::ffi::unlink_if_identity_at(parent, name, expected)
    };
    if let Err(error) = removed {
        return reconcile_remove(parent, name, expected, error);
    }
    match sync_directory_file_io(parent, facts, DirectorySyncOwner::InstallParent) {
        Ok(()) => Ok(()),
        Err(error) => reconcile_remove(parent, name, expected, error),
    }
}

pub(super) fn reconcile_remove(
    parent: &File,
    name: &[u8],
    expected: &[u8],
    error: std::io::Error,
) -> Result<()> {
    match optional_token(parent, name)? {
        None => Err(DriverError::DurabilityAmbiguous),
        Some(actual) if actual == expected => Err(DriverError::Io(error)),
        Some(_) => Err(DriverError::VisibilityAmbiguous),
    }
}

pub(in crate::apfs) fn modified_time(metadata: &NativeMetadata) -> Result<SystemTime> {
    let seconds = metadata.mtime_seconds;
    let nanos = metadata.mtime_nanoseconds;
    if nanos >= 1_000_000_000 {
        return Err(DriverError::Unsupported);
    }
    let time = if seconds >= 0 {
        UNIX_EPOCH.checked_add(Duration::new(seconds as u64, nanos))
    } else if nanos == 0 {
        UNIX_EPOCH.checked_sub(Duration::new(seconds.unsigned_abs(), 0))
    } else {
        UNIX_EPOCH.checked_sub(Duration::new(
            seconds.unsigned_abs() - 1,
            1_000_000_000 - nanos,
        ))
    };
    time.ok_or(DriverError::Unsupported)
}
