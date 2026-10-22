//! Plugin configuration for one already installed project.

use crate::error::LayerFsError;
#[cfg(unix)]
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};

/// The deployment a Workspace call needs: the Engine, image, Store volume
/// and daemon executable that Sandboxes are made from.
#[derive(Clone, Debug)]
pub struct Deploy {
    /// Docker Engine socket.
    pub docker: PathBuf,
    /// Exact local image digest, `sha256:<64 hex>`.
    pub image: String,
    /// Named volume holding the installed Store.
    pub volume: String,
    /// Host path to the LayerFS daemon binary.
    pub daemon: PathBuf,
    /// Optional loopback host for the daemon; `None` dials it as published.
    pub daemon_host: Option<String>,
    /// Optional container name prefix (default `layerfs-`).
    pub container_prefix: Option<String>,
}

/// One project's private LayerFS home and installed Store.
#[derive(Clone, Debug)]
pub struct Config {
    /// LayerFS project name.
    pub project: String,
    /// Absolute `LAYERFS_HOME`.
    pub home: PathBuf,
    /// Installed Store file opened by this process.
    pub store_path: PathBuf,
    /// Workspace deployment; `None` leaves Store calls working and refuses
    /// every `workspace_*` call.
    pub deploy: Option<Deploy>,
}

impl Config {
    /// Checks the platform, project name, private paths and deployment shape
    /// before `apply` provides the service. It opens no Store.
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
            let record = project.join("project.record");
            match std::fs::symlink_metadata(&record) {
                Ok(meta) if meta.file_type().is_symlink() || !meta.is_file() => {
                    return Err(LayerFsError::refused(
                        "Unauthorized",
                        format!("{} must be a regular file", record.display()),
                        serde_json::Value::Null,
                    ));
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Err(LayerFsError::refused(
                        "NoSuchProject",
                        format!("no project record at {}", record.display()),
                        serde_json::Value::Null,
                    ));
                }
                Err(error) => return Err(error.into()),
            }
            private_dir(&project)?;
            if !regular_file(&self.store_path)? {
                return Err(LayerFsError::refused(
                    "NoStore",
                    format!("no installed Store at {}", self.store_path.display()),
                    serde_json::Value::Null,
                ));
            }
            if let Some(deploy) = &self.deploy {
                deploy.validate()?;
            }
            Ok(())
        }
    }
}

impl Deploy {
    fn validate(&self) -> Result<(), LayerFsError> {
        if !self.docker.is_absolute() {
            return Err(LayerFsError::invalid("deploy.docker must be absolute"));
        }
        let digest = self.image.strip_prefix("sha256:").unwrap_or_default();
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(LayerFsError::invalid(
                "deploy.image must be an exact local image digest, sha256:<64 hex>",
            ));
        }
        if self.volume.is_empty()
            || !self
                .volume
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_alphanumeric())
            || !self
                .volume
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
        {
            return Err(LayerFsError::invalid("deploy.volume is not a volume name"));
        }
        if !self.daemon.is_absolute() {
            return Err(LayerFsError::invalid("deploy.daemon must be absolute"));
        }
        #[cfg(unix)]
        {
            match std::fs::metadata(&self.docker) {
                Ok(meta) if meta.file_type().is_socket() => {}
                Ok(_) => {
                    return Err(LayerFsError::invalid(
                        "deploy.docker must name a Docker Engine socket",
                    ))
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Err(LayerFsError::invalid(
                        "deploy.docker does not name an existing socket",
                    ))
                }
                Err(error) => return Err(error.into()),
            }
            match std::fs::metadata(&self.daemon) {
                Ok(meta) if meta.is_file() => {}
                Ok(_) => {
                    return Err(LayerFsError::invalid(
                        "deploy.daemon must name a daemon executable",
                    ))
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Err(LayerFsError::invalid(
                        "deploy.daemon does not name an existing file",
                    ))
                }
                Err(error) => return Err(error.into()),
            }
        }
        if let Some(host) = &self.daemon_host {
            if host.is_empty() {
                return Err(LayerFsError::invalid(
                    "deploy.daemon_host must not be empty",
                ));
            }
        }
        if let Some(prefix) = &self.container_prefix {
            if prefix.is_empty()
                || !prefix
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_alphanumeric())
                || !prefix
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
            {
                return Err(LayerFsError::invalid(
                    "deploy.container_prefix is not a container name prefix",
                ));
            }
        }
        Ok(())
    }
}

fn regular_file(path: &Path) -> Result<bool, LayerFsError> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => Ok(meta.is_file() && !meta.file_type().is_symlink()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn private_dir(path: &Path) -> Result<(), LayerFsError> {
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

#[cfg(not(target_os = "linux"))]
pub(crate) fn private_dir(path: &Path) -> Result<(), LayerFsError> {
    let meta = std::fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(LayerFsError::invalid(format!(
            "{} must be a directory",
            path.display()
        )));
    }
    Ok(())
}
