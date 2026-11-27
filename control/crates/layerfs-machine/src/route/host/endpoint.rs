//! The host worker's endpoint custody: its canonical home and three
//! endpoints ([`HostPaths`]), the lifetime lock ([`HostGuard`]), binding a
//! private endpoint and the peer check.
use layerfs_home::disk::{self, paths};
use layerfs_home::lock::flock;
use layerfs_home::reply::outcome::refused;
use layerfs_home::socket;
use layerfs_protocol::sdk::SdkRefusal;
use std::fs::{self, File};
use std::io;
use std::os::unix::{
    fs::{FileTypeExt, MetadataExt},
    net::{UnixListener, UnixStream},
};
use std::path::{Path, PathBuf};

/// Which of the worker's endpoints a caller dials.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Endpoint {
    Ordinary,
    Control,
    /// Streamed file writes only (S2.2).
    Writes,
}

impl Endpoint {
    pub(super) fn of(control: bool) -> Self {
        if control {
            Self::Control
        } else {
            Self::Ordinary
        }
    }
}

/// Canonical home authority and its three endpoints.
pub struct HostPaths {
    pub(super) cli: Option<(PathBuf, String)>,
    /// Canonical pathname of the existing home.
    pub home: PathBuf,
    /// Identity from the authoritative home record.
    pub home_id: String,
    /// Ordinary request endpoint.
    pub socket: PathBuf,
    /// Management endpoint with independent admission.
    pub control: PathBuf,
    /// Streamed write endpoint with independent admission.
    pub writes: PathBuf,
}

/// Ownership of the stable lifetime lock. Drop only after removing the sockets.
pub struct HostGuard {
    lock: File,
    directory: File,
    home: PathBuf,
}

/// Accepts only this process's user, before greeting or sending.
pub fn host_check_peer(stream: &UnixStream) -> Result<(), SdkRefusal> {
    let uid = socket::peer_uid(stream).map_err(|error| unavailable("peer credentials", error))?;
    if uid != flock::euid() {
        return Err(refused(
            "Denied",
            "host worker peer has a different effective UID",
        ));
    }
    Ok(())
}

pub(super) fn unavailable(action: &str, error: impl std::fmt::Display) -> SdkRefusal {
    refused(
        "Unavailable",
        format!("host worker {action}: {error}; no operation was sent"),
    )
}

pub(super) fn private_directory(path: &Path) -> Result<(), SdkRefusal> {
    let metadata = fs::symlink_metadata(path).map_err(|error| unavailable("directory", error))?;
    // Passing through without listing is permitted for a namespaces Sandbox.
    if !metadata.is_dir() || metadata.uid() != flock::euid() || metadata.mode() & 0o066 != 0 {
        return Err(refused(
            "Denied",
            format!(
                "{} is not a private directory owned by this user",
                path.display()
            ),
        ));
    }
    Ok(())
}

pub(super) fn existing_socket(path: &Path) -> Result<Option<fs::Metadata>, SdkRefusal> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(unavailable("socket metadata", error)),
    };
    if !metadata.file_type().is_socket() || metadata.uid() != flock::euid() {
        return Err(refused(
            "Denied",
            format!("{} is not a socket owned by this user", path.display()),
        ));
    }
    Ok(Some(metadata))
}

pub(super) fn hold(path: &Path) -> Result<Option<File>, SdkRefusal> {
    let file =
        socket::private_file(path, false).map_err(|error| unavailable("lock file", error))?;
    flock::try_lock(&file, true)
        .map(|acquired| acquired.then_some(file))
        .map_err(|error| unavailable("lock", error))
}

impl HostPaths {
    /// Reads an open home's identity without creating or changing any entry.
    pub fn new(home: &Path) -> Result<Self, SdkRefusal> {
        let canonical = fs::canonicalize(home).map_err(|error| unavailable("home", error))?;
        let file = match disk::detect(&canonical)? {
            disk::Layout::Current(file) => file,
            _ => {
                return Err(refused(
                    "Unavailable",
                    "host worker needs an open LayerFS home",
                ))
            }
        };
        Ok(Self {
            cli: None,
            socket: paths::host_worker_socket(&canonical),
            control: paths::host_worker_control(&canonical),
            writes: paths::host_worker_writes(&canonical),
            home_id: file.id,
            home: canonical,
        })
    }

    /// Pre-dial routing check. Every endpoint name must fit the kernel limit.
    pub fn fits(&self) -> bool {
        self.endpoints().iter().all(|path| socket::path_fits(path))
    }

    /// The ordinary, write and management endpoints.
    pub fn endpoints(&self) -> [&Path; 3] {
        [&self.socket, &self.writes, &self.control]
    }

    pub(super) fn verify_directories(&self) -> Result<(), SdkRefusal> {
        private_directory(&self.home)?;
        private_directory(&self.home.join("machine"))?;
        private_directory(&paths::machine_dir(
            &self.home,
            layerfs_home::names::HOST_MACHINE,
        ))
    }

    /// Takes the worker lifetime lock without waiting. The existing Home
    /// supplies the private native machine directory.
    pub fn try_hold(&self) -> Result<Option<HostGuard>, SdkRefusal> {
        self.check_paths()?;
        let directory = File::open(paths::machine_dir(
            &self.home,
            layerfs_home::names::HOST_MACHINE,
        ))
        .map_err(|error| unavailable("directory", error))?;
        let Some(lock) = hold(&paths::host_worker_lock(&self.home))? else {
            return Ok(None);
        };
        let guard = HostGuard {
            lock,
            directory,
            home: self.home.clone(),
        };
        self.check_guard(&guard)?;
        Ok(Some(guard))
    }

    pub(super) fn check_paths(&self) -> Result<(), SdkRefusal> {
        if !self.fits() {
            return Err(refused(
                "Unavailable",
                "host worker socket path is too long",
            ));
        }
        self.verify_directories()
    }

    fn check_guard(&self, guard: &HostGuard) -> Result<(), SdkRefusal> {
        if self.home != guard.home {
            return Err(refused(
                "Denied",
                "host worker lock belongs to another home",
            ));
        }
        self.check_paths()?;
        let current_home = disk::opened(&self.home)?;
        let same = |file: &File, path: PathBuf| {
            let held = file
                .metadata()
                .map_err(|error| unavailable("held lock", error))?;
            let current =
                fs::symlink_metadata(path).map_err(|error| unavailable("lock pathname", error))?;
            Ok::<_, SdkRefusal>(held.dev() == current.dev() && held.ino() == current.ino())
        };
        if self.home_id != current_home.id
            || !same(
                &guard.directory,
                paths::machine_dir(&self.home, layerfs_home::names::HOST_MACHINE),
            )?
            || !same(&guard.lock, paths::host_worker_lock(&self.home))?
        {
            return Err(refused(
                "Unavailable",
                "host worker home or lock identity changed",
            ));
        }
        Ok(())
    }

    /// Recovers a stale socket and binds a private endpoint while holding
    /// its home's worker lifetime lock.
    pub fn bind(&self, control: bool, guard: &HostGuard) -> Result<UnixListener, SdkRefusal> {
        self.bind_at(Endpoint::of(control), guard)
    }

    /// [`Self::bind`] for the streamed write endpoint.
    pub fn bind_writes(&self, guard: &HostGuard) -> Result<UnixListener, SdkRefusal> {
        self.bind_at(Endpoint::Writes, guard)
    }

    fn bind_at(&self, endpoint: Endpoint, guard: &HostGuard) -> Result<UnixListener, SdkRefusal> {
        self.check_guard(guard)?;
        let path = self.endpoint(endpoint);
        let name = path
            .file_name()
            .ok_or_else(|| unavailable("bind", "missing socket name"))?;
        socket::remove_socket(&guard.directory, name)
            .map_err(|error| unavailable("remove stale socket", error))?;
        let listener = UnixListener::bind(path).map_err(|error| unavailable("bind", error))?;
        self.check_guard(guard)?;
        if let Err(error) = socket::set_socket_private(&guard.directory, name) {
            let _ = socket::remove_socket(&guard.directory, name);
            return Err(unavailable("socket permissions", error));
        }
        Ok(listener)
    }

    /// Removes every socket file under the lifetime lock after listeners close.
    pub fn remove_endpoints(&self, guard: &HostGuard) -> Result<(), SdkRefusal> {
        self.check_guard(guard)?;
        let remove = |path: &Path| {
            let name = path
                .file_name()
                .ok_or_else(|| unavailable("cleanup", "missing socket name"))?;
            socket::remove_socket(&guard.directory, name)
                .map_err(|error| unavailable("remove socket", error))
        };
        let ordinary = remove(&self.socket);
        let writes = remove(&self.writes);
        let management = remove(&self.control);
        ordinary.and(writes).and(management)
    }

    pub(super) fn endpoint(&self, endpoint: Endpoint) -> &Path {
        match endpoint {
            Endpoint::Ordinary => &self.socket,
            Endpoint::Control => &self.control,
            Endpoint::Writes => &self.writes,
        }
    }
}
