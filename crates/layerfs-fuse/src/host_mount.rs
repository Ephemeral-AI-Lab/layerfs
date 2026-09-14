use crate::{LayerFs, SharedPort};
use fuser::{BackgroundSession, Config, MountOption};
use std::path::{Path, PathBuf};

pub struct HostMount {
    session: Option<BackgroundSession>,
    mountpoint: PathBuf,
}

impl HostMount {
    pub fn notifier(&self) -> std::io::Result<fuser::Notifier> {
        self.session
            .as_ref()
            .map(BackgroundSession::notifier)
            .ok_or_else(|| std::io::Error::other("LayerFS mount ended"))
    }

    pub fn invalidate_file(&self, node: crate::NodeId) -> std::io::Result<()> {
        self.notifier()?.inval_inode(fuser::INodeNo(node.0), 0, 0)
    }

    pub fn unmount(&mut self) -> std::io::Result<()> {
        if let Some(session) = self.session.take() {
            if session.umount_and_join().is_ok() {
                return Ok(());
            }
        }
        retry_unmount(&self.mountpoint)
    }

    pub fn join(mut self) -> std::io::Result<()> {
        match self.session.take() {
            Some(session) => session.join(),
            None => retry_unmount(&self.mountpoint),
        }
    }
}

fn retry_unmount(mountpoint: &Path) -> std::io::Result<()> {
    if !is_mounted(mountpoint)? {
        return Ok(());
    }
    for program in ["fusermount3", "fusermount"] {
        match std::process::Command::new(program)
            .arg("-u")
            .arg(mountpoint)
            .status()
        {
            Ok(status) if status.success() || !is_mounted(mountpoint)? => return Ok(()),
            Ok(_) | Err(_) => {}
        }
    }
    if std::process::Command::new("umount")
        .arg(mountpoint)
        .status()
        .is_ok_and(|status| status.success())
    {
        return Ok(());
    }
    if !is_mounted(mountpoint)? {
        return Ok(());
    }
    Err(std::io::Error::other(format!(
        "failed to unmount {}",
        mountpoint.display()
    )))
}

fn is_mounted(mountpoint: &Path) -> std::io::Result<bool> {
    use std::os::unix::ffi::OsStrExt;

    let mut encoded = Vec::new();
    for byte in mountpoint.as_os_str().as_bytes() {
        match byte {
            b' ' => encoded.extend_from_slice(br"\040"),
            b'\t' => encoded.extend_from_slice(br"\011"),
            b'\n' => encoded.extend_from_slice(br"\012"),
            b'\\' => encoded.extend_from_slice(br"\134"),
            byte => encoded.push(*byte),
        }
    }
    let mountinfo = std::fs::read("/proc/self/mountinfo")?;
    Ok(mountinfo
        .split(|byte| *byte == b'\n')
        .any(|line| line.split(|byte| *byte == b' ').nth(4) == Some(encoded.as_slice())))
}

pub fn mount_host(
    port: SharedPort,
    mount: impl AsRef<Path>,
    uid: u32,
    gid: u32,
) -> std::io::Result<HostMount> {
    let mountpoint = std::fs::canonicalize(mount.as_ref())?;
    let filesystem = LayerFs::new(port, uid, gid);
    let mut config = Config::default();
    config.mount_options = vec![
        MountOption::FSName("layerfs".into()),
        MountOption::Subtype("layerfs".into()),
        MountOption::RW,
        MountOption::NoDev,
        MountOption::NoSuid,
        MountOption::NoAtime,
        MountOption::DefaultPermissions,
    ];
    config.n_threads = Some(1);
    config.clone_fd = false;
    Ok(HostMount {
        session: Some(fuser::spawn_mount(filesystem, &mountpoint, &config)?),
        mountpoint,
    })
}

#[cfg(feature = "live")]
pub enum MountedOwner {
    Legacy(std::sync::Arc<crate::live_owner::LiveOwner>),
    Host(std::sync::Arc<crate::host_client::HostClient>),
}
#[cfg(feature = "live")]
impl MountedOwner {
    pub fn prepare_shutdown(&self) -> std::io::Result<()> {
        match self {
            Self::Legacy(owner) => owner.prepare_shutdown(),
            Self::Host(owner) => owner.prepare_shutdown(),
        }
    }
}

/// The host chooses the authority before mounting. A failed connection or
/// operation never changes that choice or falls back to a different owner.
#[cfg(feature = "live")]
pub fn mount_remote(
    endpoint: String,
    capability: [u8; 32],
    host_session: Option<[u8; 16]>,
    root: &Path,
) -> std::io::Result<(MountedOwner, crate::live_owner::LiveControl, HostMount)> {
    use std::sync::Arc;
    let runtime = crate::live_runtime::LiveRuntime::shared()?;
    if let Some(session) = host_session {
        let owner = Arc::new(runtime.block_on(crate::host_client::HostClient::connect(
            endpoint.clone(),
            capability,
            session,
            runtime.scheduler(),
        ))?);
        let control = owner.serve_control(endpoint, capability)?;
        let mount = mount_host(owner.clone(), root, 0, 0)?;
        owner.set_notifier(mount.notifier()?)?;
        owner.set_kernel_root(std::fs::File::open(root)?)?;
        Ok((MountedOwner::Host(owner), control, mount))
    } else {
        let owner = Arc::new(
            runtime
                .block_on(crate::live_owner::LiveOwner::connect(
                    endpoint.clone(),
                    capability,
                    runtime.scheduler(),
                ))
                .map_err(|error| std::io::Error::other(format!("live owner: {error:?}")))?,
        );
        let control = owner.serve_control(endpoint, capability)?;
        let mount = mount_host(owner.clone(), root, 0, 0)?;
        owner.set_notifier(mount.notifier()?)?;
        owner.set_kernel_root(std::fs::File::open(root)?)?;
        Ok((MountedOwner::Legacy(owner), control, mount))
    }
}
