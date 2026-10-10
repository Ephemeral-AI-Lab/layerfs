//! Actual protected configuration descriptors and process/placement checks.
use layerfs_bridge::daemon_setup::DaemonSetup;
use nix::{fcntl::OFlag, unistd::Uid};
use std::{
    fs::{self, OpenOptions},
    io::{self, Read},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::Path,
};

pub(super) fn read(path: &Path) -> io::Result<DaemonSetup> {
    secure_process()?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags((OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC).bits())
        .open(path)?;
    let metadata = file.metadata()?;
    let uid = Uid::effective().as_raw();
    if !metadata.is_file() || metadata.uid() != uid || metadata.mode() & 0o077 != 0 {
        return Err(invalid("private config ownership/mode/type"));
    }
    private_directory(
        path.parent().ok_or_else(|| invalid("config parent"))?,
        false,
    )?;
    let mut bytes = Vec::with_capacity(8193);
    (&mut file).take(8193).read_to_end(&mut bytes)?;
    if bytes.len() > 8192 {
        return Err(invalid("private config record size"));
    }
    let config = DaemonSetup::decode(&bytes).map_err(|_| invalid("private config fields"))?;
    if config.command_uid == uid {
        return Err(invalid("command and daemon identities must differ"));
    }
    // One early reservation restores any value below the refill window; a
    // larger one would have consecutive creates each attempt a reservation.
    if config.limits.serial_low_water >= layerfs_workspace::SERIAL_REFILL {
        return Err(invalid("serial low-water must be below the refill window"));
    }
    let store = Path::new(&config.store);
    let overlay = Path::new(&config.overlay);
    let mounts = Path::new(&config.mounts);
    if store == overlay
        || store.starts_with(mounts)
        || overlay.starts_with(mounts)
        || path.starts_with(mounts)
    {
        return Err(invalid("separate protected backing and mount paths"));
    }
    private_directory(
        overlay.parent().ok_or_else(|| invalid("Overlay parent"))?,
        true,
    )?;
    private_directory(
        store.parent().ok_or_else(|| invalid("Store parent"))?,
        config.existing_store.is_none(),
    )?;
    if let Some(manifest) = &config.existing_store {
        if manifest.daemon_sqlite.is_none() {
            return Err(invalid("existing installation metadata"));
        }
        protected_file(store)?;
    }
    Ok(config)
}
pub(super) fn protected_file(path: &Path) -> io::Result<()> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags((OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC).bits())
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != Uid::effective().as_raw()
        || metadata.mode() & 0o077 != 0
    {
        return Err(invalid("protected file ownership/mode/type"));
    }
    Ok(())
}
fn private_directory(path: &Path, create: bool) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => (),
        Err(error) if create && error.kind() == io::ErrorKind::NotFound => {
            // Exactly one create attempt. A race/failure is not an open fallback.
            fs::DirBuilder::new().mode(0o700).create(path)?;
        }
        Err(error) => return Err(error),
    }
    let directory = OpenOptions::new()
        .read(true)
        .custom_flags((OFlag::O_NOFOLLOW | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC).bits())
        .open(path)?;
    let metadata = directory.metadata()?;
    if !metadata.is_dir()
        || metadata.uid() != Uid::effective().as_raw()
        || metadata.mode() & 0o077 != 0
    {
        return Err(invalid("private directory ownership/mode/type"));
    }
    Ok(())
}
#[cfg(target_os = "linux")]
fn secure_process() -> io::Result<()> {
    nix::sys::stat::umask(nix::sys::stat::Mode::from_bits_truncate(0o077));
    nix::sys::prctl::set_dumpable(false).map_err(io::Error::from)?;
    if nix::sys::prctl::get_dumpable().map_err(io::Error::from)? {
        return Err(invalid("daemon remains dumpable"));
    }
    Ok(())
}
#[cfg(not(target_os = "linux"))]
fn secure_process() -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "production daemon requires Linux process protection",
    ))
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

/// R16 native handoffs plus the fixed control connection population; one
/// additional namespace lane serves explicitly unscoped constructor callers.
pub(super) fn read_limits(
    limits: layerfs_bridge::daemon_setup::DaemonLimits,
) -> crate::store::ReadLimits {
    crate::store::ReadLimits {
        namespaces: limits.namespaces as usize + 1,
        requests_per_namespace: 16 + usize::from(limits.connections),
    }
}
