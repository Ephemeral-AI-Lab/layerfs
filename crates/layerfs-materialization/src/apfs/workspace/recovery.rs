use super::*;

pub(super) fn encode_recovery_record(
    store_id: [u8; 32],
    owned_root: bool,
    root_name: &[u8],
    root_identity: &[u8],
) -> Vec<u8> {
    let mut record =
        Vec::with_capacity(RECOVERY_MAGIC.len() + 37 + root_name.len() + root_identity.len());
    record.extend_from_slice(RECOVERY_MAGIC);
    record.extend_from_slice(&store_id);
    record.push(u8::from(owned_root));
    record.extend_from_slice(&(root_name.len() as u16).to_be_bytes());
    record.extend_from_slice(&(root_identity.len() as u16).to_be_bytes());
    record.extend_from_slice(root_name);
    record.extend_from_slice(root_identity);
    record
}

fn decode_recovery_record(
    record: &[u8],
    expected_store_id: [u8; 32],
) -> Result<(bool, Vec<u8>, Vec<u8>)> {
    let fixed = RECOVERY_MAGIC.len() + 37;
    if record.len() < fixed
        || !record.starts_with(RECOVERY_MAGIC)
        || record[RECOVERY_MAGIC.len()..RECOVERY_MAGIC.len() + 32] != expected_store_id
    {
        return Err(DriverError::Conflict);
    }
    let offset = RECOVERY_MAGIC.len() + 32;
    let owned = match record[offset] {
        0 => false,
        1 => true,
        _ => return Err(DriverError::Conflict),
    };
    let name_len = u16::from_be_bytes(record[offset + 1..offset + 3].try_into().unwrap()) as usize;
    let identity_len =
        u16::from_be_bytes(record[offset + 3..offset + 5].try_into().unwrap()) as usize;
    if name_len == 0
        || name_len > 255
        || identity_len == 0
        || fixed
            .checked_add(name_len)
            .and_then(|length| length.checked_add(identity_len))
            != Some(record.len())
    {
        return Err(DriverError::Conflict);
    }
    let name = record[fixed..fixed + name_len].to_vec();
    if name.contains(&0) || name == b"." || name == b".." || name.contains(&b'/') {
        return Err(DriverError::Conflict);
    }
    Ok((owned, name, record[fixed + name_len..].to_vec()))
}

pub(super) fn recover_owned_workspaces(
    parent_path: &Path,
    store_id: [u8; 32],
    facts: &Recorder,
) -> Result<()> {
    let parent = super::ffi::open_directory_path_nofollow(parent_path)?;
    let mut removed = false;
    let recovery = (|| -> Result<()> {
        for entry in super::ffi::directory_entries(&parent)? {
            let (staging_name, kind, _, token, staging_identity) = match entry {
                Ok(entry) => entry,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            if kind != NativeKind::Directory || !staging_name.starts_with(b".layerfs-staging-") {
                continue;
            }
            let staging = match super::ffi::open_directory_at(&parent, &staging_name) {
                Ok(staging) => staging,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            validate_expected(&staging, Some(&token))?;
            let mut marker = match super::ffi::open_regular_at(&staging, RECOVERY_MARKER, false) {
                Ok(marker) => marker,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Err(DriverError::Conflict)
                }
                Err(error) => {
                    return Err(DriverError::Io(std::io::Error::new(
                        error.kind(),
                        format!("open recovery marker: {error}"),
                    )))
                }
            };
            if !super::ffi::try_lock_exclusive(&marker)? {
                continue;
            }
            let mut record = Vec::new();
            Read::by_ref(&mut marker)
                .take(4097)
                .read_to_end(&mut record)?;
            if record.len() > 4096 {
                return Err(DriverError::Conflict);
            }
            let (owned_root, root_name, root_identity) = decode_recovery_record(&record, store_id)?;
            if owned_root {
                match super::ffi::open_directory_at(&parent, &root_name) {
                    Ok(root) => {
                        if super::ffi::file_stable_token(&root)? != root_identity {
                            return Err(DriverError::VisibilityAmbiguous);
                        }
                        remove_recovered_tree(
                            &root,
                            &parent,
                            &root_name,
                            &root_identity,
                            b".layerfs-recovered-root-",
                            facts,
                        )?;
                        removed = true;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
            remove_recovered_tree(
                &staging,
                &parent,
                &staging_name,
                &staging_identity,
                b".layerfs-recovered-staging-",
                facts,
            )?;
            removed = true;
        }
        Ok(())
    })();
    if removed {
        sync_directory_file(&parent, facts, DirectorySyncOwner::RootParent)
            .map_err(|_| DriverError::DurabilityAmbiguous)?;
    }
    recovery
}

fn remove_recovered_tree(
    root: &File,
    parent: &File,
    name: &[u8],
    identity: &[u8],
    prefix: &[u8],
    facts: &Recorder,
) -> Result<()> {
    for _ in 0..64 {
        let mut tombstone = prefix.to_vec();
        tombstone.extend_from_slice(std::process::id().to_string().as_bytes());
        tombstone.push(b'-');
        tombstone.extend_from_slice(
            TEMP_SERIAL
                .fetch_add(1, Ordering::Relaxed)
                .to_string()
                .as_bytes(),
        );
        let start = Instant::now();
        let removed =
            super::ffi::detach_and_remove_owned_tree(root, parent, name, &tombstone, identity);
        finish_cleanup(facts, start, removed.is_ok());
        match removed {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) if error.raw_os_error() == Some(libc::ESTALE) => {
                return Err(DriverError::VisibilityAmbiguous)
            }
            Err(error) => {
                return Err(DriverError::Io(std::io::Error::new(
                    error.kind(),
                    format!("remove recovered tree: {error}"),
                )))
            }
        }
    }
    Err(DriverError::Conflict)
}
