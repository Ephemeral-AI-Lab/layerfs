//! The connections of one S3 client: TCP, or TLS over it, kept alive
//! between requests (#510).
//!
//! A request no longer asks the server to close after its reply
//! (`Connection: close`): on Cloudflare R2 that header made the edge end
//! about one ranged reply in a hundred with a clean TLS close a few KiB
//! before its `content-length`. A connection whose last reply was read to
//! its exact end, with nothing left over and no `Connection: close` from the
//! server, goes back to its client's [`Kept`] connections and the client's
//! next request may take it instead of connecting.
//!
//! Taking a kept connection sends nothing: one older than [`KEPT_IDLE`], or
//! one with anything to read (a close, an alert, stray bytes), is closed
//! and the request connects afresh. That check happens before any byte of
//! the request is written, so it is not a resend. Once a request's first
//! byte is written on a kept connection, a failure is that request's
//! outcome, as on a new connection: it is never sent again on another.
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long an idle connection may wait for its client's next request.
/// Short, so the server's own idle close is seen (by the quiet check)
/// before a request is written rather than after.
pub(crate) const KEPT_IDLE: Duration = Duration::from_secs(10);
/// Idle connections one client keeps at most; more are closed.
const MAX_KEPT: usize = 4;

/// One connection: TCP, or TLS over it.
pub(crate) enum Connection {
    Plain(TcpStream),
    #[cfg(feature = "tls")]
    Tls(Box<native_tls::TlsStream<TcpStream>>),
}

impl Connection {
    /// Wraps `socket` in TLS for `host` when `tls`; the handshake checks the
    /// certificate against the system's roots and `host`.
    pub(crate) fn open(socket: TcpStream, host: &str, tls: bool) -> Result<Self, String> {
        if !tls {
            return Ok(Self::Plain(socket));
        }
        #[cfg(feature = "tls")]
        {
            let connector = native_tls::TlsConnector::new().map_err(|e| format!("TLS: {e}"))?;
            let stream = connector
                .connect(host, socket)
                .map_err(|e| format!("TLS handshake with {host}: {e}"))?;
            Ok(Self::Tls(Box::new(stream)))
        }
        #[cfg(not(feature = "tls"))]
        Err(format!("TLS to {host}: this build has no tls feature"))
    }

    fn socket(&self) -> &TcpStream {
        match self {
            Self::Plain(socket) => socket,
            #[cfg(feature = "tls")]
            Self::Tls(stream) => stream.get_ref(),
        }
    }

    /// Whether an idle connection is still quiet: a non-blocking read finds
    /// nothing to read, neither bytes nor a close. TLS records that carry no
    /// data (session tickets) are taken in passing. It sends nothing.
    fn quiet(&mut self) -> bool {
        if self.socket().set_nonblocking(true).is_err() {
            return false;
        }
        let mut byte = [0; 1];
        let quiet = matches!(self.read(&mut byte), Err(e) if e.kind() == io::ErrorKind::WouldBlock);
        quiet && self.socket().set_nonblocking(false).is_ok()
    }
}

impl Read for Connection {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Plain(socket) => socket.read(buffer),
            #[cfg(feature = "tls")]
            Self::Tls(stream) => stream.read(buffer),
        }
    }
}

impl Write for Connection {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self {
            Self::Plain(socket) => socket.write(buffer),
            #[cfg(feature = "tls")]
            Self::Tls(stream) => stream.write(buffer),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(socket) => socket.flush(),
            #[cfg(feature = "tls")]
            Self::Tls(stream) => stream.flush(),
        }
    }
}

/// A client's idle connections, newest last.
#[derive(Default)]
pub(crate) struct Kept(Mutex<Vec<(Connection, Instant)>>);

impl std::fmt::Debug for Kept {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Kept")
    }
}

impl Kept {
    /// The newest idle connection that is recent and quiet, with how long
    /// it was idle; every stale or noisy one met on the way is closed.
    pub(crate) fn take(&self) -> Option<(Connection, Duration)> {
        let mut idle = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        idle.retain(|(_, since)| since.elapsed() < KEPT_IDLE);
        while let Some((mut connection, since)) = idle.pop() {
            let quiet_for = since.elapsed();
            if quiet_for < KEPT_IDLE && connection.quiet() {
                return Some((connection, quiet_for));
            }
        }
        None
    }

    /// Keeps `connection` for a later request, or closes it when enough
    /// are kept.
    pub(crate) fn put(&self, connection: Connection) {
        let mut idle = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        idle.retain(|(_, since)| since.elapsed() < KEPT_IDLE);
        if idle.len() < MAX_KEPT {
            idle.push((connection, Instant::now()));
        }
    }
}
