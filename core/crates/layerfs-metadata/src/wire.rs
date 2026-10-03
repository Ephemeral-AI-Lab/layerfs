//! Bounded observation of the complete client's wire, without decoding SQL data.
use std::{
    io,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
/// Cumulative physical/protocol counts for one PostgreSQL client.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PgDiagnostics {
    /// Successfully authenticated connections.
    pub connections: u64,
    /// Submitted logical SQL operations, including explicit bootstrap.
    pub operations: u64,
    /// Complete extended-protocol Sync messages sent.
    pub sync_messages: u64,
    /// Complete simple Query messages sent, used by explicit bootstrap.
    pub simple_queries: u64,
    /// Complete backend ReadyForQuery messages observed.
    pub ready_for_query: u64,
    /// Socket bytes written, including startup/TLS framing.
    pub wire_sent: u64,
    /// Socket bytes read, including startup/TLS framing.
    pub wire_received: u64,
    /// Query-protocol bytes before TLS encryption, excluding startup.
    pub protocol_sent: u64,
    /// Query-protocol bytes after TLS decryption, excluding startup.
    pub protocol_received: u64,
}
#[derive(Default)]
struct Frames {
    header: [u8; 5],
    filled: usize,
    remaining: usize,
    tag: u8,
}
impl Frames {
    fn feed(&mut self, mut bytes: &[u8], sent: bool, counts: &mut PgDiagnostics) -> io::Result<()> {
        while !bytes.is_empty() {
            if self.remaining > 0 {
                let n = self.remaining.min(bytes.len());
                self.remaining -= n;
                bytes = &bytes[n..];
                if self.remaining == 0 {
                    self.complete(sent, counts);
                }
                continue;
            }
            let n = (5 - self.filled).min(bytes.len());
            self.header[self.filled..self.filled + n].copy_from_slice(&bytes[..n]);
            self.filled += n;
            bytes = &bytes[n..];
            if self.filled == 5 {
                self.tag = self.header[0];
                let length = u32::from_be_bytes(
                    self.header[1..]
                        .try_into()
                        .map_err(|_| io::Error::other("protocol length"))?,
                ) as usize;
                if !(4..=64 * 1024 * 1024).contains(&length) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "protocol frame bound",
                    ));
                }
                self.filled = 0;
                self.remaining = length - 4;
                if self.remaining == 0 {
                    self.complete(sent, counts);
                }
            }
        }
        Ok(())
    }
    fn complete(&self, sent: bool, counts: &mut PgDiagnostics) {
        match (sent, self.tag) {
            (true, b'S') => counts.sync_messages += 1,
            (true, b'Q') => counts.simple_queries += 1,
            (false, b'Z') => counts.ready_for_query += 1,
            _ => {}
        }
    }
}
#[derive(Default)]
struct State {
    counts: PgDiagnostics,
    sent: Frames,
    received: Frames,
}
pub(crate) struct Trace {
    state: Mutex<State>,
    query: AtomicBool,
}
impl Trace {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State::default()),
            query: AtomicBool::new(false),
        })
    }
    pub(crate) fn start(&self) -> io::Result<()> {
        self.change(|counts| counts.connections += 1)?;
        self.query.store(true, Ordering::Release);
        Ok(())
    }
    pub(crate) fn snapshot(&self) -> io::Result<PgDiagnostics> {
        Ok(self
            .state
            .lock()
            .map_err(|_| io::Error::other("wire observation lock"))?
            .counts)
    }
    pub(crate) fn change(&self, call: impl FnOnce(&mut PgDiagnostics)) -> io::Result<()> {
        call(
            &mut self
                .state
                .lock()
                .map_err(|_| io::Error::other("wire observation lock"))?
                .counts,
        );
        Ok(())
    }
    fn observe(&self, bytes: &[u8], sent: bool, network: bool, protocol: bool) -> io::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("wire observation lock"))?;
        if network {
            if sent {
                state.counts.wire_sent += bytes.len() as u64;
            } else {
                state.counts.wire_received += bytes.len() as u64;
            }
        }
        if protocol && self.query.load(Ordering::Acquire) {
            let State {
                counts,
                sent: outgoing,
                received,
                ..
            } = &mut *state;
            if sent {
                counts.protocol_sent += bytes.len() as u64;
                outgoing.feed(bytes, true, counts)?;
            } else {
                counts.protocol_received += bytes.len() as u64;
                received.feed(bytes, false, counts)?;
            }
        }
        Ok(())
    }
}
pub(crate) struct Observed<S> {
    pub(crate) inner: S,
    trace: Arc<Trace>,
    network: bool,
    protocol: bool,
}
impl<S> Observed<S> {
    pub(crate) fn new(inner: S, trace: Arc<Trace>, network: bool, protocol: bool) -> Self {
        Self {
            inner,
            trace,
            network,
            protocol,
        }
    }
}
impl<S: AsyncRead + Unpin> AsyncRead for Observed<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let before = buf.filled().len();
        let result = Pin::new(&mut this.inner).poll_read(cx, buf);
        if matches!(result, Poll::Ready(Ok(()))) {
            this.trace
                .observe(&buf.filled()[before..], false, this.network, this.protocol)?;
        }
        result
    }
}
impl<S: AsyncWrite + Unpin> AsyncWrite for Observed<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.inner).poll_write(cx, bytes);
        if let Poll::Ready(Ok(n)) = result {
            this.trace
                .observe(&bytes[..n], true, this.network, this.protocol)?;
        }
        result
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}
impl<S: tokio_postgres::tls::TlsStream + Unpin> tokio_postgres::tls::TlsStream for Observed<S> {
    fn channel_binding(&self) -> tokio_postgres::tls::ChannelBinding {
        self.inner.channel_binding()
    }
}
