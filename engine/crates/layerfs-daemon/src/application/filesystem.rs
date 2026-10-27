//! Kernel serving assembly over the daemon's one Store and one overlay owner.
use crate::{
    control::{ExportServing, ExportServingConfig, Helper, KernelServing, Mounter, Service},
    store::Store,
    Owner,
};
use layerfs_bridge::daemon_setup::DaemonSetup;
use std::{path::Path, sync::Arc};

/// Selects the kernel route: `nfs` serves every mount over loopback NFSv3
/// (the native route); anything else, or unset, is FUSE.
pub const KERNEL_ROUTE: &str = "LAYERFS_KERNEL";
/// Optional helper command lines (`{port}`, `{dir}` replaced) overriding the
/// host client's defaults for the NFS route.
pub const NFS_MOUNT: &str = "LAYERFS_NFS_MOUNT";
pub const NFS_UNMOUNT: &str = "LAYERFS_NFS_UNMOUNT";
pub const NFS_FORCE: &str = "LAYERFS_NFS_FORCE";
pub const NFS_FLUSH: &str = "LAYERFS_NFS_FLUSH";

/// Whether this daemon serves its mounts over NFS.
/// macOS has no other route: there it is always NFS.
pub(super) fn nfs_route() -> bool {
    cfg!(target_os = "macos") || std::env::var(KERNEL_ROUTE).is_ok_and(|route| route == "nfs")
}
/// NFS serving for the native route, from the protected setup and the
/// declared helper overrides. Nothing is mounted until an explicit Attach.
pub(super) fn assemble_export(setup: &DaemonSetup) -> std::io::Result<Option<Arc<ExportServing>>> {
    if !nfs_route() {
        return Ok(None);
    }
    let mounts = Path::new(&setup.mounts);
    if !mounts.is_absolute() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "mount home must be absolute",
        ));
    }
    if !mounts.is_dir() {
        std::fs::create_dir(mounts)?;
    }
    let mut mounter = Mounter::host();
    let helper = |name: &str| {
        std::env::var(name)
            .ok()
            .and_then(|line| Helper::words(&line))
    };
    if let Some(mount) = helper(NFS_MOUNT) {
        mounter.mount = mount;
    }
    if let Some(unmount) = helper(NFS_UNMOUNT) {
        mounter.unmount = unmount;
    }
    if let Some(force) = helper(NFS_FORCE) {
        mounter.force = force;
    }
    mounter.flush = helper(NFS_FLUSH);
    let serving = ExportServing::start(ExportServingConfig::new(
        mounts.to_owned(),
        setup.command_uid,
        setup.command_gid,
        mounter,
    ));
    // A daemon of this mount home that ended without unmounting left its
    // client mounts; they are forced off before this one serves.
    for line in serving.reclaim() {
        eprintln!("LAYERFS_NFS_RECLAIM {line}");
    }
    Ok(Some(Arc::new(serving)))
}

/// Control and kernel serving share the same Store handle and overlay owner;
/// the Service's registry stays the sole authority for every mount. The
/// Store takes its explicit serial low-water here, before any Workspace.
pub(super) fn service(
    store: Arc<Store>,
    owner: &Owner,
    kernel: Option<&Arc<KernelServing>>,
    export: Option<&Arc<ExportServing>>,
    serial_low_water: u64,
) -> Arc<Service> {
    store.set_serial_low_water(serial_low_water);
    Arc::new(match (export, kernel) {
        (Some(export), _) => Service::with_export(store, owner, export.clone()),
        (None, Some(kernel)) => Service::with_kernel(store, owner, kernel.clone()),
        (None, None) => Service::new(store, owner),
    })
}
/// Kernel serving is a Linux capability; elsewhere control alone is assembled.
#[cfg(not(target_os = "linux"))]
pub(super) fn assemble(_setup: &DaemonSetup) -> std::io::Result<Option<Arc<KernelServing>>> {
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
    ) -> io::Result<Option<Arc<KernelServing>>> {
        // The NFS route opens no FUSE device.
        if super::nfs_route() {
            return Ok(None);
        }
        let mounts = Path::new(&setup.mounts);
        device()?;
        home(mounts)?;
        KernelServing::start(KernelConfig {
            mounts: mounts.to_owned(),
            owner_uid: Uid::effective().as_raw(),
            owner_gid: Gid::effective().as_raw(),
            command_uid: setup.command_uid,
            command_gid: setup.command_gid,
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
