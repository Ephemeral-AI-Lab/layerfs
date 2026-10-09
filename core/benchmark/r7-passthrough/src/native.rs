use crate::state::{owned_fd, Passthrough, WINDOW};
use fuser::{Config, Session, SessionACL};
use nix::mount::{mount, MsFlags};
use std::{
    env,
    ffi::CString,
    fs::OpenOptions,
    io,
    os::{
        fd::{AsRawFd, OwnedFd},
        unix::ffi::OsStrExt,
    },
    path::Path,
};

pub fn run() -> io::Result<()> {
    let args: Vec<_> = env::args_os().collect();
    if args.len() != 3 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: layerfs-r7-passthrough BACKING_DIRECTORY EMPTY_MOUNTPOINT",
        ));
    }
    let backing = Path::new(&args[1]).canonicalize()?;
    let target = Path::new(&args[2]).canonicalize()?;
    if target.starts_with(&backing) || backing.starts_with(&target) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "backing and mount must be disjoint",
        ));
    }
    if !backing.is_dir() || !target.is_dir() || target.read_dir()?.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "directories required; target must be empty",
        ));
    }
    let name = CString::new(backing.as_os_str().as_bytes())?;
    let root = owned_fd(unsafe {
        libc::open(
            name.as_ptr(),
            libc::O_PATH | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    })
    .map_err(|e| io::Error::from_raw_os_error(e.code()))?;
    let fs = Passthrough::new(root)?;
    // The per-request creation mode already includes the caller's umask.
    unsafe {
        libc::umask(0);
    }
    let device: OwnedFd = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/fuse")?
        .into();
    let data = format!("fd={},rootmode=40000,user_id={},group_id={},allow_other,default_permissions,max_read={WINDOW}",
        device.as_raw_fd(), unsafe { libc::geteuid() }, unsafe { libc::getegid() });
    mount(
        Some("layerfs-r7-passthrough"),
        &target,
        Some("fuse"),
        MsFlags::MS_NOSUID | MsFlags::MS_NODEV | MsFlags::MS_NOATIME,
        Some(data.as_str()),
    )
    .map_err(io::Error::from)?;
    let mut config = Config::default();
    config.acl = SessionACL::All;
    config.n_threads = Some(2);
    config.clone_fd = false;
    // Caller owns exactly one plain native unmount; no implicit/lazy teardown.
    let session = match Session::from_fd(fs, device, SessionACL::All, config) {
        Ok(session) => session,
        Err(error) => {
            let _ = crate::lifecycle::emit(&format!("{{\"event\":\"handshake_failure\",\"mounted\":true,\"connection_disposition\":\"from_fd failed and descriptor dropped\",\"original_error\":{}}}", crate::lifecycle::quote(&error.to_string())));
            return Err(error);
        }
    };
    crate::lifecycle::serve(session)
}
