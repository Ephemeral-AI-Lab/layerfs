//! One authenticated duplex connection with bounded records and absolute deadlines.
use super::protocol::{Frame, Kind, HEADER};
use crate::contract::*;
use snow::{Builder, StatelessTransportState};
use std::{
    io::{self, Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
// The ARMv8 AEAD profile is required on aarch64 for every build that produces a
// binary, and it is a build input, not a runtime choice: `aes`/`polyval` gate their ARMv8 backends on the `aes_armv8` and
// `polyval_armv8` cfgs, which only a global flag can set. A build without them is
// refused here rather than quietly negotiating the 2-4x slower ChaCha20-Poly1305
// suite, which is what used to happen when the flags were dropped - for example by
// invoking cargo from a directory where `config.toml` was not discovered. The
// repository-root `.cargo/config.toml` supplies them for every aarch64 build made
// from inside the repository; a build made from outside must pass them explicitly.
#[cfg(all(target_arch = "aarch64", not(all(aes_armv8, polyval_armv8)), not(doc)))]
compile_error!(
    "aarch64 builds require the ARMv8 AEAD profile: --cfg aes_armv8 --cfg polyval_armv8 \
     -C target-feature=+aes,+sha2. The repository-root .cargo/config.toml supplies it; \
     build from inside the repository, or pass those flags explicitly (an explicit \
     RUSTFLAGS overrides the config table, so repeat all four). aarch64 has no other \
     supported suite: ChaCha20-Poly1305 is not offered there."
);

/// The authenticated suite this build negotiates.
///
/// One suite per architecture, chosen by the build profile and never by a runtime
/// preference: AES-GCM where its backend is accelerated (x86_64 detects
/// AES-NI/CLMUL at runtime, aarch64 requires the ARMv8 profile above), and
/// ChaCha20-Poly1305 on every other target. No build can select the soft AES path,
/// and two builds that chose different suites reject each other's handshake
/// explicitly rather than degrading.
#[cfg(any(target_arch = "x86_64", target_arch = "x86", target_arch = "aarch64"))]
pub const NOISE: &str = "Noise_KK_25519_AESGCM_SHA256";
#[cfg(not(any(target_arch = "x86_64", target_arch = "x86", target_arch = "aarch64")))]
pub const NOISE: &str = "Noise_KK_25519_ChaChaPoly_BLAKE2s";
const CIPHER_MAX: usize = METADATA_BYTES + HEADER + 16;
/// Full-size data records are coalesced into one vectored write up to this many
/// bytes. The record and `Frame` sizes are unchanged, so the bytes on the wire are
/// identical; only the number of records one syscall carries changes.
const RECORD_BATCH_BYTES: usize = 256 * 1024;
/// A record smaller than this is written immediately instead of joining the open
/// batch. Coalescing a short or control record would defer the peer's progress
/// clock (`IO_PROGRESS_MS`) while saving a syscall that bulk data already pays for.
const RECORD_BATCH_MIN_BYTES: usize = FRAME_BYTES / 2;

pub use crate::contract::VerifiedPeer;

impl VerifiedPeer {
    /// Trusted direct entry: derive the caller identity from its private key.
    pub fn from_private(private: &[u8; 32]) -> Result<Self, Failure> {
        use snow::resolvers::CryptoResolver;
        let mut dh = snow::resolvers::DefaultResolver
            .resolve_dh(&snow::params::DHChoice::Curve25519)
            .ok_or(Code::Unsupported)?;
        dh.set(private);
        Ok(Self::authenticated(
            dh.pubkey().try_into().map_err(|_| Code::Unsupported)?,
        ))
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
    sealed: Vec<u8>,
    plain: Vec<u8>,
}
pub struct Sender {
    io: Socket,
    state: Arc<StatelessTransportState>,
    nonce: u64,
    plain: Vec<u8>,
    sealed: Vec<u8>,
    batch: Vec<u8>,
}
struct Socket {
    stream: TcpStream,
    deadline: Instant,
    activity: Arc<Mutex<Instant>>,
}
impl Socket {
    fn remaining(&self) -> io::Result<Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .map(|d| d.min(Duration::from_millis(IO_PROGRESS_MS)))
            .ok_or_else(|| io::Error::from(io::ErrorKind::TimedOut))
    }
}
impl Read for Socket {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        if b.is_empty() {
            return Ok(0);
        }
        // Upload and response waits overlap. A productive upload must keep the
        // response wait alive; silence in both directions still expires.
        let started = Instant::now();
        loop {
            let last = (*self
                .activity
                .lock()
                .map_err(|_| io::Error::other("socket activity poisoned"))?)
            .max(started);
            let end = self
                .deadline
                .min(last + Duration::from_millis(IO_PROGRESS_MS));
            let remaining = end
                .checked_duration_since(Instant::now())
                .filter(|value| !value.is_zero())
                .ok_or(io::ErrorKind::TimedOut)?;
            self.stream.set_read_timeout(Some(remaining))?;
            match self.stream.read(b) {
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                    ) => {}
                result => {
                    if result.as_ref().is_ok_and(|count| *count > 0) {
                        *self
                            .activity
                            .lock()
                            .map_err(|_| io::Error::other("socket activity poisoned"))? =
                            Instant::now();
                    }
                    return result;
                }
            }
        }
    }
}
impl Write for Socket {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.stream.set_write_timeout(Some(self.remaining()?))?;
        let count = self.stream.write(b)?;
        if count > 0 {
            *self
                .activity
                .lock()
                .map_err(|_| io::Error::other("socket activity poisoned"))? = Instant::now();
        }
        Ok(count)
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
    /// One record, decrypted into this receiver's reused scratch buffers.
    pub fn read(&mut self) -> Result<Frame, Failure> {
        read_record_into(&mut self.io, CIPHER_MAX, &mut self.sealed)?;
        if self.sealed.len() < 16 {
            return Err(Code::InvalidInput.into());
        }
        self.plain.clear();
        self.plain.resize(self.sealed.len() - 16, 0);
        let n = self
            .state
            .read_message(self.nonce, &self.sealed, &mut self.plain)
            .map_err(|_| Code::Denied)?;
        self.nonce = self.nonce.checked_add(1).ok_or(Code::Capacity)?;
        let mut cursor = io::Cursor::new(&self.plain[..n]);
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
    pub fn end_upload(&mut self) {
        let _ = self.flush();
        let _ = self.io.stream.shutdown(Shutdown::Write);
    }
    pub fn close(&self) {
        let _ = self.io.stream.shutdown(Shutdown::Both);
    }
    /// Encrypts one record into the open batch, reusing this sender's scratch.
    fn append_record(&mut self, frame: &Frame) -> Result<(), Failure> {
        frame.encode_into(&mut self.plain)?;
        self.sealed.clear();
        self.sealed.resize(self.plain.len() + 16, 0);
        let n = self
            .state
            .write_message(self.nonce, &self.plain, &mut self.sealed)
            .map_err(|_| Code::Io)?;
        self.nonce = self.nonce.checked_add(1).ok_or(Code::Capacity)?;
        self.batch.extend_from_slice(&(n as u32).to_be_bytes());
        self.batch.extend_from_slice(&self.sealed[..n]);
        Ok(())
    }
    /// A full-size data record may wait for the rest of its batch. A control record
    /// or a short record flushes first and then goes out alone, so a peer's progress
    /// clock (`IO_PROGRESS_MS`) always advances and no peer waits on a held record.
    pub fn write(&mut self, frame: &Frame) -> Result<(), Failure> {
        let bulk = matches!(frame.kind, Kind::Body | Kind::ResultData)
            && frame.bytes.len() >= RECORD_BATCH_MIN_BYTES;
        if !bulk {
            self.flush()?;
        }
        self.append_record(frame)?;
        if !bulk || self.batch.len() >= RECORD_BATCH_BYTES {
            self.flush()?;
        }
        Ok(())
    }
    /// The batch is contiguous, so one `write_all` carries every record it holds.
    pub fn flush(&mut self) -> Result<(), Failure> {
        if self.batch.is_empty() {
            return Ok(());
        }
        let result = self
            .io
            .write_all(&self.batch)
            .map_err(|_| Failure::from(Code::Io));
        self.batch.clear();
        result
    }
}
pub fn connect(
    address: SocketAddr,
    selector: u32,
    private: &[u8; 32],
    server: &[u8; 32],
) -> Result<Connection, Failure> {
    connect_with_deadline(address, selector, private, server, None)
}

/// One connection attempt whose connect, authentication and subsequent HELLO
/// share the caller's deadline. Each native phase retains its five-second cap.
pub fn connect_until(
    address: SocketAddr,
    selector: u32,
    private: &[u8; 32],
    server: &[u8; 32],
    deadline: Instant,
) -> Result<Connection, Failure> {
    connect_with_deadline(address, selector, private, server, Some(deadline))
}

fn connect_with_deadline(
    address: SocketAddr,
    selector: u32,
    private: &[u8; 32],
    server: &[u8; 32],
    deadline: Option<Instant>,
) -> Result<Connection, Failure> {
    let phase_limit = Duration::from_secs(5);
    let remaining = deadline
        .map_or(Some(phase_limit), |end| {
            end.checked_duration_since(Instant::now())
                .filter(|left| !left.is_zero())
        })
        .ok_or(Code::Deadline)?;
    let stream = TcpStream::connect_timeout(&address, remaining.min(phase_limit))?;
    stream.set_nodelay(true)?;
    let phase_end = Instant::now() + phase_limit;
    let mut io = Socket {
        stream,
        deadline: deadline.map_or(phase_end, |end| end.min(phase_end)),
        activity: Arc::new(Mutex::new(Instant::now())),
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
    stream.set_nodelay(true)?;
    let mut io = Socket {
        stream,
        deadline: Instant::now() + Duration::from_secs(5),
        activity: Arc::new(Mutex::new(Instant::now())),
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
/// Ordinary TCP listener; kernel buffer sizes are observations, not admission limits.
pub fn listen(address: SocketAddr) -> Result<TcpListener, Failure> {
    Ok(TcpListener::bind(address)?)
}
/// The negotiated suite requires the CPU feature its backend needs.
///
/// The pinned `aes`/`polyval` crates detect the feature at runtime and fall back
/// to a soft implementation when it is absent — measured 0.111 GB/s for soft
/// AES-GCM against 1.61 GB/s accelerated — so an absent feature is a refusal
/// rather than a silent downgrade of the negotiated profile.
fn verify_cpu_backend() -> Result<(), Failure> {
    #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
    {
        if !(std::arch::is_x86_feature_detected!("aes")
            && std::arch::is_x86_feature_detected!("pclmulqdq"))
        {
            return Err(Code::Unsupported.into());
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        // `aes` implies PMULL on the ARMv8 crypto extension. The profile is a
        // build input (see the compile-time refusal above), so this is the one
        // place a machine without the extension is still refused: a binary built
        // for ARMv8 must not run the soft AES path on a CPU that lacks it.
        if !std::arch::is_aarch64_feature_detected!("aes") {
            return Err(Code::Unsupported.into());
        }
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "x86", target_arch = "aarch64")))]
    {
        // The fallback suite needs NEON on other aarch64-like targets; every
        // supported one declares it, so there is nothing to detect.
    }
    Ok(())
}
fn handshake(
    mut io: Socket,
    selector: u32,
    private: &[u8; 32],
    public: [u8; 32],
    initiator: bool,
) -> Result<Connection, Failure> {
    verify_cpu_backend()?;
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
            activity: Arc::clone(&io.activity),
        },
        state: Arc::clone(&state),
        nonce: 0,
        plain: Vec::new(),
        sealed: Vec::new(),
        batch: Vec::new(),
    };
    Ok(Connection {
        receive: Receiver {
            io,
            state,
            nonce: 0,
            sealed: Vec::new(),
            plain: Vec::new(),
        },
        send,
        peer: VerifiedPeer::authenticated(public),
    })
}
/// Reads one length-prefixed record into a caller-owned buffer.
fn read_record_into(io: &mut Socket, limit: usize, out: &mut Vec<u8>) -> Result<(), Failure> {
    let mut h = [0; 4];
    io.read_exact(&mut h)?;
    let n = u32::from_be_bytes(h) as usize;
    if n == 0 || n > limit {
        return Err(Code::Capacity.into());
    }
    out.clear();
    out.resize(n, 0);
    io.read_exact(out)?;
    Ok(())
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
