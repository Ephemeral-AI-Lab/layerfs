//! A TCP proxy between a Store's PostgreSQL sessions and the server that
//! counts every message the server sends while armed and can inject one
//! fault at message `N`:
//!
//! * [`Fault::Drop`]: message `N` and everything after it are discarded and
//!   both sockets of that connection are shut: the server sees its client
//!   vanish, the client sees the connection end without the reply.
//! * [`Fault::HoldKill`]: from message `N` on, that connection's replies are
//!   held (the server keeps running whatever the client already sent), the
//!   backend is terminated with `pg_terminate_backend` from a separate,
//!   direct connection, and once it is gone (bounded) the held bytes are
//!   delivered in order, followed by whatever the server sent before it
//!   closed: a network delay that outlives the backend.
//! * [`Fault::HoldKillOther`]: as `HoldKill`, but the backend terminated is
//!   that of the other connection this window saw first (for a Commit: the
//!   lane connection while the writer works inside the lane).
//! * [`Fault::Pause`]: from message `N` on, that connection's replies are
//!   held until the test calls [`FaultProxy::release`] (or 30 s pass), then
//!   delivered in order: the client is stopped at a chosen point while the
//!   test acts on the server from elsewhere.
//!
//! Messages are the protocol's own (type byte and length); the startup of
//! a connection opened while armed counts too. The backend's pid comes from
//! its `BackendKeyData`. Nothing here is product code; the proxy listens on
//! 127.0.0.1 only.
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Fault {
    Drop,
    HoldKill,
    HoldKillOther,
    Pause,
}

/// Where the armed fault fired.
#[derive(Clone, Debug)]
pub struct Fired {
    pub at: u64,
    pub connection: usize,
    pub kind: char,
    pub pid: Option<i32>,
    /// HoldKill: whether the backend was seen gone within the bound.
    pub killed: Option<bool>,
}

/// What one armed window saw.
#[derive(Clone, Debug, Default)]
pub struct Window {
    /// Server messages counted.
    pub seen: u64,
    /// Per counted message: its connection and type byte.
    pub log: Vec<(usize, char)>,
    pub fired: Option<Fired>,
}

#[derive(Default)]
struct State {
    armed: bool,
    target: Option<(u64, Fault)>,
    window: Window,
    pids: Vec<Option<i32>>,
    released: bool,
}

pub struct FaultProxy {
    pub port: u16,
    shared: Arc<Mutex<State>>,
}

/// The proxy's connection string for the server `url` names.
pub fn proxied_url(url: &str, port: u16) -> (String, SocketAddr) {
    let config = layerfs_persistence::PostgresConfig::from_str(url).unwrap();
    let host = match &config.get_hosts()[0] {
        layerfs_persistence::PostgresHost::Tcp(host) => host.clone(),
        _ => panic!("the test server must be reached over TCP"),
    };
    let upstream = (
        host.as_str(),
        config.get_ports().first().copied().unwrap_or(5432),
    )
        .to_socket_addrs()
        .unwrap()
        .next()
        .unwrap();
    let mut proxied = format!(
        "host=127.0.0.1 port={port} user={} dbname={} sslmode=disable",
        config.get_user().unwrap(),
        config.get_dbname().unwrap_or(config.get_user().unwrap()),
    );
    if let Some(password) = config.get_password() {
        proxied.push_str(&format!(
            " password={}",
            String::from_utf8(password.to_vec()).unwrap()
        ));
    }
    (proxied, upstream)
}

impl FaultProxy {
    /// Starts a proxy to the server `url` names; returns it and the URL to
    /// dial through it. `admin` is the direct URL the killer connects with.
    pub fn start(url: &str) -> (Self, String) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (proxied, upstream) = proxied_url(url, port);
        let shared: Arc<Mutex<State>> = Arc::default();
        let state = shared.clone();
        let admin = url.to_owned();
        std::thread::spawn(move || {
            for client in listener.incoming() {
                let Ok(client) = client else { return };
                let Ok(server) = TcpStream::connect(upstream) else {
                    return;
                };
                let _ = client.set_nodelay(true);
                let _ = server.set_nodelay(true);
                let connection = {
                    let mut state = state.lock().unwrap();
                    state.pids.push(None);
                    state.pids.len() - 1
                };
                let (up_from, up_to) = (client.try_clone().unwrap(), server.try_clone().unwrap());
                std::thread::spawn(move || forward(up_from, up_to));
                let state = state.clone();
                let admin = admin.clone();
                std::thread::spawn(move || back(server, client, state, connection, &admin));
            }
        });
        (Self { port, shared }, proxied)
    }

    /// Starts counting from zero; `target` fires once at its message.
    pub fn arm(&self, target: Option<(u64, Fault)>) {
        let mut state = self.shared.lock().unwrap();
        state.armed = true;
        state.target = target;
        state.window = Window::default();
        state.released = false;
    }

    /// Waits (bounded) until the armed fault fired.
    pub fn wait_fired(&self, limit: Duration) -> bool {
        let until = Instant::now() + limit;
        while Instant::now() < until {
            if self.shared.lock().unwrap().window.fired.is_some() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        false
    }

    /// Lets a [`Fault::Pause`] deliver what it holds.
    pub fn release(&self) {
        self.shared.lock().unwrap().released = true;
    }

    /// Stops counting; what the window saw.
    pub fn disarm(&self) -> Window {
        let mut state = self.shared.lock().unwrap();
        state.armed = false;
        state.target = None;
        state.released = true;
        std::mem::take(&mut state.window)
    }

    /// Backend pids of the proxied connections, in connect order.
    pub fn pids(&self) -> Vec<Option<i32>> {
        self.shared.lock().unwrap().pids.clone()
    }
}

/// Client to server: plain bytes.
fn forward(mut from: TcpStream, mut to: TcpStream) {
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let read = match from.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        if to.write_all(&buffer[..read]).is_err() {
            break;
        }
    }
    let _ = to.shutdown(Shutdown::Write);
}

/// Server to client, message by message.
fn back(
    mut server: TcpStream,
    mut client: TcpStream,
    shared: Arc<Mutex<State>>,
    connection: usize,
    admin: &str,
) {
    let mut buffer = vec![0u8; 1 << 16];
    let mut pending: Vec<u8> = Vec::new();
    let mut passthrough = false;
    loop {
        let read = match server.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        if passthrough {
            if client.write_all(&buffer[..read]).is_err() {
                break;
            }
            continue;
        }
        pending.extend_from_slice(&buffer[..read]);
        let mut offset = 0;
        let mut fault = None;
        while pending.len() - offset >= 5 {
            let kind = pending[offset];
            let length =
                u32::from_be_bytes(pending[offset + 1..offset + 5].try_into().unwrap()) as usize;
            if pending.len() - offset < 1 + length {
                break;
            }
            let mut state = shared.lock().unwrap();
            if kind == b'K' && length >= 12 {
                let pid = i32::from_be_bytes(pending[offset + 5..offset + 9].try_into().unwrap());
                state.pids[connection] = Some(pid);
            }
            if state.armed {
                let at = state.window.seen;
                state.window.seen += 1;
                state.window.log.push((connection, kind as char));
                if let Some((target, chosen)) = state.target {
                    if target == at && state.window.fired.is_none() {
                        state.window.fired = Some(Fired {
                            at,
                            connection,
                            kind: kind as char,
                            pid: state.pids[connection],
                            killed: None,
                        });
                        fault = Some(chosen);
                        break;
                    }
                }
            }
            drop(state);
            offset += 1 + length;
        }
        if client.write_all(&pending[..offset]).is_err() {
            break;
        }
        pending.drain(..offset);
        match fault {
            None => {}
            Some(Fault::Drop) => {
                let _ = server.shutdown(Shutdown::Both);
                let _ = client.shutdown(Shutdown::Both);
                return;
            }
            Some(Fault::Pause) => {
                let until = Instant::now() + Duration::from_secs(30);
                while !shared.lock().unwrap().released && Instant::now() < until {
                    std::thread::sleep(Duration::from_millis(1));
                }
                if client.write_all(&pending).is_err() {
                    break;
                }
                pending.clear();
                passthrough = true;
            }
            Some(hold @ (Fault::HoldKill | Fault::HoldKillOther)) => {
                let pid = {
                    let state = shared.lock().unwrap();
                    if hold == Fault::HoldKill {
                        state.pids[connection]
                    } else {
                        state
                            .window
                            .log
                            .iter()
                            .map(|(c, _)| *c)
                            .find(|c| *c != connection)
                            .and_then(|other| state.pids[other])
                    }
                };
                // Held inside a connection's startup: its BackendKeyData is
                // among (or right behind) the held messages.
                let pid = match pid {
                    Some(pid) => Some(pid),
                    None if hold == Fault::HoldKill => {
                        let until = Instant::now() + Duration::from_secs(5);
                        let _ = server.set_read_timeout(Some(Duration::from_millis(50)));
                        loop {
                            if let Some(pid) = backend_pid(&pending) {
                                break Some(pid);
                            }
                            if Instant::now() >= until {
                                break None;
                            }
                            match server.read(&mut buffer) {
                                Ok(0) => break None,
                                Ok(read) => pending.extend_from_slice(&buffer[..read]),
                                Err(_) => {}
                            }
                        }
                    }
                    None => None,
                };
                let _ = server.set_read_timeout(None);
                if let Some(pid) = pid {
                    shared.lock().unwrap().pids[connection] = Some(pid);
                }
                if let Some(fired) = &mut shared.lock().unwrap().window.fired {
                    fired.pid = pid;
                }
                let killed = pid.map(|pid| kill(admin, pid));
                if let Some(fired) = &mut shared.lock().unwrap().window.fired {
                    fired.killed = killed;
                }
                if client.write_all(&pending).is_err() {
                    break;
                }
                pending.clear();
                passthrough = true;
            }
        }
    }
    let _ = client.shutdown(Shutdown::Write);
}

/// The pid in the first complete `BackendKeyData` of `bytes`, a run of
/// whole or partial server messages.
fn backend_pid(bytes: &[u8]) -> Option<i32> {
    let mut offset = 0;
    while bytes.len() - offset >= 5 {
        let length = u32::from_be_bytes(bytes[offset + 1..offset + 5].try_into().unwrap()) as usize;
        if bytes.len() - offset < 1 + length {
            return None;
        }
        if bytes[offset] == b'K' && length >= 12 {
            return Some(i32::from_be_bytes(
                bytes[offset + 5..offset + 9].try_into().unwrap(),
            ));
        }
        offset += 1 + length;
    }
    None
}

/// Terminates backend `pid` from a direct connection and waits (bounded,
/// 10 s) until it is gone.
fn kill(admin: &str, pid: i32) -> bool {
    let Ok(mut client) = postgres::Client::connect(admin, postgres::NoTls) else {
        return false;
    };
    if client
        .query_one("SELECT pg_terminate_backend($1)", &[&pid])
        .is_err()
    {
        return false;
    }
    let until = Instant::now() + Duration::from_secs(10);
    while Instant::now() < until {
        let alive: i64 = match client.query_one(
            "SELECT count(*) FROM pg_stat_activity WHERE pid = $1",
            &[&pid],
        ) {
            Ok(row) => row.get(0),
            Err(_) => return false,
        };
        if alive == 0 {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    false
}
