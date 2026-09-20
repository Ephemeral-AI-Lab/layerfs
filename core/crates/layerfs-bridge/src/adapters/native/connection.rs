//! One authenticated duplex connection with bounded records and absolute deadlines.
use super::protocol::{Frame, HEADER};
use crate::contract::*;
use snow::{Builder, StatelessTransportState};
use std::{
    io::{self, Read, Write},
    net::{Shutdown, SocketAddr, TcpStream},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const NOISE: &str = "Noise_KK_25519_ChaChaPoly_BLAKE2s";
const CIPHER_MAX: usize = METADATA_BYTES + HEADER + 16;

/// Authentication evidence includes the actual public key; a numeric selector is
/// never sufficient authority. Direct callers prove possession of the same key.
#[derive(Clone, Copy, Debug)]
pub struct VerifiedPeer {
    public: [u8; 32],
}
impl VerifiedPeer {
    pub const fn public_key(&self) -> &[u8; 32] {
        &self.public
    }
    pub fn from_private(private: &[u8; 32]) -> Result<Self, Failure> {
        use snow::resolvers::CryptoResolver;
        let mut dh = snow::resolvers::DefaultResolver
            .resolve_dh(&snow::params::DHChoice::Curve25519)
            .ok_or(Code::Unsupported)?;
        dh.set(private);
        Ok(Self {
            public: dh.pubkey().try_into().map_err(|_| Code::Unsupported)?,
        })
    }
}
#[derive(Clone)]
pub struct Peer {
    pub selector: u32,
    pub public: [u8; 32],
    pub expires_unix: u64,
}
impl Peer {
    pub fn valid(&self) -> bool {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .is_ok_and(|t| t.as_secs() < self.expires_unix)
    }
}
pub struct Connection {
    pub receive: Receiver,
    pub send: Sender,
    pub peer: VerifiedPeer,
}
pub struct Receiver {
    io: Socket,
    state: Arc<StatelessTransportState>,
    nonce: u64,
}
pub struct Sender {
    io: Socket,
    state: Arc<StatelessTransportState>,
    nonce: u64,
}
struct Socket {
    stream: TcpStream,
    deadline: Instant,
}
impl Socket {
    fn remaining(&self) -> io::Result<Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| io::Error::from(io::ErrorKind::TimedOut))
    }
}
impl Read for Socket {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.stream.set_read_timeout(Some(self.remaining()?))?;
        self.stream.read(b)
    }
}
impl Write for Socket {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.stream.set_write_timeout(Some(self.remaining()?))?;
        self.stream.write(b)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Receiver {
    pub fn deadline(&mut self, d: Instant) {
        self.io.deadline = d;
    }
    pub fn close(&self) {
        let _ = self.io.stream.shutdown(Shutdown::Both);
    }
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.io.stream.peer_addr()
    }
    pub fn read(&mut self) -> Result<Frame, Failure> {
        let encrypted = read_record(&mut self.io, CIPHER_MAX)?;
        if encrypted.len() < 16 {
            return Err(Code::InvalidInput.into());
        }
        let mut plain = vec![0; encrypted.len() - 16];
        let n = self
            .state
            .read_message(self.nonce, &encrypted, &mut plain)
            .map_err(|_| Code::Denied)?;
        self.nonce = self.nonce.checked_add(1).ok_or(Code::Capacity)?;
        let mut cursor = io::Cursor::new(&plain[..n]);
        let frame = Frame::read(&mut cursor)?;
        if cursor.position() != n as u64 {
            return Err(Code::InvalidInput.into());
        }
        Ok(frame)
    }
}
impl Sender {
    pub fn deadline(&mut self, d: Instant) {
        self.io.deadline = d;
    }
    pub fn end_upload(&self) {
        let _ = self.io.stream.shutdown(Shutdown::Write);
    }
    pub fn close(&self) {
        let _ = self.io.stream.shutdown(Shutdown::Both);
    }
    pub fn write(&mut self, frame: &Frame) -> Result<(), Failure> {
        let plain = frame.encode()?;
        let mut encrypted = vec![0; plain.len() + 16];
        let n = self
            .state
            .write_message(self.nonce, &plain, &mut encrypted)
            .map_err(|_| Code::Io)?;
        self.nonce = self.nonce.checked_add(1).ok_or(Code::Capacity)?;
        write_record(&mut self.io, &encrypted[..n])
    }
}
pub fn connect(
    address: SocketAddr,
    selector: u32,
    private: &[u8; 32],
    server: &[u8; 32],
) -> Result<Connection, Failure> {
    let stream = TcpStream::connect_timeout(&address, Duration::from_secs(5))?;
    configure(&stream)?;
    let mut io = Socket {
        stream,
        deadline: Instant::now() + Duration::from_secs(5),
    };
    io.write_all(&selector.to_be_bytes())?;
    handshake(io, selector, private, *server, true)
}
pub fn accept(
    stream: TcpStream,
    private: &[u8; 32],
    peers: &[Peer],
) -> Result<Connection, Failure> {
    if peers.len() > 16 {
        return Err(Code::Capacity.into());
    }
    // macOS inherits O_NONBLOCK from the listener. This adapter uses bounded
    // blocking I/O and absolute timeouts, so normalize the accepted descriptor.
    stream.set_nonblocking(false)?;
    configure(&stream)?;
    let mut io = Socket {
        stream,
        deadline: Instant::now() + Duration::from_secs(5),
    };
    let mut selector = [0; 4];
    io.read_exact(&mut selector)?;
    let selector = u32::from_be_bytes(selector);
    let peer = peers
        .iter()
        .find(|p| p.selector == selector && p.valid())
        .ok_or(Code::Denied)?;
    handshake(io, selector, private, peer.public, false)
}
fn handshake(
    mut io: Socket,
    selector: u32,
    private: &[u8; 32],
    public: [u8; 32],
    initiator: bool,
) -> Result<Connection, Failure> {
    let params = NOISE.parse().map_err(|_| Code::Unsupported)?;
    let prologue = selector.to_be_bytes();
    let builder = Builder::new(params)
        .local_private_key(private)
        .remote_public_key(&public)
        .prologue(&prologue);
    let mut state = if initiator {
        builder.build_initiator()
    } else {
        builder.build_responder()
    }
    .map_err(|_| Code::Denied)?;
    let mut buffer = [0; 1024];
    let mut payload = [0; 1024];
    if initiator {
        let n = state
            .write_message(&[], &mut buffer)
            .map_err(|_| Code::Denied)?;
        write_record(&mut io, &buffer[..n])?;
        let input = read_record(&mut io, 1024)?;
        if state
            .read_message(&input, &mut payload)
            .map_err(|_| Code::Denied)?
            != 0
        {
            return Err(Code::Denied.into());
        }
    } else {
        let input = read_record(&mut io, 1024)?;
        if state
            .read_message(&input, &mut payload)
            .map_err(|_| Code::Denied)?
            != 0
        {
            return Err(Code::Denied.into());
        }
        let n = state
            .write_message(&[], &mut buffer)
            .map_err(|_| Code::Denied)?;
        write_record(&mut io, &buffer[..n])?;
    }
    if state.get_remote_static() != Some(public.as_slice()) {
        return Err(Code::Denied.into());
    }
    let state = Arc::new(
        state
            .into_stateless_transport_mode()
            .map_err(|_| Code::Denied)?,
    );
    let send = Sender {
        io: Socket {
            stream: io.stream.try_clone()?,
            deadline: io.deadline,
        },
        state: Arc::clone(&state),
        nonce: 0,
    };
    Ok(Connection {
        receive: Receiver {
            io,
            state,
            nonce: 0,
        },
        send,
        peer: VerifiedPeer { public },
    })
}
fn read_record(io: &mut Socket, limit: usize) -> Result<Vec<u8>, Failure> {
    let mut h = [0; 4];
    io.read_exact(&mut h)?;
    let n = u32::from_be_bytes(h) as usize;
    if n == 0 || n > limit {
        return Err(Code::Capacity.into());
    }
    let mut b = vec![0; n];
    io.read_exact(&mut b)?;
    Ok(b)
}
fn write_record(io: &mut Socket, b: &[u8]) -> Result<(), Failure> {
    io.write_all(&(b.len() as u32).to_be_bytes())?;
    io.write_all(b)?;
    Ok(())
}

fn configure(stream: &TcpStream) -> Result<(), Failure> {
    use nix::sys::socket::{getsockopt, setsockopt, sockopt};
    let size = 128usize * 1024;
    setsockopt(stream, sockopt::SndBuf, &size).map_err(|_| Code::Unsupported)?;
    setsockopt(stream, sockopt::RcvBuf, &size).map_err(|_| Code::Unsupported)?;
    let send = getsockopt(stream, sockopt::SndBuf).map_err(|_| Code::Unsupported)?;
    let receive = getsockopt(stream, sockopt::RcvBuf).map_err(|_| Code::Unsupported)?;
    if send > size * 2 || receive > size * 2 {
        return Err(Code::Capacity.into());
    }
    stream.set_nodelay(true)?;
    Ok(())
}
