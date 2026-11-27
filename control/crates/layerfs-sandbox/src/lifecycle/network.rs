//! What the side that launches Sandboxes adds to their own network: the
//! address its worker listens on, so a non-loopback one (a worker on
//! `0.0.0.0`, Docker Desktop's and apple-container's) is closed to every
//! Sandbox's commands by the firewall table, and the table written once
//! when a worker starts (it is also written at every Sandbox start). A
//! loopback listener, every daemon's and `native`'s, needs no rule: a
//! command's loopback is its Sandbox's own.
use std::net::SocketAddr;
use std::sync::OnceLock;

/// The worker's listening address, set once when it starts to serve.
static LISTEN: OnceLock<SocketAddr> = OnceLock::new();

/// Records that this process is a worker listening on `address`.
pub fn serving_on(address: SocketAddr) {
    let _ = LISTEN.set(address);
}

/// The ports a Sandbox started by this process closes to its commands:
/// the worker's own port when it does not listen on loopback.
pub fn closed_ports() -> Vec<u16> {
    match LISTEN.get() {
        Some(address) if !address.ip().is_loopback() => vec![address.port()],
        _ => Vec::new(),
    }
}

/// Writes the machine's firewall table for its current pool and this
/// worker's closed port (`network::prepare`), once, as a worker starts;
/// `Ok` names the pool. Refused where the machine cannot hold Sandbox
/// networks, which each Sandbox start then reports itself.
#[cfg(target_os = "linux")]
pub fn worker_start() -> Result<String, String> {
    crate::network::prepare(&closed_ports())
        .map(|pool| pool.to_string())
        .map_err(|error| error.to_string())
}
