//! The daemon's own mount-namespace descriptor, served to the `ns-exec` helper.
//!
//! The daemon is not dumpable (`application/config.rs`) and a Sandbox has
//! CAP_SYS_ADMIN but not CAP_SYS_PTRACE, so even root inside the container
//! cannot open `/proc/<daemon>/ns/mnt` (`EACCES`). The daemon can open its own
//! namespace, so it hands the descriptor to a root helper over a private Unix
//! socket in one `SCM_RIGHTS` message. The peer must be root in this process's
//! pid namespace; nothing is read from it and nothing else is sent.
use nix::{
    sys::socket::{
        getsockopt, recvmsg, sendmsg, sockopt, ControlMessage, ControlMessageOwned, MsgFlags,
        SockaddrStorage, UnixCredentials,
    },
    unistd,
};
use std::{
    fs::{self, File},
    io::{self, IoSlice, IoSliceMut},
    os::{
        fd::{AsRawFd, RawFd},
        unix::{
            fs::PermissionsExt,
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    thread,
};

/// One private socket that serves this process's mount-namespace descriptor.
pub struct NsFd {
    path: PathBuf,
}

impl NsFd {
    /// Bind the socket (mode 0600, any stale path removed) and serve it from
    /// one background thread. Called once, after the private namespace exists
    /// and before the daemon listens for control connections.
    pub fn start(path: &Path) -> io::Result<Self> {
        match fs::remove_file(path) {
            Ok(()) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
        let listener = UnixListener::bind(path)?;
        if let Err(error) = fs::set_permissions(path, fs::Permissions::from_mode(0o600)) {
            let _ = fs::remove_file(path);
            return Err(error);
        }
        let path = path.to_owned();
        thread::Builder::new()
            .name("layerfs-ns-fd".to_owned())
            .spawn(move || serve(listener))?;
        Ok(Self { path })
    }
}

impl Drop for NsFd {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn serve(listener: UnixListener) {
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                // One message and one close per connection; a refused peer
                // gets nothing and cannot retry over the same connection.
                let _ = send(stream);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return,
        }
    }
}

/// Admit one accepted peer, then send exactly one descriptor for this process's
/// mount namespace. A refusal closes the connection without sending.
fn send(stream: UnixStream) -> io::Result<()> {
    let credentials = getsockopt(&stream, sockopt::PeerCredentials)?;
    if !admit(&credentials) {
        return Err(io::Error::from(io::ErrorKind::PermissionDenied));
    }
    let namespace = File::open("/proc/self/ns/mnt")?;
    let descriptors = [namespace.as_raw_fd()];
    let control = [ControlMessage::ScmRights(&descriptors)];
    let payload = [0u8];
    sendmsg(
        stream.as_raw_fd(),
        &[IoSlice::new(&payload)],
        &control,
        MsgFlags::empty(),
        None::<&SockaddrStorage>,
    )?;
    Ok(())
}

/// A root peer in this process's pid namespace. The socket itself is 0600 in
/// the daemon's private directory; both checks make a grant to a wrong process
/// impossible, and an unreadable pid namespace refuses.
fn admit(credentials: &UnixCredentials) -> bool {
    admit_uid(credentials.uid()) && same_pid_namespace(credentials.pid())
}

fn admit_uid(uid: u32) -> bool {
    uid == 0
}

fn same_pid_namespace(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }
    let peer = fs::read_link(format!("/proc/{pid}/ns/pid"));
    let own = fs::read_link("/proc/self/ns/pid");
    matches!((peer, own), (Ok(peer), Ok(own)) if peer == own)
}

/// Connect to a daemon's socket and receive the one mount-namespace descriptor.
/// The received raw descriptor is reopened through `/proc/self/fd` so the
/// returned file is owned: this crate forbids unsafe code and `SCM_RIGHTS`
/// yields a raw descriptor. The label is checked before anything is returned.
pub fn receive(path: &Path) -> io::Result<File> {
    let stream = UnixStream::connect(path)?;
    let mut byte = [0u8; 1];
    let mut iov = [IoSliceMut::new(&mut byte)];
    let mut space = nix::cmsg_space!([RawFd; 1]);
    let message = recvmsg::<()>(
        stream.as_raw_fd(),
        &mut iov,
        Some(&mut space),
        MsgFlags::empty(),
    )?;
    let mut descriptors = Vec::new();
    for control in message.cmsgs()? {
        if let ControlMessageOwned::ScmRights(received) = control {
            descriptors.extend(received);
        }
    }
    let Some(descriptor) = descriptors.first().copied() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "the socket sent no mount namespace descriptor",
        ));
    };
    for extra in &descriptors[1..] {
        let _ = unistd::close(*extra);
    }
    let label = fs::read_link(format!("/proc/self/fd/{descriptor}"))?;
    let namespace = File::open(format!("/proc/self/fd/{descriptor}"))?;
    let _ = unistd::close(descriptor);
    if !label.to_string_lossy().starts_with("mnt:") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("the socket sent {label:?}, not a mount namespace"),
        ));
    }
    Ok(namespace)
}
