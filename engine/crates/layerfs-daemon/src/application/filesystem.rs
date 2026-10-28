//! Kernel serving assembly over the daemon's one Store and one overlay owner.
use crate::{
    control::{KernelServing, Service},
    store::Store,
    Owner,
};
use std::sync::Arc;

/// Control and kernel serving share the same Store handle and overlay owner;
/// the Service's registry stays the sole authority for every mount. The
/// Store takes its explicit serial low-water here, before any Workspace.
pub(super) fn service(
    store: Arc<Store>,
    owner: &Owner,
    kernel: Option<&Arc<KernelServing>>,
    serial_low_water: u64,
) -> Arc<Service> {
    store.set_serial_low_water(serial_low_water);
    Arc::new(match kernel {
        Some(kernel) => Service::with_kernel(store, owner, kernel.clone()),
        None => Service::new(store, owner),
    })
}
/// Kernel serving is a Linux capability; elsewhere control alone is assembled.
#[cfg(not(target_os = "linux"))]
pub(super) fn assemble(
    _setup: &layerfs_bridge::daemon_setup::DaemonSetup,
) -> std::io::Result<Option<Arc<KernelServing>>> {
    Ok(None)
}
#[cfg(target_os = "linux")]
pub(super) use linux::assemble;
#[cfg(target_os = "linux")]
mod linux {
    use crate::control::{KernelConfig, KernelServing};
    use layerfs_bridge::daemon_setup::DaemonSetup;
    use nix::{
        fcntl::OFlag,
        unistd::{Gid, Uid},
    };
    use std::{
        fs::{self, OpenOptions, Permissions},
        io::{self, Write},
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
    ) -> io::Result<Option<Arc<KernelServing>>> {
        let mounts = Path::new(&setup.mounts);
        device()?;
        home(mounts)?;
        // Beside the Overlay file, in the directory configuration checked
        // private, so no command identity can reach another connection.
        // Best effort: where the mount is refused (a user namespace refuses
        // it, though not FUSE mounts) the shared tree, unmounted there, binds
        // nothing and forced unmount is refused before any effect, as it was
        // before this mount existed. The refusal is written once to stderr.
        let overlay = Path::new(&setup.overlay).parent();
        let private = overlay.ok_or_else(|| invalid("Overlay parent"))?;
        let control = match layerfs_fuse::mount::control_root(&private.join("fusectl")) {
            Ok(control) => control,
            Err(cause) => {
                let _ = writeln!(io::stderr().lock(), "LAYERFS_DAEMON_ABORT_UNBOUND {cause}");
                layerfs_fuse::mount::CONTROL.into()
            }
        };
        KernelServing::start(KernelConfig {
            mounts: mounts.to_owned(),
            owner_uid: Uid::effective().as_raw(),
            owner_gid: Gid::effective().as_raw(),
            command_uid: setup.command_uid,
            command_gid: setup.command_gid,
            ready_wait: READY_WAIT,
            drain_wait: DRAIN_WAIT,
            control,
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
