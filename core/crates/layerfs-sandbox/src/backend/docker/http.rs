//! One-attempt local Engine HTTP exchange and original transfer custody.
use super::{
    body::{line, Body, Framing},
    headers::field,
    socket::Socket,
};
use crate::{ErrorDiagnostic, ExecId, RuntimeError, WireFailure};
use std::{
    io::{self, BufReader, Read, Write},
    net::Shutdown,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
/// Concrete local Docker Engine endpoint. Control waits never apply to command streams.
#[derive(Clone, Debug)]
pub struct Docker {
    pub(super) socket: PathBuf,
    pub(super) control_wait: Duration,
    pub(super) deadline: Option<Instant>,
}
pub(super) struct Response {
    pub status: u16,
    pub body: Body<BufReader<Socket>>,
    pub writer: Socket,
    pub sent: u64,
    header_bytes: u64,
    pub multiplexed: bool,
}
impl Docker {
    /// Selects an explicit local endpoint and blocking metadata I/O wait.
    pub fn new(socket: impl AsRef<Path>, control_wait: Duration) -> Result<Self, RuntimeError> {
        if !socket.as_ref().is_absolute() || control_wait.is_zero() {
            return Err(RuntimeError::Protocol(
                "explicit Engine endpoint/control wait",
            ));
        }
        Ok(Self {
            socket: socket.as_ref().to_owned(),
            control_wait,
            deadline: None,
        })
    }
    pub(super) fn exchange(
        &self,
        method: &str,
        path: &str,
        upgrade: bool,
        mut encode: impl FnMut(&mut dyn Write) -> io::Result<()>,
    ) -> Result<Response, Box<WireFailure>> {
        let mut count = Count(0);
        encode(&mut count).map_err(|e| failure(false, 0, None, e.into(), None))?;
        self.exchange_body(method, path, upgrade, "application/json", count.0, |w| {
            encode(w)
        })
    }
    /// Streams one known-length body once; no pre-read or file-sized allocation.
    pub(super) fn exchange_body(
        &self,
        method: &str,
        path: &str,
        upgrade: bool,
        media: &str,
        length: u64,
        encode: impl FnOnce(&mut dyn Write) -> io::Result<()>,
    ) -> Result<Response, Box<WireFailure>> {
        let mut stream = Socket::connect(&self.socket, self.control_wait, self.deadline)
            .map_err(|e| failure(false, 0, None, e.into(), None))?;
        let mut sent = 0u64;
        let mut attempted = false;
        let mut observed_status = None;
        let mut pending_request = crate::PendingRequest::default();
        let head=format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: {media}\r\nContent-Length: {length}\r\n{}",if upgrade {"Connection: Upgrade\r\nUpgrade: tcp\r\n\r\n"} else {"Connection: close\r\n\r\n"});
        let header_bytes = head.len() as u64;
        let result = (|| {
            {
                let sender = Sender {
                    stream: &mut stream,
                    sent: &mut sent,
                    attempted: &mut attempted,
                };
                let mut writer = BufferedSender {
                    inner: sender,
                    bytes: [0; 8192],
                    used: 0,
                };
                let encoded = (|| {
                    writer.write_all(head.as_bytes())?;
                    let mut body = ExactBody {
                        inner: &mut writer,
                        remaining: length,
                    };
                    encode(&mut body)?;
                    if body.remaining != 0 {
                        return Err(RuntimeError::Protocol("short request body"));
                    }
                    writer.flush()?;
                    Ok::<(), RuntimeError>(())
                })();
                if encoded.is_err() {
                    pending_request
                        .bytes
                        .extend_from_slice(&writer.bytes[..writer.used]);
                }
                encoded?;
            }
            let read = stream.try_clone()?;
            let mut reader = BufReader::with_capacity(8192, read);
            let (status, mode, multiplexed) = headers(&mut reader, upgrade, &mut observed_status)?;
            if upgrade && status == 101 {
                stream.wait = None;
                stream.deadline = None;
                reader.get_mut().wait = None;
                reader.get_mut().deadline = None;
            }
            Ok::<_, RuntimeError>((reader, status, mode, multiplexed))
        })();
        match result {
            Ok((reader, status, mode, multiplexed)) => Ok(Response {
                status,
                body: Body::new(reader, mode),
                writer: stream,
                sent,
                multiplexed,
                header_bytes,
            }),
            Err(cause) => {
                let fence_error = stream.stream.shutdown(Shutdown::Both).err();
                let mut original = failure(attempted, sent, observed_status, cause, fence_error);
                original.request_header_bytes = header_bytes;
                original.sent_body_bytes = sent.saturating_sub(header_bytes);
                original.pending_request = pending_request;
                Err(original)
            }
        }
    }
}
impl Response {
    pub fn failed_fence(&self, error: io::Error) -> Box<WireFailure> {
        let mut original = failure(
            true,
            self.sent,
            Some(self.status),
            RuntimeError::Protocol("original logs fence failed"),
            Some(error),
        );
        original.request_header_bytes = self.header_bytes;
        original.sent_body_bytes = self.sent.saturating_sub(self.header_bytes);
        original
    }
    pub fn fail(&mut self, cause: RuntimeError, observed: Option<ExecId>) -> Box<WireFailure> {
        let error_body = if matches!(cause, RuntimeError::Http(_)) {
            let mut bytes = [0; 1025];
            let mut used = 0;
            let mut complete = false;
            let mut read_error = None;
            while used < bytes.len() {
                match self.body.read(&mut bytes[used..]) {
                    Ok(0) => {
                        complete = true;
                        break;
                    }
                    Ok(n) => used += n,
                    Err(error) => {
                        read_error = Some(error);
                        break;
                    }
                }
            }
            Some(ErrorDiagnostic {
                prefix: bytes[..used.min(1024)].to_vec(),
                observed_bytes: used as u64,
                complete,
                read_error,
            })
        } else {
            None
        };
        let mut result = failure(
            true,
            self.sent,
            Some(self.status),
            cause,
            self.writer.stream.shutdown(Shutdown::Both).err(),
        );
        result.request_header_bytes = self.header_bytes;
        result.sent_body_bytes = self.sent.saturating_sub(self.header_bytes);
        result.observed_exec = observed;
        result.error_body = error_body;
        result
    }
}
pub(super) fn failure(
    attempted: bool,
    sent_bytes: u64,
    status: Option<u16>,
    cause: RuntimeError,
    fence_error: Option<io::Error>,
) -> Box<WireFailure> {
    Box::new(WireFailure {
        attempted,
        sent_bytes,
        request_header_bytes: 0,
        sent_body_bytes: 0,
        pending_request: Default::default(),
        status,
        cause,
        fence_error,
        observed_exec: None,
        observed_container: None,
        requested_container: None,
        requested_exec: None,
        error_body: None,
    })
}
struct Count(u64);
impl Write for Count {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "request length width"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct Sender<'a> {
    stream: &'a mut Socket,
    sent: &'a mut u64,
    attempted: &'a mut bool,
}
impl Write for Sender<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        *self.attempted = true;
        let n = self.stream.write(bytes)?;
        *self.sent = self.sent.saturating_add(n as u64);
        Ok(n)
    }
    fn write_all(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        while !bytes.is_empty() {
            let n = self.write(bytes)?;
            if n == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
            bytes = &bytes[n..];
        }
        Ok(())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}
/// Fixed request window; flushes retain exact socket positive progress and never retry errors.
struct BufferedSender<'a> {
    inner: Sender<'a>,
    bytes: [u8; 8192],
    used: usize,
}
impl Write for BufferedSender<'_> {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        if input.is_empty() {
            return Ok(0);
        }
        if self.used == self.bytes.len() {
            self.flush()?;
        }
        let n = input.len().min(self.bytes.len() - self.used);
        self.bytes[self.used..self.used + n].copy_from_slice(&input[..n]);
        self.used += n;
        Ok(n)
    }
    fn write_all(&mut self, mut input: &[u8]) -> io::Result<()> {
        while !input.is_empty() {
            let n = self.write(input)?;
            if n == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
            input = &input[n..];
        }
        Ok(())
    }
    fn flush(&mut self) -> io::Result<()> {
        let mut sent = 0;
        while sent < self.used {
            let result = self
                .inner
                .write(&self.bytes[sent..self.used])
                .and_then(|n| {
                    if n == 0 {
                        Err(io::ErrorKind::WriteZero.into())
                    } else {
                        Ok(n)
                    }
                });
            match result {
                Ok(n) => sent += n,
                Err(error) => {
                    self.bytes.copy_within(sent..self.used, 0);
                    self.used -= sent;
                    return Err(error);
                }
            }
        }
        self.used = 0;
        Ok(())
    }
}

fn headers(
    reader: &mut impl Read,
    upgrade: bool,
    observed: &mut Option<u16>,
) -> Result<(u16, Framing, bool), RuntimeError> {
    let start = line(reader, 8192)?;
    let mut pieces = start.splitn(3, |b| *b == b' ');
    if pieces.next() != Some(b"HTTP/1.1".as_slice()) {
        return Err(RuntimeError::Protocol("Engine HTTP version"));
    }
    let status = pieces.next().ok_or(RuntimeError::Protocol("HTTP status"))?;
    if status.len() != 3 || !status.iter().all(u8::is_ascii_digit) {
        return Err(RuntimeError::Protocol("HTTP status digits"));
    }
    let status = u16::from(status[0] - b'0') * 100
        + u16::from(status[1] - b'0') * 10
        + u16::from(status[2] - b'0');
    if !(100..=599).contains(&status) {
        return Err(RuntimeError::Protocol("HTTP status range"));
    }
    let reason = pieces
        .next()
        .ok_or(RuntimeError::Protocol("HTTP status separator"))?;
    if reason.iter().any(|b| (*b < 32 && *b != b'\t') || *b == 127) {
        return Err(RuntimeError::Protocol("HTTP status reason"));
    }
    *observed = Some(status);
    let mut length = None;
    let mut chunked = false;
    let mut connection = None;
    let mut protocol = None;
    let mut media = None;
    let mut total = start.len() + 2;
    loop {
        let row = line(reader, 8192)?;
        total += row.len() + 2;
        if total > 32768 {
            return Err(RuntimeError::Protocol("HTTP header metadata window"));
        }
        if row.is_empty() {
            break;
        }
        let (name, value) = field(&row)?;
        if name.eq_ignore_ascii_case(b"content-length") {
            if length.is_some() || value.is_empty() || !value.iter().all(u8::is_ascii_digit) {
                return Err(RuntimeError::Protocol("HTTP length header"));
            }
            let mut n = 0u64;
            for &b in value {
                n = n
                    .checked_mul(10)
                    .and_then(|n| n.checked_add(u64::from(b - b'0')))
                    .ok_or(RuntimeError::Protocol("HTTP length width"))?;
            }
            length = Some(n);
        } else if name.eq_ignore_ascii_case(b"transfer-encoding") {
            if chunked || !value.eq_ignore_ascii_case(b"chunked") {
                return Err(RuntimeError::Protocol("HTTP transfer encoding"));
            }
            chunked = true;
        } else if name.eq_ignore_ascii_case(b"content-type") {
            if media
                .replace(value.eq_ignore_ascii_case(b"application/vnd.docker.multiplexed-stream"))
                .is_some()
            {
                return Err(RuntimeError::Protocol("duplicate stream media type"));
            }
        } else if name.eq_ignore_ascii_case(b"connection") {
            if connection
                .replace(value.eq_ignore_ascii_case(b"upgrade"))
                .is_some()
            {
                return Err(RuntimeError::Protocol("duplicate HTTP connection"));
            }
        } else if name.eq_ignore_ascii_case(b"upgrade")
            && protocol
                .replace(value.eq_ignore_ascii_case(b"tcp"))
                .is_some()
        {
            return Err(RuntimeError::Protocol("duplicate HTTP upgrade"));
        }
    }
    if chunked && length.is_some() {
        return Err(RuntimeError::Protocol("ambiguous HTTP framing"));
    }
    if status == 101 {
        if !upgrade
            || connection != Some(true)
            || protocol != Some(true)
            || media != Some(true)
            || chunked
            || length.is_some()
        {
            return Err(RuntimeError::Protocol("Engine upgrade framing"));
        }
        return Ok((status, Framing::Hijacked, true));
    }
    if status == 204 {
        if chunked || length.is_some_and(|n| n != 0) {
            return Err(RuntimeError::Protocol("HTTP no-body framing"));
        }
        return Ok((status, Framing::Fixed(0), false));
    }
    if status < 200 {
        return Err(RuntimeError::Protocol(
            "unsupported informational HTTP response",
        ));
    }
    Ok((
        status,
        if chunked {
            Framing::Chunked
        } else if let Some(n) = length {
            Framing::Fixed(n)
        } else {
            Framing::Close
        },
        media == Some(true),
    ))
}

/// Rejects a local encoder exceeding its declared body before those extra bytes enter the wire.
struct ExactBody<'a> {
    inner: &'a mut dyn Write,
    remaining: u64,
}
impl Write for ExactBody<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "request body exceeded length",
            ));
        }
        let n = self.inner.write(bytes)?;
        self.remaining -= n as u64;
        Ok(n)
    }
    fn write_all(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        while !bytes.is_empty() {
            let n = self.write(bytes)?;
            if n == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
            bytes = &bytes[n..];
        }
        Ok(())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
