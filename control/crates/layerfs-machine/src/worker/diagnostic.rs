//! One observational hello and ping on a socket with an absolute deadline.
use super::{client::refusal_of, frame_bytes, read_frame_within, Endpoint, Hello, MAX_HELLO};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::{refused, Outcome};
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

/// An authenticated diagnostic connection; nothing is automatically retried.
pub struct DiagnosticWorker {
    stream: DeadlineStream,
    /// Authenticated protocol and bundle identity, checked before a ping.
    pub hello: Hello,
}

/// Connects and authenticates without refreshing records or starting a worker.
/// Every read/write, including a trickling frame, is bounded by `deadline`.
pub fn diagnostic_connect(
    endpoint: &Endpoint,
    deadline: Instant,
) -> Result<DiagnosticWorker, SdkRefusal> {
    if !endpoint.address.ip().is_loopback()
        || endpoint.address.port() == 0
        || endpoint.secret.is_empty()
    {
        return Err(refused(
            "InvalidInput",
            "worker endpoint must have a loopback address, nonzero port and secret",
        ));
    }
    let duration = remaining(deadline).map_err(io_refusal)?;
    let socket = TcpStream::connect_timeout(&endpoint.address, duration).map_err(io_refusal)?;
    let _ = socket.set_nodelay(true);
    let mut stream = DeadlineStream { socket, deadline };
    let secret = frame_bytes(&SdkValue::map([(
        "secret",
        SdkValue::text(&endpoint.secret),
    )]))?;
    if secret.len() > MAX_HELLO + 4 {
        return Err(refused(
            "InvalidInput",
            "worker secret exceeds hello frame limit",
        ));
    }
    stream.write_all(&secret).map_err(io_refusal)?;
    let frame = read_frame_within(&mut stream, MAX_HELLO)?
        .ok_or_else(|| refused("Unavailable", "worker closed before authenticating"))?;
    if let Some(value) = frame.get("refused") {
        return Err(refusal_of(value));
    }
    if frame.get("ready") != Some(&SdkValue::Bool(true)) {
        return Err(refused("Failed", "worker did not return a ready hello"));
    }
    Ok(DiagnosticWorker {
        hello: Hello::of(&frame),
        stream,
    })
}

impl DiagnosticWorker {
    /// Sends only the existing read-only worker.ping request, after compatibility.
    pub fn ping(mut self) -> Outcome {
        if let Some(refusal) = self.hello.mismatch(
            &self
                .stream
                .socket
                .peer_addr()
                .map_err(io_refusal)?
                .to_string(),
        ) {
            return Err(refusal);
        }
        let request = frame_bytes(&SdkValue::map([
            ("op", SdkValue::text("worker.ping")),
            ("args", SdkValue::map([("capacity", SdkValue::Bool(true))])),
        ]))?;
        self.stream.write_all(&request).map_err(io_refusal)?;
        let frame = read_frame_within(&mut self.stream, MAX_HELLO)?
            .ok_or_else(|| refused("Unavailable", "worker closed without answering ping"))?;
        if let Some(value) = frame.get("value") {
            return Ok(value.clone());
        }
        if let Some(value) = frame.get("refused") {
            return Err(refusal_of(value));
        }
        Err(refused("Failed", "worker returned no ping result"))
    }
}

fn io_refusal(error: io::Error) -> SdkRefusal {
    refused(
        if matches!(
            error.kind(),
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
        ) {
            "Timeout"
        } else {
            "Unavailable"
        },
        format!("worker diagnostic connection: {error}"),
    )
}

fn remaining(deadline: Instant) -> io::Result<Duration> {
    let duration = deadline.saturating_duration_since(Instant::now());
    if duration.is_zero() {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "diagnostic deadline exceeded",
        ))
    } else {
        Ok(duration)
    }
}

struct DeadlineStream {
    socket: TcpStream,
    deadline: Instant,
}
impl Read for DeadlineStream {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.socket
            .set_read_timeout(Some(remaining(self.deadline)?))?;
        self.socket.read(bytes)
    }
}
impl Write for DeadlineStream {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.socket
            .set_write_timeout(Some(remaining(self.deadline)?))?;
        self.socket.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.socket
            .set_write_timeout(Some(remaining(self.deadline)?))?;
        self.socket.flush()
    }
}
