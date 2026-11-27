//! One observation of an existing host worker, bounded by a single deadline.
use super::endpoint::{existing_socket, host_check_peer, HostPaths};
use super::hello::{build_identity, HostHello};
use layerfs_home::disk::paths;
use layerfs_home::lock::flock;
use layerfs_home::reply::outcome::refused;
use layerfs_home::socket;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::{fs::File, io, os::unix::fs::MetadataExt, path::Path, time::Instant};

/// The observed step at which a host worker could not be reached.
#[derive(Clone, Copy)]
pub enum HostProbe {
    /// Path and metadata checks before dialing.
    Endpoint,
    /// Connection to the verified socket.
    Connect,
    /// Authentication of the kernel's peer credentials.
    Peer,
    /// Reading and binding the greeting to this home.
    Hello,
    /// The single read-only request after a compatible hello.
    Ping,
}

/// Evidence from a probe that never starts a worker or waits for a lock.
pub enum HostObservation {
    /// No control socket was present when observed.
    Absent,
    /// A socket was present but the kernel found no listener, or it never
    /// got its private mode (its worker died between bind and chmod), and
    /// no worker or start holds the endpoint's locks. The next start
    /// replaces it.
    Stale,
    /// The same socket while a worker holds the lifetime lock (it is
    /// between bind and its private mode, wedged there, or closing) or a
    /// caller holds the startup lock: nothing replaces it until they are
    /// released.
    Owned,
    /// The probe could not verify the endpoint or its answer.
    Unreachable(HostProbe, SdkRefusal),
    /// This home's worker identified a different build or protocol.
    Incompatible(SdkRefusal),
    /// The worker authenticated and reported it was draining.
    Draining(SdkValue),
    /// The worker answered its ping with its current state.
    Ready(SdkValue),
}

impl HostPaths {
    /// Verifies the existing host worker directory's ownership and modes.
    pub fn check_worker_directories(&self) -> Result<(), SdkRefusal> {
        self.verify_directories()
    }

    /// Dials the management endpoint once, authenticates it and sends one
    /// ping only after a compatible hello. An absent socket gives no evidence
    /// about a child that has yet to publish its endpoint.
    pub fn observe(&self, end: Instant) -> HostObservation {
        use HostObservation::*;
        let fault = |stage, message| Unreachable(stage, refused("Unavailable", message));
        let began = Instant::now();
        if began >= end {
            return fault(HostProbe::Endpoint, "host worker probe budget expired");
        }
        if let Err(error) = self.verify_directories() {
            return Unreachable(HostProbe::Endpoint, error);
        }
        for path in [&self.socket, &self.writes, &self.control] {
            match existing_socket(path) {
                Ok(None) if path == &self.control => return Absent,
                // Never dialled: a call treats it as absent once no worker
                // holds the lifetime lock, and the next start replaces it.
                Ok(Some(metadata)) if metadata.mode() & 0o777 != 0o600 => {
                    return self.unowned();
                }
                Ok(_) => {}
                Err(error) => return Unreachable(HostProbe::Endpoint, error),
            }
        }
        if !self.fits() {
            return fault(HostProbe::Endpoint, "host worker socket path is too long");
        }
        let stream = match socket::connect(&self.control, end) {
            Ok(stream) => stream,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Absent,
            Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
                return self.unowned()
            }
            Err(error) => {
                return Unreachable(
                    HostProbe::Connect,
                    refused(
                        "Unavailable",
                        format!("host worker control connection: {error}"),
                    ),
                )
            }
        };
        if let Err(error) = host_check_peer(&stream) {
            return Unreachable(HostProbe::Peer, error);
        }
        let hello = match Self::read_hello(&stream, Some(end)) {
            Ok(hello) => hello,
            Err(error) => return Unreachable(HostProbe::Hello, error),
        };
        if HostHello::home_generation(&hello, self).is_none() {
            return fault(
                HostProbe::Hello,
                "host worker hello does not identify this home",
            );
        }
        let build = match build_identity() {
            Ok(build) => build,
            Err(error) => return Unreachable(HostProbe::Hello, error),
        };
        let generation = match HostHello::generation_of(&hello, self, &build) {
            Ok(generation) => generation,
            Err(error) => return Incompatible(error),
        };
        match self.diagnostic_ping(&stream, &generation, began, end) {
            Ok(reply) if matches_reply(&reply, &hello, &generation) => Ready(reply),
            Ok(_) => fault(
                HostProbe::Ping,
                "host worker ping did not identify its session",
            ),
            Err(error)
                if error.code == "Unavailable"
                    && (error.detail.get_text("state") == Some("draining")
                        || error.message == "the host worker is draining") =>
            {
                Draining(hello)
            }
            Err(error) => Unreachable(HostProbe::Ping, error),
        }
    }
}

impl HostPaths {
    /// [`HostObservation::Stale`] only when nothing owns the endpoint: the
    /// startup lock, then the lifetime lock, each tried once without waiting
    /// and released at once. The startup lock comes first, so this check
    /// never takes the lifetime lock from a starting worker its caller has
    /// just spawned. A lock file that is not there is free and is not made.
    fn unowned(&self) -> HostObservation {
        let free = (|| {
            let Some(_startup) = free_lock(&paths::host_worker_startup_lock(&self.home))? else {
                return Ok(false);
            };
            Ok(free_lock(&paths::host_worker_lock(&self.home))?.is_some())
        })();
        match free {
            Ok(true) => HostObservation::Stale,
            Ok(false) => HostObservation::Owned,
            Err(error) => HostObservation::Unreachable(HostProbe::Endpoint, error),
        }
    }
}

/// A lock [`free_lock`] took, released explicitly when dropped: a child
/// forked meanwhile shares the open file description until its exec, so
/// closing the descriptor alone would leave it held (F-053).
pub(super) struct Taken(File);

impl Drop for Taken {
    fn drop(&mut self) {
        flock::unlock(&self.0);
    }
}

/// The lock at `path` taken without waiting (`None` when another holds it),
/// or no file when there is none.
pub(super) fn free_lock(path: &Path) -> Result<Option<Option<Taken>>, SdkRefusal> {
    let unavailable = |error: io::Error| {
        refused(
            "Unavailable",
            format!("host worker lock {}: {error}", path.display()),
        )
    };
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Some(None)),
        Err(error) => return Err(unavailable(error)),
        Ok(metadata) if !metadata.is_file() => {
            return Err(unavailable(io::Error::other("not a regular file")))
        }
        Ok(_) => {}
    }
    let file = File::open(path).map_err(unavailable)?;
    match flock::try_lock(&file, true).map_err(unavailable)? {
        true => Ok(Some(Some(Taken(file)))),
        false => Ok(None),
    }
}

fn matches_reply(reply: &SdkValue, hello: &SdkValue, generation: &str) -> bool {
    reply.get_text("state") == Some("ready")
        && reply.get_text("generation") == Some(generation)
        && ["home_id", "build"]
            .iter()
            .all(|key| reply.get_text(key) == hello.get_text(key))
}
