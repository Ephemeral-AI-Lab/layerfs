//! (Copied from #768's `verify768_tap.rs`.) A pass-through tap on the PostgreSQL wire for the external #768 checks:
//! it sits in front of a [`FaultProxy`](super::fault_proxy::FaultProxy) and
//! names, for every server message, the SQL text of the request it answers,
//! so a test can choose fault positions by what the Store is writing (the
//! held tail's publication, the hint rows) instead of by a message index
//! read off one build's flight.
//!
//! It parses the client's messages (simple `Query`; extended `Parse`,
//! `Bind`, `Describe`, `Execute`, `Close`, `Sync`) into a queue of pending
//! requests per connection, and pops that queue by the server's replies
//! (`ParseComplete`, `BindComplete`, `CloseComplete`, `RowDescription` or
//! `NoData` after a `Describe`, `CommandComplete` / `EmptyQueryResponse` /
//! `PortalSuspended` after an `Execute`, an `ErrorResponse` skipping to the
//! next `Sync`, `ReadyForQuery`). Every request is recorded before its bytes
//! are forwarded, so a reply always finds its request queued.
//!
//! While armed it keeps, per connection, every server message with its
//! request's SQL. A connection's message sequence is the one the fault proxy
//! behind it counts, so [`annotate`] lines a proxy [`Window`] up with the
//! tap's messages connection by connection. Nothing here is product code;
//! it listens on 127.0.0.1 only.
use super::fault_proxy::Window;
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

/// One server message and the SQL of the request it answers.
#[derive(Clone, Debug)]
pub struct Reply {
    pub kind: char,
    pub sql: String,
}

enum Pending {
    Parse(String),
    Bind(String),
    Describe(String),
    Execute(String),
    Close,
    Sync,
    Simple(String),
}

#[derive(Default)]
struct Conn {
    statements: HashMap<String, String>,
    portals: HashMap<String, String>,
    pending: VecDeque<Pending>,
    replies: Vec<Reply>,
    /// An `Execute` of `COPY ... FROM STDIN` was sent and its `CopyDone`
    /// (or `CopyFail`) not yet: the server ignores a `Sync` sent meanwhile.
    copying: bool,
}

#[derive(Default)]
struct Shared {
    armed: bool,
    conns: Vec<Conn>,
}

pub struct Tap {
    pub port: u16,
    shared: Arc<Mutex<Shared>>,
}

impl Tap {
    /// Listens on 127.0.0.1 and forwards every connection to `upstream`.
    pub fn start(upstream: SocketAddr) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let shared: Arc<Mutex<Shared>> = Arc::default();
        let state = shared.clone();
        std::thread::spawn(move || {
            for client in listener.incoming() {
                let Ok(client) = client else { return };
                let Ok(server) = TcpStream::connect(upstream) else {
                    return;
                };
                let _ = client.set_nodelay(true);
                let _ = server.set_nodelay(true);
                let index = {
                    let mut state = state.lock().unwrap();
                    state.conns.push(Conn::default());
                    state.conns.len() - 1
                };
                let (from, to) = (client.try_clone().unwrap(), server.try_clone().unwrap());
                let up = state.clone();
                std::thread::spawn(move || requests(from, to, up, index));
                let down = state.clone();
                std::thread::spawn(move || replies(server, client, down, index));
            }
        });
        Self { port, shared }
    }

    /// Starts recording from nothing on every connection.
    pub fn arm(&self) {
        let mut state = self.shared.lock().unwrap();
        for conn in &mut state.conns {
            conn.replies.clear();
        }
        state.armed = true;
    }

    /// Stops recording: per connection (accept order), what it recorded.
    pub fn disarm(&self) -> Vec<Vec<Reply>> {
        let mut state = self.shared.lock().unwrap();
        state.armed = false;
        state
            .conns
            .iter_mut()
            .map(|conn| std::mem::take(&mut conn.replies))
            .collect()
    }
}

/// The proxy window's messages, in its order, each with the SQL of its
/// request; panics when the tap saw another message sequence.
pub fn annotate(window: &Window, taps: &[Vec<Reply>]) -> Vec<(usize, Reply)> {
    let mut at: HashMap<usize, usize> = HashMap::new();
    window
        .log
        .iter()
        .enumerate()
        .map(|(n, (connection, kind))| {
            let j = at.entry(*connection).or_default();
            let reply = taps
                .get(*connection)
                .and_then(|replies| replies.get(*j))
                .unwrap_or_else(|| panic!("message {n}: c{connection}#{j} not tapped"))
                .clone();
            assert_eq!(reply.kind, *kind, "message {n}: c{connection}#{j}");
            *j += 1;
            (*connection, reply)
        })
        .collect()
}

fn cstr(body: &[u8], at: &mut usize) -> String {
    let end = body[*at..]
        .iter()
        .position(|b| *b == 0)
        .map_or(body.len(), |n| *at + n);
    let text = String::from_utf8_lossy(&body[*at..end]).into_owned();
    *at = (end + 1).min(body.len());
    text
}

fn short(sql: &str) -> String {
    let flat: String = sql.split_whitespace().collect::<Vec<_>>().join(" ");
    flat.chars().take(160).collect()
}

fn record(state: &Mutex<Shared>, index: usize, kind: u8, body: &[u8]) {
    let mut state = state.lock().unwrap();
    let conn = &mut state.conns[index];
    let mut at = 0;
    match kind {
        b'P' => {
            let name = cstr(body, &mut at);
            let sql = short(&cstr(body, &mut at));
            conn.statements.insert(name, sql.clone());
            conn.pending.push_back(Pending::Parse(sql));
        }
        b'B' => {
            let portal = cstr(body, &mut at);
            let statement = cstr(body, &mut at);
            let sql = conn.statements.get(&statement).cloned().unwrap_or_default();
            conn.portals.insert(portal, sql.clone());
            conn.pending.push_back(Pending::Bind(sql));
        }
        b'D' => {
            let what = body.first().copied().unwrap_or(b'S');
            at = 1;
            let name = cstr(body, &mut at);
            let sql = match what {
                b'P' => conn.portals.get(&name).cloned(),
                _ => conn.statements.get(&name).cloned(),
            };
            conn.pending
                .push_back(Pending::Describe(sql.unwrap_or_default()));
        }
        b'E' => {
            let portal = cstr(body, &mut at);
            let sql = conn.portals.get(&portal).cloned().unwrap_or_default();
            if sql.starts_with("COPY") && sql.contains("FROM STDIN") {
                conn.copying = true;
            }
            conn.pending.push_back(Pending::Execute(sql));
        }
        b'C' => conn.pending.push_back(Pending::Close),
        b'S' if conn.copying => {}
        b'S' => conn.pending.push_back(Pending::Sync),
        b'c' | b'f' => conn.copying = false,
        b'Q' => {
            let sql = short(&cstr(body, &mut at));
            conn.pending.push_back(Pending::Simple(sql));
        }
        _ => {}
    }
}

/// Client to server: each whole message is recorded, then forwarded.
fn requests(mut from: TcpStream, mut to: TcpStream, state: Arc<Mutex<Shared>>, index: usize) {
    let mut buffer = vec![0u8; 1 << 16];
    let mut pending: Vec<u8> = Vec::new();
    let mut started = false;
    loop {
        let read = match from.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        pending.extend_from_slice(&buffer[..read]);
        let mut offset = 0;
        loop {
            if !started {
                // The startup packet has no type byte.
                if pending.len() - offset < 4 {
                    break;
                }
                let length =
                    u32::from_be_bytes(pending[offset..offset + 4].try_into().unwrap()) as usize;
                if pending.len() - offset < length {
                    break;
                }
                offset += length;
                started = true;
                continue;
            }
            if pending.len() - offset < 5 {
                break;
            }
            let kind = pending[offset];
            let length =
                u32::from_be_bytes(pending[offset + 1..offset + 5].try_into().unwrap()) as usize;
            if pending.len() - offset < 1 + length {
                break;
            }
            record(
                &state,
                index,
                kind,
                &pending[offset + 5..offset + 1 + length],
            );
            offset += 1 + length;
        }
        pending.drain(..offset);
        if to.write_all(&buffer[..read]).is_err() {
            break;
        }
    }
    let _ = to.shutdown(Shutdown::Write);
}

/// Server to client: each whole message is labelled and passed through.
fn replies(mut server: TcpStream, mut client: TcpStream, state: Arc<Mutex<Shared>>, index: usize) {
    let mut buffer = vec![0u8; 1 << 16];
    let mut pending: Vec<u8> = Vec::new();
    loop {
        let read = match server.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        pending.extend_from_slice(&buffer[..read]);
        let mut offset = 0;
        while pending.len() - offset >= 5 {
            let kind = pending[offset];
            let length =
                u32::from_be_bytes(pending[offset + 1..offset + 5].try_into().unwrap()) as usize;
            if pending.len() - offset < 1 + length {
                break;
            }
            label(&state, index, kind);
            offset += 1 + length;
        }
        pending.drain(..offset);
        if client.write_all(&buffer[..read]).is_err() {
            break;
        }
    }
    let _ = client.shutdown(Shutdown::Write);
}

fn label(state: &Mutex<Shared>, index: usize, kind: u8) {
    let mut state = state.lock().unwrap();
    let armed = state.armed;
    let conn = &mut state.conns[index];
    let sql = match conn.pending.front() {
        Some(
            Pending::Parse(sql)
            | Pending::Bind(sql)
            | Pending::Describe(sql)
            | Pending::Execute(sql)
            | Pending::Simple(sql),
        ) => sql.clone(),
        Some(Pending::Close) => "<close>".into(),
        Some(Pending::Sync) => "<sync>".into(),
        None => String::new(),
    };
    let front = conn.pending.front();
    let pop = matches!(
        (kind, front),
        (b'1', Some(Pending::Parse(_)))
            | (b'2', Some(Pending::Bind(_)))
            | (b'3', Some(Pending::Close))
            | (b'n' | b'T', Some(Pending::Describe(_)))
            | (b'C' | b'I' | b's', Some(Pending::Execute(_)))
            | (b'Z', Some(Pending::Sync | Pending::Simple(_)))
    );
    if kind == b'E' && !matches!(front, Some(Pending::Simple(_)) | None) {
        while !matches!(conn.pending.front(), Some(Pending::Sync) | None) {
            conn.pending.pop_front();
        }
    } else if pop {
        conn.pending.pop_front();
    }
    if armed {
        conn.replies.push(Reply {
            kind: kind as char,
            sql,
        });
    }
}
