//! Plugin configuration for one already installed project.

use crate::error::LayerFsError;
use std::path::{Path, PathBuf};

/// One project's private LayerFS home and installed Store.
#[derive(Clone, Debug)]
pub struct Config {
    /// LayerFS project name.
    pub project: String,
    /// Absolute `LAYERFS_HOME`.
    pub home: PathBuf,
    /// Installed Store file opened by this process.
    pub store_path: PathBuf,
}

impl Config {
    /// Checks the platform, project name and private paths before `apply`
    /// provides the service. It opens no Store.
    pub fn validate(&self) -> Result<(), LayerFsError> {
        #[cfg(not(target_os = "linux"))]
        {
            return Err(LayerFsError::unsupported(
                "LayerFS Store calls require Linux",
            ));
        }

        #[cfg(target_os = "linux")]
        {
            layerfs_history::HistoryName::new(&self.project)
                .map_err(|error| LayerFsError::invalid(format!("project: {error}")))?;
            if !self.home.is_absolute() {
                return Err(LayerFsError::invalid("home must be absolute"));
            }
            if !self.store_path.is_absolute() {
                return Err(LayerFsError::invalid("store_path must be absolute"));
            }
            private_dir(&self.home)?;
            let project = self.home.join(&self.project);
            private_dir(&project)?;
            let record = project.join("project.record");
            if !record.is_file() {
                return Err(LayerFsError::refused(
                    "NoSuchProject",
                    format!("no project record at {}", record.display()),
                    serde_json::Value::Null,
                ));
            }
            if !self.store_path.is_file() {
                return Err(LayerFsError::refused(
                    "NoStore",
                    format!("no installed Store at {}", self.store_path.display()),
                    serde_json::Value::Null,
                ));
            }
            Ok(())
        }
    }
}

#[cfg(target_os = "linux")]
fn private_dir(path: &Path) -> Result<(), LayerFsError> {
    use std::os::unix::fs::MetadataExt;

    let meta = std::fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(LayerFsError::invalid(format!(
            "{} must be a directory",
            path.display()
        )));
    }
    // SAFETY: geteuid has no preconditions.
    let owner = unsafe { libc::geteuid() };
    if meta.uid() != owner || meta.mode() & 0o077 != 0 {
        return Err(LayerFsError::refused(
            "Unauthorized",
            format!("{} must be owned by this user and private", path.display()),
            serde_json::Value::Null,
        ));
    }
    Ok(())
}
