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
    super::filesystem::check_route(config.route)?;
    // The NFS route opens no FUSE device and the native route runs its
    // commands as the daemon's own user, unconfined (no isolation); every
    // other route keeps the two identities apart.
    if config.command_uid == uid && !super::filesystem::nfs_route(&config) {
        return Err(invalid("command and daemon identities must differ"));
    }
    // One early reservation restores any value below the refill window; a
    // larger one would have consecutive creates each attempt a reservation.
    if config.limits.serial_low_water >= layerfs_workspace::SERIAL_REFILL {
        return Err(invalid("serial low-water must be below the refill window"));
    }
    let overlay = Path::new(&config.overlay);
    let mounts = Path::new(&config.mounts);
    if let layerfs_bridge::DaemonStore::Sqlite { locator, .. } = &config.store {
        let store = Path::new(locator);
        if store == overlay
            || store.starts_with(mounts)
            || overlay.starts_with(mounts)
            || path.starts_with(mounts)
        {
            return Err(invalid("separate protected backing and mount paths"));
        }
    } else if overlay.starts_with(mounts) || path.starts_with(mounts) {
        return Err(invalid("separate protected backing and mount paths"));
    }
    private_directory(
        overlay.parent().ok_or_else(|| invalid("Overlay parent"))?,
        true,
    )?;
    // a PostgreSQL Store (#506) has no file or volume here
    let (locator, install) = match &config.store {
        layerfs_bridge::DaemonStore::Postgres { .. } => return Ok(config),
        layerfs_bridge::DaemonStore::Sqlite { locator, install } => (locator, install),
    };
    let store = Path::new(locator);
    private_directory(
        store.parent().ok_or_else(|| invalid("Store parent"))?,
        install.is_none(),
    )?;
    if let Some(manifest) = install {
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
    let uid = Uid::effective().as_raw();
    let mode = metadata.mode() & 0o7777;
    if !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "private directory ownership/mode/type at {}: not a directory (mode {mode:04o})",
                path.display()
            ),
        ));
    }
    if metadata.uid() != uid {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "private directory ownership/mode/type at {}: owned by uid {}, expected uid {uid}",
                path.display(),
                metadata.uid()
            ),
        ));
    }
    if mode & 0o077 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "private directory ownership/mode/type at {}: mode is {mode:04o}, expected 0700 (fix with: chmod 700 {})",
                path.display(),
                path.display()
            ),
        ));
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
/// macOS (the native NFS route): files the daemon creates are its own
/// alone, and it never writes a core file holding its keys or Store pages.
/// There is no dumpable flag; a debugger of the same user is not refused.
#[cfg(target_os = "macos")]
fn secure_process() -> io::Result<()> {
    use nix::sys::resource::{setrlimit, Resource};
    nix::sys::stat::umask(nix::sys::stat::Mode::from_bits_truncate(0o077));
    setrlimit(Resource::RLIMIT_CORE, 0, 0).map_err(io::Error::from)?;
    Ok(())
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn secure_process() -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "production daemon requires Linux or macOS process protection",
    ))
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
