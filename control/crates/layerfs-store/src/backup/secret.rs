//! The home's secret folder, as the backup family uses it: a bucket key's
//! secret kept at `secret/bucket/<bucket>` under the home (folders 0700,
//! the file 0600), written whole by one rename, never answered and never
//! logged.
//!
//! SWITCH POINT (call surface 2.0): saved PostgreSQL logins (package F-pgd)
//! build the home's own secret folder by the same location rule (the
//! home's `secret/` folder, 0600 files). When that lands, these three
//! functions become calls of it with the same names (`bucket/<bucket>`);
//! nothing else in the backup family reads or writes a secret.
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::home;
use layerfs_home::outcome::{failed, refused};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The secret named `name` (`bucket/<bucket>`) under `home`.
fn path(home: &Path, name: &str) -> PathBuf {
    home.join("secret").join(name)
}

/// The secret named `name`; a missing one is `Failed`, naming the call
/// that keeps it again.
pub fn read(home: &Path, name: &str) -> Result<String> {
    match std::fs::read_to_string(path(home, name)) {
        Ok(secret) if !secret.is_empty() => Ok(secret),
        Ok(_) => Err(refused(
            "Failed",
            format!("the home's secret {name} is empty"),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(refused(
            "Failed",
            format!("the home keeps no secret {name}; disconnect it and connect it again"),
        )),
        Err(error) => Err(refused(
            "Failed",
            format!("the home's secret {name}: {error}"),
        )),
    }
}

/// Keeps `secret` as the secret named `name`: a private temporary file in
/// its folder, then one rename.
pub fn write(home: &Path, name: &str, secret: &str) -> Result<()> {
    let target = path(home, name);
    let folder = target.parent().expect("a secret has a folder");
    home::private_dir(folder)?;
    let temporary = folder.join(format!(
        ".{}.{}.tmp",
        target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("secret"),
        std::process::id()
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temporary)
        .map_err(failed)?;
    let written = file.write_all(secret.as_bytes()).map_err(failed);
    drop(file);
    match written.and_then(|()| std::fs::rename(&temporary, &target).map_err(failed)) {
        Ok(()) => Ok(()),
        Err(refusal) => {
            let _ = std::fs::remove_file(&temporary);
            Err(refusal)
        }
    }
}

/// Forgets the secret named `name`; one already gone is no error.
pub fn remove(home: &Path, name: &str) -> Result<()> {
    match std::fs::remove_file(path(home, name)) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(failed(error)),
        _ => Ok(()),
    }
}
