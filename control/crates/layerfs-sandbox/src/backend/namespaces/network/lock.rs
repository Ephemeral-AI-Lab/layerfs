//! The lock around choosing a slot and making its host end: an abstract
//! Unix socket, `@layerfs/network`. Abstract names belong to a network
//! namespace, exactly the scope of the addresses it guards, and they reach
//! across filesystems (the host and a Linux Engine worker container share
//! the host's namespace but not its `/run`). The holder binds and listens;
//! a waiter connects and reads until the holder's socket closes, then binds
//! again. The kernel releases it when the holder ends, so a crash leaves no
//! lock behind. Waiting is bounded by the caller's deadline.
use std::io::{self, Read};
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};
use std::time::{Duration, Instant};

const NAME: &[u8] = b"layerfs/network";
/// The pause after a holder that went between the two calls.
const PAUSE: Duration = Duration::from_millis(1);

/// The held lock; dropping it releases it.
pub struct NetworkLock(#[allow(dead_code)] UnixListener);

impl NetworkLock {
    /// Takes the lock, waiting at most `wait` for another holder.
    pub fn take(wait: Duration) -> io::Result<Self> {
        let address = SocketAddr::from_abstract_name(NAME)?;
        let deadline = Instant::now() + wait;
        loop {
            match UnixListener::bind_addr(&address) {
                Ok(listener) => return Ok(Self(listener)),
                Err(error) if error.kind() == io::ErrorKind::AddrInUse => {}
                Err(error) => return Err(error),
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "another Sandbox start held this machine's network lock",
                ));
            }
            match UnixStream::connect_addr(&address) {
                Ok(mut stream) => {
                    stream.set_read_timeout(Some(left))?;
                    let mut byte = [0u8; 1];
                    // end of file or a reset: the holder let go; a time-out
                    // ends at the deadline check above
                    let _ = stream.read(&mut byte);
                }
                Err(_) => std::thread::sleep(PAUSE),
            }
        }
    }
}
