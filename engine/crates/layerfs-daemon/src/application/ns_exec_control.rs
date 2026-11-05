//! The exec supervisor's control socket (#538, skeleton).
//!
//! One supervisor owns one child process group. This socket is how a caller
//! learns that the child started, asks for its status, signals the child's
//! group, or learns that it exited. Frames are a four-byte big-endian length
//! followed by a fixed short body, so a client cannot make the supervisor
//! allocate from a client-chosen size. The socket lives in the supervisor's
//! private directory and is created `0600`.
//!
//! Authorization beyond the socket mode is deliberately deferred: the skeleton
//! admits only a peer with the supervisor's own effective uid and does not yet
//! check the peer's pid namespace, cgroup or Workspace membership. Client
//! presence, reconnect and the daemon session registry belong to the later
//! #538 work, not to this slice.
use nix::{
    sys::socket::{getsockopt, sockopt},
    unistd::Uid,
};
use std::{
    fs,
    io::{self, Read, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::Path,
};

/// The longest body the skeleton sends or accepts.
const MAX_BODY: usize = 24;
const STARTED: u8 = 1;
const EXITED: u8 = 2;
const SIGNAL: u8 = 3;
const STATUS: u8 = 4;
const STATUS_REPLY: u8 = 5;
const STOP: u8 = 6;

/// One control message. `Started` and `Exited` flow supervisor to caller;
/// `Signal`, `Status` and `Stop` flow the other way; `StatusReply` answers
/// `Status` and a caller that connects after `Started` also receives the
/// current `Started`/`Exited` snapshot under the same lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    Started {
        pid: i32,
        pgid: i32,
    },
    Exited {
        code: i32,
        signal: i32,
        group_alive: bool,
    },
    Signal {
        signo: i32,
    },
    Status,
    StatusReply {
        running: bool,
        pid: i32,
        pgid: i32,
        code: i32,
        signal: i32,
        group_alive: bool,
    },
    Stop {
        grace_ms: u32,
    },
}

impl Message {
    fn encode(self) -> (u8, Vec<u8>) {
        match self {
            Self::Started { pid, pgid } => {
                let mut body = Vec::with_capacity(8);
                body.extend_from_slice(&pid.to_be_bytes());
                body.extend_from_slice(&pgid.to_be_bytes());
                (STARTED, body)
            }
            Self::Exited {
                code,
                signal,
                group_alive,
            } => {
                let mut body = Vec::with_capacity(9);
                body.extend_from_slice(&code.to_be_bytes());
                body.extend_from_slice(&signal.to_be_bytes());
                body.push(u8::from(group_alive));
                (EXITED, body)
            }
            Self::Signal { signo } => (SIGNAL, signo.to_be_bytes().to_vec()),
            Self::Status => (STATUS, Vec::new()),
            Self::StatusReply {
                running,
                pid,
                pgid,
                code,
                signal,
                group_alive,
            } => {
                let mut body = Vec::with_capacity(18);
                body.push(u8::from(running));
                body.extend_from_slice(&pid.to_be_bytes());
                body.extend_from_slice(&pgid.to_be_bytes());
                body.extend_from_slice(&code.to_be_bytes());
                body.extend_from_slice(&signal.to_be_bytes());
                body.push(u8::from(group_alive));
                (STATUS_REPLY, body)
            }
            Self::Stop { grace_ms } => (STOP, grace_ms.to_be_bytes().to_vec()),
        }
    }

    fn decode(kind: u8, body: &[u8]) -> io::Result<Self> {
        let invalid = |expected: usize| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "control frame {kind} has {} bytes, expected {expected}",
                    body.len()
                ),
            )
        };
        match kind {
            STARTED => {
                if body.len() != 8 {
                    return Err(invalid(8));
                }
                Ok(Self::Started {
                    pid: integer(&body[0..4]),
                    pgid: integer(&body[4..8]),
                })
            }
            EXITED => {
                if body.len() != 9 {
                    return Err(invalid(9));
                }
                Ok(Self::Exited {
                    code: integer(&body[0..4]),
                    signal: integer(&body[4..8]),
                    group_alive: body[8] != 0,
                })
            }
            SIGNAL => {
                if body.len() != 4 {
                    return Err(invalid(4));
                }
                Ok(Self::Signal {
                    signo: integer(body),
                })
            }
            STATUS => {
                if !body.is_empty() {
                    return Err(invalid(0));
                }
                Ok(Self::Status)
            }
            STATUS_REPLY => {
                if body.len() != 18 {
                    return Err(invalid(18));
                }
                Ok(Self::StatusReply {
                    running: body[0] != 0,
                    pid: integer(&body[1..5]),
                    pgid: integer(&body[5..9]),
                    code: integer(&body[9..13]),
                    signal: integer(&body[13..17]),
                    group_alive: body[17] != 0,
                })
            }
            STOP => {
                if body.len() != 4 {
                    return Err(invalid(4));
                }
                Ok(Self::Stop {
                    grace_ms: u32::from_be_bytes(body.try_into().expect("four bytes")),
                })
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown control frame kind {kind}"),
            )),
        }
    }
}

fn integer(bytes: &[u8]) -> i32 {
    i32::from_be_bytes(bytes.try_into().expect("an encoded i32"))
}

/// Bind the control socket (mode 0600, any stale path removed). The caller
/// serves it; a bind failure must happen before the child is spawned.
pub fn listen(path: &Path) -> io::Result<UnixListener> {
    match fs::remove_file(path) {
        Ok(()) => (),
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        Err(error) => return Err(error),
    }
    let listener = UnixListener::bind(path)?;
    if let Err(error) = fs::set_permissions(path, fs::Permissions::from_mode(0o600)) {
        let _ = fs::remove_file(path);
        return Err(error);
    }
    Ok(listener)
}

/// Write one length-framed message.
pub fn send(stream: &mut UnixStream, message: Message) -> io::Result<()> {
    let (kind, body) = message.encode();
    debug_assert!(body.len() < MAX_BODY);
    let length = u32::try_from(body.len() + 1).expect("a bounded frame");
    stream.write_all(&length.to_be_bytes())?;
    stream.write_all(&[kind])?;
    stream.write_all(&body)
}

/// Read one length-framed message; `None` is a clean end of stream. A caller
/// that disconnects mid-frame is an error, not a silent message.
pub fn receive(stream: &mut UnixStream) -> io::Result<Option<Message>> {
    let mut length = [0u8; 4];
    match stream.read(&mut length[..1])? {
        0 => return Ok(None),
        1 => (),
        _ => unreachable!("one byte was requested"),
    }
    stream.read_exact(&mut length[1..])?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_BODY {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("control frame length {length} is outside 1..={MAX_BODY}"),
        ));
    }
    let mut frame = vec![0u8; length];
    stream.read_exact(&mut frame)?;
    Ok(Some(Message::decode(frame[0], &frame[1..])?))
}

/// Admit a peer with the supervisor's own effective uid. The socket is 0600,
/// so this only makes the check explicit; pid-namespace and Workspace
/// authorization are deferred (see the module comment).
pub fn admit(stream: &UnixStream) -> bool {
    getsockopt(stream, sockopt::PeerCredentials)
        .is_ok_and(|credentials| credentials.uid() == Uid::effective().as_raw())
}
