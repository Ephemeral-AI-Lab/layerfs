//! The single bounded native connection acceptor.
//!
//! Both entry points use this implementation: the operator process watches its
//! own stdin for shutdown, while a composed [`crate::Server`] listens until the
//! caller clears its stop flag. Sessions are bounded by the Store's own write
//! budget, and a closing acceptor interrupts its live owners before returning.
use crate::Service;
use layerfs_bridge::{
    adapters::native::{
        connection::{accept, Peer},
        server::serve,
    },
    contract::*,
};
use layerfs_telemetry::runtime::Runtime;
use nix::poll::{poll, PollFd, PollFlags};
use std::{
    io,
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    os::fd::AsFd,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

/// One session can carry one operation, so the transport admits the Store's
/// whole write budget plus the service read bound. A smaller cap would be an
/// accidental ceiling under the configured writer setting.
pub(crate) fn session_capacity(max_concurrent_writes: u8) -> usize {
    layerfs_bridge::contract::session_capacity(max_concurrent_writes)
}

/// How the acceptor learns that admission must stop.
pub(crate) enum Stop<'a> {
    /// The operator process stops when its stdin reaches EOF.
    Stdin,
    /// A composed Server stops when the flag is set.
    Flag(&'a AtomicBool),
}

struct Session {
    socket: TcpStream,
    worker: thread::JoinHandle<()>,
}

pub(crate) struct Acceptor {
    listener: TcpListener,
    capacity: usize,
    private: [u8; 32],
    peers: Vec<Peer>,
    service: Arc<Service>,
    runtime: Runtime,
}

impl Acceptor {
    pub(crate) fn bind(
        endpoint: SocketAddr,
        capacity: usize,
        private: [u8; 32],
        peers: Vec<Peer>,
        service: Arc<Service>,
        runtime: Runtime,
    ) -> Result<Self, Failure> {
        let listener = layerfs_bridge::adapters::native::listen(endpoint)?;
        listener.set_nonblocking(true)?;
        Ok(Self {
            listener,
            capacity,
            private,
            peers,
            service,
            runtime,
        })
    }

    pub(crate) fn endpoint(&self) -> Result<SocketAddr, Failure> {
        self.listener.local_addr().map_err(|_| Code::Io.into())
    }

    /// Admit connections until `stop` reports that admission must end.
    pub(crate) fn serve(&self, stop: Stop<'_>) -> Result<(), Failure> {
        let stdin = io::stdin();
        let mut sessions: Vec<Session> = Vec::with_capacity(self.capacity);
        let started = Instant::now();
        let (mut accepted, mut admitted, mut reaped, mut capacity_dropped, mut peak_live) =
            (0u64, 0u64, 0u64, 0u64, 0usize);
        let result = (|| {
            loop {
                if let Stop::Flag(flag) = &stop {
                    if flag.load(Ordering::Acquire) {
                        break;
                    }
                }
                let mut i = 0;
                while i < sessions.len() {
                    if sessions[i].worker.is_finished() {
                        let session = sessions.swap_remove(i);
                        let _ = session.worker.join();
                        reaped = reaped.saturating_add(1);
                    } else {
                        i += 1;
                    }
                }
                let mut fds = [
                    PollFd::new(self.listener.as_fd(), PollFlags::POLLIN),
                    PollFd::new(stdin.as_fd(), PollFlags::POLLIN),
                ];
                let count = if matches!(stop, Stop::Stdin) { 2 } else { 1 };
                poll(&mut fds[..count], 100u16).map_err(|_| Code::Io)?;
                if count == 2
                    && fds[1]
                        .revents()
                        .is_some_and(|f| f.intersects(PollFlags::POLLIN | PollFlags::POLLHUP))
                {
                    break;
                }
                if !fds[0]
                    .revents()
                    .is_some_and(|f| f.contains(PollFlags::POLLIN))
                {
                    continue;
                }
                let (stream, _) = match self.listener.accept() {
                    Ok(v) => v,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                    Err(_) => return Err(Code::Io.into()),
                };
                accepted = accepted.saturating_add(1);
                if sessions.len() == self.capacity {
                    capacity_dropped = capacity_dropped.saturating_add(1);
                    if capacity_dropped <= 8 {
                        layerfs_bridge::adapters::native::pipe::diagnostic(&format!(
                            "layerfs-server acceptor drop at_ms={} live={} capacity={} dropped={}\n",
                            started.elapsed().as_millis(), sessions.len(), self.capacity, capacity_dropped
                        ));
                    }
                    drop(stream);
                    continue;
                }
                let runtime = self.runtime.clone();
                let service = Arc::clone(&self.service);
                let peers = Arc::clone(&Arc::new(self.peers.clone()));
                let private = self.private;
                let socket = stream.try_clone()?;
                let handle = thread::Builder::new()
                    .name("layerfs-connection".into())
                    .stack_size(2 * 1024 * 1024)
                    .spawn(move || {
                        if let Ok(connection) = accept(stream, &private, &peers) {
                            let _ = serve(connection, |peer, request, input, output, deadline| {
                                let (result, diagnostic) =
                                    service.handle_until(peer, request, input, output, deadline);
                                runtime.publish(diagnostic);
                                result
                            });
                        }
                    })?;
                sessions.push(Session {
                    socket,
                    worker: handle,
                });
                admitted = admitted.saturating_add(1);
                peak_live = peak_live.max(sessions.len());
            }
            Ok(())
        })();
        let live_at_stop = sessions.len();
        // Stop admission, then interrupt handshake/input/output on every live
        // owner. Shutdown clones share sockets; they are not extra slots.
        for session in &sessions {
            let _ = session.socket.shutdown(Shutdown::Both);
        }
        let end = Instant::now() + Duration::from_secs(2);
        for session in &sessions {
            while !session.worker.is_finished() && Instant::now() < end {
                thread::park_timeout(Duration::from_millis(10));
            }
            if !session.worker.is_finished() {
                layerfs_bridge::adapters::native::pipe::diagnostic(
                    "layerfs-server: shutdown expired; operation outcomes unresolved\n",
                );
                // This is executable assembly. Keep all owners until explicit
                // process exit; detaching a worker is not completion or proof
                // of rollback.
                std::process::exit(1);
            }
        }
        for session in sessions {
            let _ = session.worker.join();
            reaped = reaped.saturating_add(1);
        }
        layerfs_bridge::adapters::native::pipe::diagnostic(&format!(
            "layerfs-server acceptor summary accepted={accepted} admitted={admitted} live_at_stop={live_at_stop} peak_live={peak_live} reaped={reaped} capacity_dropped={capacity_dropped} capacity={} elapsed_ms={}\n",
            self.capacity,
            started.elapsed().as_millis()
        ));
        result
    }
}
