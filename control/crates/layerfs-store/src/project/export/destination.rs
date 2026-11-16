//! The destination on the caller's host: checked before any effect, and
//! the hidden sibling staging folder that becomes it by one rename.
use crate::project::staged;
use layerfs_bridge::control::SdkRefusal;
use layerfs_bridge::sdk_values::hex;
use layerfs_home::outcome::{failed, refused};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::DirBuilderExt;
use std::path::{Component, Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Checks `destination` on the caller's host before reaching the Store or
/// creating any staging directory.
pub(super) fn check_destination(home: &Path, text: &str) -> Result<PathBuf> {
    let dest = PathBuf::from(text);
    if !dest.is_absolute() {
        return Err(refused(
            "InvalidInput",
            "the destination must be an absolute path",
        ));
    }
    if dest
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(refused(
            "InvalidInput",
            format!(
                "destination {} must not contain `.` or `..` components",
                dest.display()
            ),
        ));
    }
    let Some(parent) = dest.parent() else {
        return Err(refused(
            "InvalidInput",
            format!(
                "destination {} must name a directory path with a parent",
                dest.display()
            ),
        ));
    };
    if dest.file_name().is_none() {
        return Err(refused(
            "InvalidInput",
            format!(
                "destination {} must name a directory, not the filesystem root",
                dest.display()
            ),
        ));
    }
    if overlaps_home(home, &dest, parent) {
        return Err(refused(
            "InvalidInput",
            format!(
                "destination {} is inside the Store home ({})",
                dest.display(),
                home.display()
            ),
        ));
    }
    check_destination_state(&dest, parent)?;
    Ok(dest)
}

fn overlaps_home(home: &Path, dest: &Path, parent: &Path) -> bool {
    if dest.starts_with(home) || home.starts_with(dest) {
        return true;
    }
    let Ok(canon_home) = fs::canonicalize(home) else {
        return false;
    };
    if let Ok(canon_parent) = fs::canonicalize(parent) {
        if let Some(name) = dest.file_name() {
            let resolved = canon_parent.join(name);
            if resolved.starts_with(&canon_home) || canon_home.starts_with(&resolved) {
                return true;
            }
        }
    }
    false
}

/// Verifies that `dest` is either absent (with an existing real parent
/// directory) or an existing empty real directory (not a symlink or file).
pub(super) fn check_destination_state(dest: &Path, parent: &Path) -> Result<()> {
    match fs::symlink_metadata(dest) {
        Ok(meta) => {
            if meta.file_type().is_symlink() || !meta.is_dir() {
                return Err(refused(
                    "Exists",
                    format!(
                        "destination {} already exists and is not a directory",
                        dest.display()
                    ),
                ));
            }
            let mut entries = fs::read_dir(dest)
                .map_err(|e| refused("Failed", format!("{}: {e}", dest.display())))?;
            if entries.next().transpose().map_err(failed)?.is_some() {
                return Err(refused(
                    "NotEmpty",
                    format!("destination {} is not empty", dest.display()),
                ));
            }
            check_parent_is_real_dir(parent)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => check_parent_is_real_dir(parent),
        Err(e) => Err(refused("Failed", format!("{}: {e}", dest.display()))),
    }
}

fn check_parent_is_real_dir(parent: &Path) -> Result<()> {
    let meta = match fs::metadata(parent) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(refused(
                "NotFound",
                format!(
                    "destination parent directory {} does not exist",
                    parent.display()
                ),
            ))
        }
        Err(e) => return Err(refused("Failed", format!("{}: {e}", parent.display()))),
    };
    if !meta.is_dir() {
        return Err(refused(
            "NotDirectory",
            format!("destination parent {} is not a directory", parent.display()),
        ));
    }
    // refuse intermediate symlinks when all ancestor components are under a
    // non-system path or if the immediate parent itself is a symlink when
    // destination already exists; note `/var` -> `/private/var` on macOS is a
    // system root symlink, so we only check `metadata(parent).is_dir()` above.
    Ok(())
}

/// Hidden sibling staging directory `<parent>/.layerfs-export-<id>` that
/// cleans itself up (including `0o555` subdirectories) unless committed.
pub(super) struct StagingGuard {
    staging: PathBuf,
    committed: bool,
}

impl StagingGuard {
    pub(super) fn create(dest: &Path) -> Result<Self> {
        let parent = dest
            .parent()
            .ok_or_else(|| refused("InvalidInput", "destination has no parent"))?;
        let random = layerfs_home::helpers::random()?;
        let staging = parent.join(format!(".layerfs-export-{}", hex(&random[..8])));
        fs::DirBuilder::new()
            
            .create(&staging)
            .map_err(|e| {
                let code = if e.raw_os_error() == Some(libc::ENOSPC) {
                    "Capacity"
                } else {
                    "Failed"
                };
                refused(code, format!("{}: {e}", staging.display()))
            })?;
        Ok(Self {
            staging,
            committed: false,
        })
    }

    pub(super) fn path(&self) -> &Path {
        &self.staging
    }

    pub(super) fn publish(mut self, dest: &Path) -> Result<()> {
        let parent = dest
            .parent()
            .ok_or_else(|| refused("InvalidInput", "destination has no parent"))?;
        check_destination_state(dest, parent)?;
        fs::rename(&self.staging, dest).map_err(|e| {
            let code = match e.raw_os_error() {
                Some(libc::ENOTEMPTY | libc::EEXIST) => "NotEmpty",
                Some(libc::ENOSPC) => "Capacity",
                _ => "Failed",
            };
            refused(code, format!("{}: {e}", dest.display()))
        })?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for StagingGuard {
    fn drop(&mut self) {
        if !self.committed {
            let _ = staged::remove(&self.staging);
        }
    }
}
