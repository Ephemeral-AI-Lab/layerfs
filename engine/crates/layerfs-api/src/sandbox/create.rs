//! Ordinary Sandbox startup, original deployment inputs and failure custody.
use crate::{internal::Control, OperationFailure};
use layerfs_bridge::control::{ControlError, DaemonStatus, HelloRequest, Reply, Request};
use layerfs_bridge::duplex::ChannelError;
use layerfs_sandbox::Runtime;
use std::{io, sync::Arc};

/// The address to dial for a daemon `endpoint` observed on the Docker host's
/// loopback: the endpoint itself, or the port on `host` (one resolution, its
/// first address; nothing is tried twice).
pub fn connect_address(
    endpoint: std::net::SocketAddr,
    host: Option<&str>,
) -> io::Result<std::net::SocketAddr> {
    let Some(host) = host else {
        return Ok(endpoint);
    };
    std::net::ToSocketAddrs::to_socket_addrs(&(host, endpoint.port()))?
        .next()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("daemon host {host} has no address"),
            )
        })
}

/// Concrete boundary reached by one original lifecycle operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SandboxPhase {
    /// Local validation, no container effect.
    Validate,
    /// Caller executable open/metadata.
    Executable,
    /// Volume observation and original Create.
    Create,
    /// Original stopped-container archive extraction.
    Upload,
    /// Original container Start.
    Start,
    /// Actual original stdout listener event wait.
    Listener,
    /// Exact container endpoint observation.
    Endpoint,
    /// One TCP connection attempt.
    Connect,
    /// One expected-peer duplex authentication.
    Authenticate,
    /// Clears only explicit handshake waits after authentication; owner remains retained on failure.
    Policy,
    /// One correlated Hello through the retained Control owner.
    Hello,
    /// One original sealed Project installation.
    Install,
    /// One explicit whole-Sandbox stop attempt.
    Stop,
    /// One delete attempt after acknowledged stop.
    Delete,
}

/// Exact original failure, independent of owned resource disposition.
#[derive(Debug)]
pub enum SandboxCause {
    /// Local filesystem/socket error.
    Io(io::Error),
    /// Local config or correlated startup validation.
    Setup(ControlError),
    /// Original Engine transfer, no replay.
    Runtime(Box<layerfs_sandbox::WireFailure>),
    /// Original provider launch cause; partial runtime ownership is retained separately.
    Provider(Box<layerfs_sandbox::provider::LaunchFailure>),
    /// Original authentication failure; handshake work remains in the receipt.
    Channel(ChannelError),
    /// Original correlated daemon control exchange/refusal.
    Control(Box<OperationFailure>),
    /// Original streaming installation failure and exact publication knowledge.
    Install(Box<crate::InstallFailure>),
}

/// Actual local Engine and authenticated daemon lifecycle organization.
#[derive(Clone)]
pub struct SandboxApi {
    pub(crate) runtime: Arc<Runtime>,
}

pub(super) fn hello(
    control: &mut Control,
    request: HelloRequest,
) -> Result<DaemonStatus, Box<OperationFailure>> {
    let request = Request::Hello(request);
    match crate::base::failure::exchange(control, request.clone())? {
        Reply::Hello(status) => Ok(status),
        reply => Err(OperationFailure::unexpected(request, reply)),
    }
}
