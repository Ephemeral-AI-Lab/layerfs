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
    net::TcpListener,
    os::fd::AsFd,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
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
    let listener = TcpListener::bind(env("LAYERFS_LISTEN")?)?;
    listener.set_nonblocking(true)?;
    eprintln!("layerfs-service ready {}", listener.local_addr()?);
    let runtime = super::config::telemetry(1);
    let service = Arc::new(Service::new(
        vec![StoreAccess {
            id: 1,
            store,
            grants,
        }],
        runtime.recorder(),
    )?);
    let peers = Arc::new(peers);
    let stdin = io::stdin();
    let mut threads: Vec<thread::JoinHandle<()>> = Vec::with_capacity(MAX_SESSIONS);
    loop {
        let mut i = 0;
        while i < threads.len() {
            if threads[i].is_finished() {
                let handle = threads.swap_remove(i);
                let _ = handle.join();
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
        if threads.len() == MAX_SESSIONS {
            drop(stream);
            continue;
        }
        let runtime = runtime.clone();
        let service = Arc::clone(&service);
        let peers = Arc::clone(&peers);
        let handle = thread::Builder::new()
            .name("layerfs-connection".into())
            .stack_size(2 * 1024 * 1024)
            .spawn(move || {
                if let Ok(connection) = accept(stream, &private, &peers) {
                    let _ = serve(connection, |peer, r, input, output| {
                        let (result, diagnostic) = service.handle(peer, r, input, output);
                        runtime.publish(diagnostic);
                        result
                    });
                }
            })?;
        threads.push(handle);
    }
    let end = Instant::now() + Duration::from_secs(2);
    for handle in threads {
        while !handle.is_finished() && Instant::now() < end {
            thread::park_timeout(Duration::from_millis(10));
        }
        if handle.is_finished() {
            let _ = handle.join();
        }
    }
    Ok(())
}
