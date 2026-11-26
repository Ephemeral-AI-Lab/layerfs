//! One observation of an existing host worker, bounded by a single deadline.
use super::host::{existing_socket, host_check_peer, HostPaths};
use super::host_hello::{build_identity, HostHello};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::{home::host_socket, outcome::refused};
use std::{io, os::unix::fs::MetadataExt, time::Instant};

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

/// Evidence from a probe that never starts a worker or takes a lock.
pub enum HostObservation {
    /// No control socket was present when observed.
    Absent,
    /// A socket was present but the kernel found no listener.
    Stale,
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
        for path in [&self.socket, &self.control] {
            match existing_socket(path) {
                Ok(None) if path == &self.control => return Absent,
                Ok(Some(metadata)) if metadata.mode() & 0o777 != 0o600 => {
                    return Unreachable(
                        HostProbe::Endpoint,
                        refused("Denied", format!("{} must have mode 0600", path.display())),
                    )
                }
                Ok(_) => {}
                Err(error) => return Unreachable(HostProbe::Endpoint, error),
            }
        }
        if !self.fits() {
            return fault(HostProbe::Endpoint, "host worker socket path is too long");
        }
        let stream = match host_socket::connect(&self.control, end) {
            Ok(stream) => stream,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Absent,
            Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => return Stale,
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
        let hello = match Self::read_hello(&stream, end) {
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

fn matches_reply(reply: &SdkValue, hello: &SdkValue, generation: &str) -> bool {
    reply.get_text("state") == Some("ready")
        && reply.get_text("generation") == Some(generation)
        && ["home_id", "build"]
            .iter()
            .all(|key| reply.get_text(key) == hello.get_text(key))
}
