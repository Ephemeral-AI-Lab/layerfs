//! What both worker servers' accept loops share (the guest worker's in
//! `layerfs-api`, the host worker's in `layerfs-store`): what a server
//! answered before it stopped, which `accept` failures it outlives, and how
//! often it logs each.
use std::time::{Duration, Instant};

/// What a worker served before it stopped.
#[derive(Debug, Default)]
pub struct Served {
    /// Requests answered.
    pub requests: u64,
}

/// The name of an `accept` failure that says nothing about the listener:
/// the process or the system is out of descriptors, buffers or memory, or
/// the peer gave up before it was accepted. `None` for every other failure
/// (`EBADF`, `EINVAL`, `ENOTSOCK`: the listener is not one), which ends
/// the worker's accept loop.
pub fn passing(error: &std::io::Error) -> Option<&'static str> {
    match error.raw_os_error()? {
        libc::EMFILE => Some("EMFILE"),
        libc::ENFILE => Some("ENFILE"),
        libc::ENOBUFS => Some("ENOBUFS"),
        libc::ENOMEM => Some("ENOMEM"),
        libc::ECONNABORTED => Some("ECONNABORTED"),
        libc::EINTR => Some("EINTR"),
        _ => None,
    }
}

/// When each kind of failure was last logged, and how many of it were not.
pub struct Limited {
    /// The least time between two lines of one kind.
    pub every: Duration,
    /// Each kind's last line and how many went unlogged since.
    pub kinds: Vec<(&'static str, Instant, u64)>,
}

impl Limited {
    /// Whether a failure of `kind` is logged now: at most one line of a
    /// kind in each `every`. `Some` holds how many of that kind went
    /// unlogged since its last line.
    pub fn due(&mut self, kind: &'static str) -> Option<u64> {
        let now = Instant::now();
        match self.kinds.iter_mut().find(|(known, ..)| *known == kind) {
            None => {
                self.kinds.push((kind, now, 0));
                Some(0)
            }
            Some((_, at, unlogged)) if now.duration_since(*at) >= self.every => {
                *at = now;
                Some(std::mem::take(unlogged))
            }
            Some((_, _, unlogged)) => {
                *unlogged += 1;
                None
            }
        }
    }
}
