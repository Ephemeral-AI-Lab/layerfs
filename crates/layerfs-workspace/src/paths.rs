//! Owned workspace-path creation, validation, and removal.

use crate::{Result, WorkspaceError, WorkspaceTicket};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const OWNER_MAGIC: &[u8; 8] = b"LFSWORK1";

#[derive(Debug)]
pub struct WorkspacePaths {
    root: PathBuf,
    owner: PathBuf,
    recovery: PathBuf,
    view: PathBuf,
    spool: PathBuf,
    marker: Vec<u8>,
    identity: [u64; 2],
}

impl WorkspacePaths {
    pub fn create(working_root: &Path, ticket: &WorkspaceTicket) -> Result<Self> {
        reject_link(working_root)?;
        let workspaces = working_root.join("workspaces");
        fs::create_dir_all(&workspaces)?;
        reject_link(&workspaces)?;
        set_private(&workspaces)?;
        let name = format!(
            "{}-{}",
            hex(ticket.operation_id.as_bytes()),
            hex(&ticket.nonce)
        );
        let root = workspaces.join(name);
        fs::create_dir(&root)?;
        let identity = directory_identity(&root)?;
        let result = Self::initialize(root.clone(), identity, ticket);
        if result.is_err() {
            let _ = remove_setup_root(&root, identity);
        }
        result
    }

    fn initialize(root: PathBuf, identity: [u64; 2], ticket: &WorkspaceTicket) -> Result<Self> {
        set_private(&root)?;
        let owner = root.join("owner");
        let recovery = root.join("recovery");
        let view = root.join("view");
        let spool = root.join("spool");
        fs::create_dir(&view)?;
        fs::create_dir(&spool)?;
        set_private(&view)?;
        set_private(&spool)?;
        let marker = marker(ticket);
        write_new(&owner, &marker)?;
        write_new(&recovery, &marker)?;
        let paths = Self {
            root,
            owner,
            recovery,
            view,
            spool,
            marker,
            identity,
        };
        paths.validate()?;
        Ok(paths)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn view(&self) -> &Path {
        &self.view
    }

    pub fn spool(&self) -> &Path {
        &self.spool
    }

    pub fn recovery(&self) -> &Path {
        &self.recovery
    }

    pub fn validate(&self) -> Result<()> {
        if directory_identity(&self.root)? != self.identity {
            return Err(WorkspaceError::OwnershipMismatch);
        }
        for path in [&self.root, &self.view, &self.spool] {
            let metadata = fs::symlink_metadata(path)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(WorkspaceError::OwnershipMismatch);
            }
        }
        let owner = fs::symlink_metadata(&self.owner)?;
        let recovery = fs::symlink_metadata(&self.recovery)?;
        if owner.file_type().is_symlink()
            || !owner.is_file()
            || recovery.file_type().is_symlink()
            || !recovery.is_file()
            || fs::read(&self.owner)? != self.marker
            || fs::read(&self.recovery)? != self.marker
        {
            return Err(WorkspaceError::OwnershipMismatch);
        }
        Ok(())
    }

    pub fn remove_owned(&mut self) -> Result<()> {
        self.validate()?;
        let parent = self
            .root
            .parent()
            .ok_or(WorkspaceError::OwnershipMismatch)?;
        let name = self
            .root
            .file_name()
            .ok_or(WorkspaceError::OwnershipMismatch)?
            .to_string_lossy();
        let quarantine = parent.join(format!(".{name}.removing"));
        if fs::symlink_metadata(&quarantine).is_ok() {
            return Err(WorkspaceError::OwnershipMismatch);
        }
        fs::rename(&self.root, &quarantine)?;
        self.root = quarantine;
        self.owner = self.root.join("owner");
        self.recovery = self.root.join("recovery");
        self.view = self.root.join("view");
        self.spool = self.root.join("spool");
        self.validate()?;
        fs::remove_dir_all(&self.root)?;
        Ok(())
    }
}

fn marker(ticket: &WorkspaceTicket) -> Vec<u8> {
    let mut marker = Vec::with_capacity(8 + 32 + 32 + 16);
    marker.extend_from_slice(OWNER_MAGIC);
    marker.extend_from_slice(&ticket.working_storage_id);
    marker.extend_from_slice(ticket.operation_id.as_bytes());
    marker.extend_from_slice(&ticket.nonce);
    marker
}

fn remove_setup_root(root: &Path, identity: [u64; 2]) -> Result<()> {
    if directory_identity(root)? != identity {
        return Err(WorkspaceError::OwnershipMismatch);
    }
    let parent = root.parent().ok_or(WorkspaceError::OwnershipMismatch)?;
    let name = root
        .file_name()
        .ok_or(WorkspaceError::OwnershipMismatch)?
        .to_string_lossy();
    let quarantine = parent.join(format!(".{name}.setup-removing"));
    if fs::symlink_metadata(&quarantine).is_ok() {
        return Err(WorkspaceError::OwnershipMismatch);
    }
    fs::rename(root, &quarantine)?;
    if directory_identity(&quarantine)? != identity {
        return Err(WorkspaceError::OwnershipMismatch);
    }
    fs::remove_dir_all(quarantine)?;
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn reject_link(path: &Path) -> Result<()> {
    if path.exists() && fs::symlink_metadata(path)?.file_type().is_symlink() {
        Err(WorkspaceError::OwnershipMismatch)
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn directory_identity(path: &Path) -> Result<[u64; 2]> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(WorkspaceError::OwnershipMismatch);
    }
    Ok([metadata.dev(), metadata.ino()])
}

#[cfg(not(unix))]
fn directory_identity(_path: &Path) -> Result<[u64; 2]> {
    Err(WorkspaceError::OwnershipMismatch)
}

#[cfg(unix)]
fn set_private(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private(_path: &Path) -> Result<()> {
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_failure_cleanup_preserves_a_substituted_root() {
        let parent = std::env::temp_dir().join(format!(
            "layerfs-workspace-setup-substitution-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&parent);
        fs::create_dir(&parent).unwrap();
        let root = parent.join("owned");
        fs::create_dir(&root).unwrap();
        let identity = directory_identity(&root).unwrap();
        let displaced = parent.join("displaced");
        fs::rename(&root, &displaced).unwrap();
        fs::create_dir(&root).unwrap();
        fs::write(root.join("keep"), b"substitute").unwrap();

        assert!(matches!(
            remove_setup_root(&root, identity),
            Err(WorkspaceError::OwnershipMismatch)
        ));
        assert!(root.join("keep").exists());
        assert!(displaced.exists());
        fs::remove_dir_all(parent).unwrap();
    }
}
