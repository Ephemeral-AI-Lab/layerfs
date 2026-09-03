use crate::{LayerFs, SharedPort};
use fuser::{BackgroundSession, Config, MountOption};
use std::path::{Path, PathBuf};

pub struct HostMount {
    session: Option<BackgroundSession>,
    mountpoint: PathBuf,
}

impl HostMount {
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
