use crate::LayerFs;
use fuser::{Config, MountOption, Session};
use layerfs_sdk::Workspace;
use signal_hook::consts::signal::{SIGHUP, SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub fn run_mount(
    workspace: Arc<Mutex<Workspace>>,
    mount: impl AsRef<Path>,
    uid: u32,
    gid: u32,
) -> std::io::Result<()> {
    let filesystem = LayerFs::new(workspace, uid, gid);
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
    config.n_threads = Some(4);
    config.clone_fd = true;
    let mut session = Session::new(filesystem, mount, &config)?;
    let mut unmount = session.unmount_callable();
    std::thread::spawn(move || {
        if let Ok(mut signals) = Signals::new([SIGTERM, SIGINT, SIGHUP]) {
            if signals.forever().next().is_some() {
                let _ = unmount.unmount();
            }
        }
    });
    session.run()
}
