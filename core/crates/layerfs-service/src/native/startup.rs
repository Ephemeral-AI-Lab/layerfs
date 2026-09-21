//! Native assembly owns configured keys, Store paths and bounded connection threads.
use crate::{Grant, Service, StoreAccess};
use layerfs_bridge::{
    adapters::native::{
        connection::{accept, Peer},
        pipe::key,
        server::serve,
    },
    contract::*,
};
use layerfs_storage::Store;
use layerfs_telemetry::timer::Timing;
use nix::poll::{poll, PollFd, PollFlags};
use std::{
    io,
    net::{Shutdown, TcpStream},
    os::fd::AsFd,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
struct Session {
    socket: TcpStream,
    worker: thread::JoinHandle<()>,
}
fn env(name: &str) -> Result<String, Failure> {
    let value = std::env::var(name).map_err(|_| Code::InvalidInput)?;
    if value.len() > 4096 {
        return Err(Code::Capacity.into());
    }
    Ok(value)
}
pub fn run() -> Result<(), Failure> {
    let private = key(&env("LAYERFS_PRIVATE_KEY")?)?;
    let mut peers = Vec::new();
    let mut grants = Vec::new();
    for entry in env("LAYERFS_PEERS")?.split(';') {
        if peers.len() == 16 {
            return Err(Code::Capacity.into());
        }
        let values: Vec<_> = entry.split(',').collect();
        if values.len() != 4 {
            return Err(Code::InvalidInput.into());
        }
        let selector = values[0].parse::<u32>().map_err(|_| Code::InvalidInput)?;
        let public = key(values[1])?;
        let expires_unix = values[2].parse().map_err(|_| Code::InvalidInput)?;
        let operations = values[3].parse().map_err(|_| Code::InvalidInput)?;
        if peers.iter().any(|p: &Peer| p.selector == selector) {
            return Err(Code::InvalidInput.into());
        }
        peers.push(Peer {
            selector,
            public,
            expires_unix,
        });
        grants.push(Grant {
            public_key: public,
            operations,
            expires_unix,
        });
    }
    let path = env("LAYERFS_STORE")?;
    let store = Timing::disabled("open", |s| Store::open(path, s.child("open")))
        .0
        .map_err(crate::operation::failure::storage)?;
    // One session can carry one operation, so the transport admits this Store's
    // whole write budget plus the service read bound. A smaller cap would be an
    // accidental ceiling under the configured writer setting.
    let sessions_capacity = session_capacity(
        store
            .max_concurrent_writes()
            .map_err(crate::operation::failure::storage)?,
    );
    let listener = layerfs_bridge::adapters::native::listen(
        env("LAYERFS_LISTEN")?
            .parse()
            .map_err(|_| Code::InvalidInput)?,
    )?;
    listener.set_nonblocking(true)?;
    layerfs_bridge::adapters::native::pipe::diagnostic(&format!(
        "layerfs-service ready {}\n",
        listener.local_addr()?
    ));
    let runtime = super::config::telemetry(1);
    let service = Arc::new(Service::new(
        vec![StoreAccess {
            id: 1,
            store,
            grants,
            history: super::config::history()?,
        }],
        runtime.recorder(),
    )?);
    let peers = Arc::new(peers);
    let stdin = io::stdin();
    let mut sessions: Vec<Session> = Vec::with_capacity(sessions_capacity);
    let result = (|| {
        loop {
            let mut i = 0;
            while i < sessions.len() {
                if sessions[i].worker.is_finished() {
                    let session = sessions.swap_remove(i);
                    let _ = session.worker.join();
                } else {
                    i += 1;
                }
            }
            let mut fds = [
                PollFd::new(listener.as_fd(), PollFlags::POLLIN),
                PollFd::new(stdin.as_fd(), PollFlags::POLLIN),
            ];
            poll(&mut fds, 100u16).map_err(|_| Code::Io)?;
            if fds[1]
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
            let (stream, _) = match listener.accept() {
                Ok(v) => v,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                Err(_) => return Err(Code::Io.into()),
            };
            if sessions.len() == sessions_capacity {
                drop(stream);
                continue;
            }
            let runtime = runtime.clone();
            let service = Arc::clone(&service);
            let peers = Arc::clone(&peers);
            let socket = stream.try_clone()?;
            let handle = thread::Builder::new()
                .name("layerfs-connection".into())
                .stack_size(2 * 1024 * 1024)
                .spawn(move || {
                    if let Ok(connection) = accept(stream, &private, &peers) {
                        let _ = serve(connection, |peer, r, input, output, deadline| {
                            let (result, diagnostic) =
                                service.handle_until(peer, r, input, output, deadline);
                            runtime.publish(diagnostic);
                            result
                        });
                    }
                })?;
            sessions.push(Session {
                socket,
                worker: handle,
            });
        }
        Ok(())
    })();
    // Stop admission, then interrupt handshake/input/output on every live owner.
    // Shutdown clones share sockets; they are not additional connection slots.
    drop(listener);
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
                "layerfs-service: shutdown expired; operation outcomes unresolved\n",
            );
            // This is executable assembly. Keep all owners until explicit process
            // exit; detaching a worker is not completion or proof of rollback.
            std::process::exit(1);
        }
    }
    for session in sessions {
        let _ = session.worker.join();
    }
    result
}
