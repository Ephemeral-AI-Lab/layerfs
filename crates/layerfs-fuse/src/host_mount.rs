use crate::{LayerFs, SharedPort};
use fuser::{BackgroundSession, Config, MountOption};
use std::path::Path;

pub struct HostMount(Option<BackgroundSession>);

impl HostMount {
    pub fn unmount(mut self) -> std::io::Result<()> {
        self.0.take().expect("live mount").umount_and_join()
    }

    pub fn join(mut self) -> std::io::Result<()> {
        self.0.take().expect("live mount").join()
    }
}

pub fn mount_host(
    port: SharedPort,
    mount: impl AsRef<Path>,
    uid: u32,
    gid: u32,
) -> std::io::Result<HostMount> {
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
    Ok(HostMount(Some(fuser::spawn_mount(
        filesystem, mount, &config,
    )?)))
}
