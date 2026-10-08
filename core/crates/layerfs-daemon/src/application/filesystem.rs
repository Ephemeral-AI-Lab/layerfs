//! Native serving assembly over the daemon's one Store and one overlay owner.
use crate::{
    control::{NativeServing, Service},
    store::Store,
    Owner,
};
use std::sync::Arc;

/// Control and native serving share the same Store handle and overlay owner;
/// the Service's registry stays the sole authority for every mount.
pub(super) fn service(
    store: Arc<Store>,
    owner: &Owner,
    native: Option<&Arc<NativeServing>>,
) -> Arc<Service> {
    Arc::new(match native {
        Some(native) => Service::with_native(store, owner, native.clone()),
        None => Service::new(store, owner),
    })
}
/// Native serving is a Linux capability; elsewhere control alone is assembled.
#[cfg(not(target_os = "linux"))]
pub(super) fn assemble(
    _setup: &layerfs_bridge::daemon_setup::DaemonSetup,
) -> std::io::Result<Option<Arc<NativeServing>>> {
    Ok(None)
}
#[cfg(target_os = "linux")]
pub(super) use linux::assemble;
#[cfg(target_os = "linux")]
mod linux {
    use crate::control::{NativeConfig, NativeServing};
    use layerfs_bridge::daemon_setup::DaemonSetup;
    use nix::{
        fcntl::OFlag,
        unistd::{Gid, Uid},
    };
    use std::{
        fs::{self, OpenOptions, Permissions},
        io,
        os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        path::Path,
        sync::Arc,
        time::Duration,
    };

    /// Observation deadline for one Attach's all-loop serving evidence.
    const READY_WAIT: Duration = Duration::from_secs(5);
    /// Observation deadline for one unmount's loop joins and request drain.
    /// Expiry keeps the exact owners Retained; it cancels and drops nothing.
    const DRAIN_WAIT: Duration = Duration::from_secs(5);

    /// Starts the fixed shared workers once, before control readiness. No
    /// device is opened and nothing is mounted until an explicit Attach.
    pub(in crate::application) fn assemble(
        setup: &DaemonSetup,
    ) -> io::Result<Option<Arc<NativeServing>>> {
        let mounts = Path::new(&setup.mounts);
        device()?;
        home(mounts)?;
        let limits = setup.limits;
        NativeServing::start(NativeConfig {
            mounts: mounts.to_owned(),
            owner_uid: Uid::effective().as_raw(),
            owner_gid: Gid::effective().as_raw(),
            command_uid: setup.command_uid,
            command_gid: setup.command_gid,
            read_handles: usize::from(limits.read_handles),
            namespaces: limits.namespaces as usize,
            ready_wait: READY_WAIT,
            drain_wait: DRAIN_WAIT,
        })
        .map(|serving| Some(Arc::new(serving)))
        .map_err(|failure| io::Error::other(failure.to_string()))
    }
    /// The command identity must never open the FUSE device. The deployment
    /// supplies the daemon's own device node; one attempt narrows it to its
    /// owner before control readiness. A node the daemon does not own, or one
    /// that stays reachable, refuses startup rather than serving unprotected.
    fn device() -> io::Result<()> {
        let path = Path::new("/dev/fuse");
        let protected = |metadata: &fs::Metadata| {
            metadata.file_type().is_char_device() && metadata.uid() == Uid::effective().as_raw()
        };
        let metadata = fs::symlink_metadata(path)?;
        if !protected(&metadata) {
            return Err(invalid("FUSE device ownership/type"));
        }
        if metadata.mode() & 0o077 != 0 {
            fs::set_permissions(path, Permissions::from_mode(0o600))?;
            let narrowed = fs::symlink_metadata(path)?;
            if !protected(&narrowed) || narrowed.mode() & 0o077 != 0 {
                return Err(invalid("FUSE device remains reachable"));
            }
        }
        Ok(())
    }
    /// The mount home is daemon-owned, writable by the daemon alone and
    /// traversable by the command identity. One create attempt; an existing
    /// path is checked, never repaired.
    fn home(path: &Path) -> io::Result<()> {
        if !path.is_absolute() {
            return Err(invalid("mount home must be absolute"));
        }
        match fs::symlink_metadata(path) {
            Ok(_) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::DirBuilder::new().mode(0o755).create(path)?;
                // The protected process umask narrows the create mode.
                fs::set_permissions(path, Permissions::from_mode(0o755))?;
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
            || metadata.mode() & 0o022 != 0
            || metadata.mode() & 0o005 != 0o005
        {
            return Err(invalid("mount home ownership/mode/type"));
        }
        Ok(())
    }
    fn invalid(message: &'static str) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidInput, message)
    }
}
