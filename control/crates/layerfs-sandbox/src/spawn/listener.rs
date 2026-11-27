//! The daemon's startup line: a bounded wait for the loopback address it
//! prints once it listens. Both local backends spawn a daemon whose output is
//! a log file, so both read that same marker from it, each look taking only
//! what was appended since the last one. Nothing is retried: the wait ends at
//! the marker, at the daemon's exit, or at the caller's deadline.
use crate::RuntimeError;
use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
    time::{Duration, Instant},
};

/// The daemon's startup line; the address follows it.
pub(crate) const MARKER: &[u8] = b"LAYERFS_DAEMON_LISTEN ";
/// Pause between two reads of a starting daemon's output.
const PAUSE: Duration = Duration::from_millis(10);

/// Waits until `log` names the daemon's loopback address, the daemon is no
/// longer alive (as `alive` reports), or `wait` ends.
pub(crate) fn await_listener(
    log: &Path,
    wait: Duration,
    alive: &dyn Fn() -> bool,
) -> Result<u16, RuntimeError> {
    let deadline = Instant::now() + wait;
    let mut file = File::open(log)?;
    let mut offset = 0u64;
    let mut line = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        file.seek(SeekFrom::Start(offset))?;
        loop {
            let n = file.read(&mut chunk)?;
            if n == 0 {
                break;
            }
            offset += n as u64;
            for &byte in &chunk[..n] {
                if byte != b'\n' {
                    // a startup line is short; a longer one is not the marker
                    if line.len() < 256 {
                        line.push(byte);
                    }
                    continue;
                }
                if let Some(address) = line.strip_prefix(MARKER) {
                    let address = std::str::from_utf8(address)
                        .ok()
                        .and_then(|text| text.parse::<std::net::SocketAddr>().ok())
                        .filter(|a| a.ip().is_loopback() && a.port() != 0)
                        .ok_or(RuntimeError::Protocol("local daemon loopback listener"))?;
                    return Ok(address.port());
                }
                line.clear();
            }
        }
        if !alive() {
            return Err(RuntimeError::Protocol(
                "local daemon exited before its listener line",
            ));
        }
        if Instant::now() >= deadline {
            return Err(io::Error::from(io::ErrorKind::TimedOut).into());
        }
        std::thread::sleep(PAUSE);
    }
}
