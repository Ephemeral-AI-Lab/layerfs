//! One deadline a control connection cannot outlive (#540, #543).
//!
//! A bounded read that only sets socket timeouts bounds each read and write
//! system call, not the exchange: a daemon that trickles one byte just
//! inside the timeout can hold the reader, its connection and its pool slot
//! far past the time the caller promised. A fence instead holds a clone of
//! the connection's socket and shuts both directions down at one absolute
//! deadline, which ends the blocked call immediately. The doctor's bounded
//! observation (`doctor/sandbox_channel.rs` outside this crate) uses the same
//! pattern.
use layerfs_bridge::duplex::CloseHandle;
use std::net::{Shutdown, TcpStream};
use std::sync::mpsc;
use std::time::Instant;

/// What this fence ends at its deadline: a socket cloned from a connection
/// being opened, or the application-owned close handle of an established
/// channel.
enum End {
    Socket(TcpStream),
    Channel(CloseHandle),
}

impl End {
    fn shut(&self) {
        match self {
            Self::Socket(socket) => {
                let _ = socket.shutdown(Shutdown::Both);
            }
            // the handle quarantines the channel itself, and the pool reads
            // that flag, so it closes the connection instead of lending it
            // again
            Self::Channel(handle) => {
                let _ = handle.close();
            }
        }
    }
}

/// A socket fence, armed at one absolute deadline; its `Drop` cancels the
/// deadline and joins the thread that waits for it.
pub struct Fence {
    stop: mpsc::Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Fence {
    /// Arms `socket` (a clone of the connection's) at `deadline`.
    pub fn on(socket: TcpStream, deadline: Instant) -> Self {
        Self::armed(End::Socket(socket), deadline)
    }

    /// Arms established channel `handle` at `deadline`.
    pub fn channel(handle: CloseHandle, deadline: Instant) -> Self {
        Self::armed(End::Channel(handle), deadline)
    }

    fn armed(end: End, deadline: Instant) -> Self {
        let (stop, wait) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            if matches!(
                wait.recv_timeout(deadline.saturating_duration_since(Instant::now())),
                Err(mpsc::RecvTimeoutError::Timeout)
            ) {
                end.shut();
            }
        });
        Self {
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for Fence {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
