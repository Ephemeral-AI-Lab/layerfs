use crate::store::set_private;
use crate::{DurableError, DurableStore, Result};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

impl DurableStore {
    pub fn backup(&self, destination: &Path) -> Result<()> {
        self.storage.backup_to(destination)?;
        Ok(())
    }

    pub fn restore(backup: &Path, root: &Path) -> Result<Self> {
        if fs::symlink_metadata(root).is_ok() {
            return Err(DurableError::InvalidPath);
        }
        let parent = root.parent().ok_or(DurableError::InvalidPath)?;
        fs::create_dir_all(parent)?;
        let parent = fs::canonicalize(parent)?;
        let destination = parent.join(root.file_name().ok_or(DurableError::InvalidPath)?);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| std::io::Error::other(error.to_string()))?
            .as_nanos();
        let staging = parent.join(format!(".layerfs-restore-{}-{nonce}", std::process::id()));
        fs::create_dir(&staging)?;
        set_private(&staging)?;
        let staging_identity = directory_identity(&staging)?;
        let restored = (|| {
            let generation_root = staging.join("durable.sqlite.generations");
            let storage = layerfs_storage::generation::restore_full_durable_backup(
                backup,
                &generation_root,
                &layerfs_storage::generation::NativeGenerationDriver,
            )?;
            let expected = storage.storage_id();
            drop(storage);
            fs::File::open(&staging)?.sync_all()?;
            if fs::symlink_metadata(&destination).is_ok()
                || directory_identity(&staging)? != staging_identity
            {
                return Err(DurableError::InvalidPath);
            }
            fs::rename(&staging, &destination)?;
            fs::File::open(&parent)?.sync_all()?;
            let restored = Self::open(&destination)?;
            if restored.storage_id() != expected {
                return Err(DurableError::InvalidPath);
            }
            Ok(restored)
        })();
        if restored.is_err()
            && staging.exists()
            && directory_identity(&staging).ok() == Some(staging_identity)
        {
            let _ = fs::remove_dir_all(&staging);
            let _ = fs::File::open(&parent).and_then(|directory| directory.sync_all());
        }
        restored
    }
}

#[cfg(unix)]
fn directory_identity(path: &Path) -> std::io::Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() {
        return Err(std::io::Error::other("restore path is not a directory"));
    }
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(not(unix))]
fn directory_identity(path: &Path) -> std::io::Result<(u64, u64)> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() {
        return Err(std::io::Error::other("restore path is not a directory"));
    }
    Ok((metadata.len(), 0))
}
