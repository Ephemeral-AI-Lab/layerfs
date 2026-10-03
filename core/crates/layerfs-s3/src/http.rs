//! Bounded HTTP/1.1 framing over one persistent, observed socket.
use crate::{config::S3Config, counters::S3Diagnostics};
use layerfs_storage::port::ObjectError;
use std::{
    collections::BTreeMap,
    io::{self, BufRead, BufReader, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    time::Instant,
};
const HEADER_LIMIT: usize = 16 * 1024;
pub(crate) struct Reply {
    pub(crate) status: u16,
    pub(crate) headers: BTreeMap<String, String>,
    pub(crate) body: Vec<u8>,
    pub(crate) close: bool,
}
struct Observed {
    socket: TcpStream,
    deadline: Instant,
    counts: S3Diagnostics,
}
impl Observed {
    fn remaining(&self) -> io::Result<std::time::Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|left| !left.is_zero())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "object request deadline"))
    }
}
impl Read for Observed {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.socket.set_read_timeout(Some(self.remaining()?))?;
        let n = self.socket.read(out)?;
        self.counts.wire_received += n as u64;
        Ok(n)
    }
}
impl Write for Observed {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.socket.set_write_timeout(Some(self.remaining()?))?;
        let n = self.socket.write(bytes)?;
        self.counts.wire_sent += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.socket.flush()
    }
}
pub(crate) struct Http {
    reader: BufReader<Observed>,
}
impl Http {
    pub(crate) fn connect(config: &S3Config) -> Result<Self, ObjectError> {
        let started = Instant::now();
        let (host, port, _) = config.address()?;
        // Choose one IPv4 address for the declared local profile before attempting
        // the connection. A failed address is never replaced or retried.
        let address = (host.as_str(), port)
            .to_socket_addrs()
            .map_err(|_| ObjectError::Uncertain)?
            .find(|address| address.is_ipv4())
            .ok_or(ObjectError::Malformed)?;
        let socket = TcpStream::connect_timeout(&address, config.connect_timeout)
            .map_err(|_| ObjectError::Uncertain)?;
        socket
            .set_nodelay(true)
            .map_err(|_| ObjectError::Uncertain)?;
        let deadline = Instant::now()
            .checked_add(config.request_timeout)
            .ok_or(ObjectError::Malformed)?;
        Ok(Self {
            reader: BufReader::new(Observed {
                socket,
                deadline,
                counts: S3Diagnostics {
                    connections: 1,
                    connect_ns: started.elapsed().as_nanos() as u64,
                    ..Default::default()
                },
            }),
        })
    }
    pub(crate) fn note_local(&mut self, method: &str, signing_ns: u64, validation_ns: u64) {
        let row = &mut self.reader.get_mut().counts.request_work[crate::work::index(method)];
        row.signing_ns += signing_ns;
        row.validation_ns += validation_ns;
    }
    pub(crate) fn diagnostics(&self) -> S3Diagnostics {
        self.reader.get_ref().counts
    }
    pub(crate) fn request(
        &mut self,
        config: &S3Config,
        method: &str,
        path: &str,
        headers: &str,
        body: &[u8],
    ) -> Result<Reply, ObjectError> {
        self.reader.get_mut().deadline = Instant::now()
            .checked_add(config.request_timeout)
            .ok_or(ObjectError::Malformed)?;
        let counts = &mut self.reader.get_mut().counts;
        counts.requests += 1;
        counts.request_work[crate::work::index(method)].calls += 1;
        match method {
            "PUT" => counts.puts += 1,
            "GET" => counts.gets += 1,
            "HEAD" => counts.heads += 1,
            _ => return Err(ObjectError::Malformed),
        }
        // These bounded bodies are already available: send them immediately,
        // then require the same complete conditional-create acknowledgement.
        let request=format!("{method} {path} HTTP/1.1\r\n{headers}Content-Length: {}\r\nConnection: keep-alive\r\n\r\n",body.len());
        let started = Instant::now();
        self.reader
            .get_mut()
            .write_all(request.as_bytes())
            .map_err(|_| ObjectError::Uncertain)?;
        self.reader
            .get_mut()
            .flush()
            .map_err(|_| ObjectError::Uncertain)?;
        let elapsed = started.elapsed().as_nanos() as u64;
        self.reader.get_mut().counts.header_write_ns += elapsed;
        self.reader.get_mut().counts.request_work[crate::work::index(method)].header_write_ns +=
            elapsed;
        let started = Instant::now();
        let mut written = 0;
        while written < body.len() {
            let n = self
                .reader
                .get_mut()
                .write(&body[written..])
                .map_err(|_| ObjectError::Uncertain)?;
            if n == 0 {
                return Err(ObjectError::Uncertain);
            }
            written += n;
            self.reader.get_mut().counts.body_sent += n as u64;
            self.reader.get_mut().counts.request_work[0].body_sent += n as u64;
        }
        self.reader
            .get_mut()
            .flush()
            .map_err(|_| ObjectError::Uncertain)?;
        let elapsed = started.elapsed().as_nanos() as u64;
        self.reader.get_mut().counts.body_write_ns += elapsed;
        self.reader.get_mut().counts.request_work[crate::work::index(method)].body_write_ns +=
            elapsed;
        let started = Instant::now();
        let head = self.read_head();
        let elapsed = started.elapsed().as_nanos() as u64;
        self.reader.get_mut().counts.response_head_ns += elapsed;
        self.reader.get_mut().counts.request_work[crate::work::index(method)].response_head_ns +=
            elapsed;
        let (status, headers) = head?;
        if status == 100 {
            return Err(ObjectError::Malformed);
        }
        self.response(config, method, status, headers)
    }
    fn read_head(&mut self) -> Result<(u16, BTreeMap<String, String>), ObjectError> {
        let mut remaining = HEADER_LIMIT;
        let line = self.line(&mut remaining)?;
        let status = std::str::from_utf8(&line).map_err(|_| ObjectError::Malformed)?;
        let mut words = status.splitn(3, ' ');
        if words.next() != Some("HTTP/1.1") {
            return Err(ObjectError::Malformed);
        }
        let status = words
            .next()
            .filter(|s| s.len() == 3)
            .ok_or(ObjectError::Malformed)?
            .parse::<u16>()
            .map_err(|_| ObjectError::Malformed)?;
        if status != 100 && !(200..=599).contains(&status) {
            return Err(ObjectError::Malformed);
        }
        let mut headers = BTreeMap::new();
        loop {
            let line = self.line(&mut remaining)?;
            if line.is_empty() {
                break;
            }
            let line = std::str::from_utf8(&line).map_err(|_| ObjectError::Malformed)?;
            let (name, value) = line.split_once(':').ok_or(ObjectError::Malformed)?;
            if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
                return Err(ObjectError::Malformed);
            }
            let name = name.to_ascii_lowercase();
            let value = value.trim().to_owned();
            if headers.contains_key(&name)
                && matches!(
                    name.as_str(),
                    "content-length" | "transfer-encoding" | "content-range" | "connection"
                )
            {
                return Err(ObjectError::Malformed);
            }
            headers.insert(name, value);
        }
        Ok((status, headers))
    }
    fn response(
        &mut self,
        config: &S3Config,
        method: &str,
        status: u16,
        headers: BTreeMap<String, String>,
    ) -> Result<Reply, ObjectError> {
        let started = Instant::now();
        let close = match headers.get("connection") {
            None => false,
            Some(value) if value.eq_ignore_ascii_case("keep-alive") => false,
            Some(value) if value.eq_ignore_ascii_case("close") => true,
            Some(_) => return Err(ObjectError::Malformed),
        };
        let mut body = Vec::new();
        if method != "HEAD" {
            match (
                headers.get("content-length"),
                headers.get("transfer-encoding"),
            ) {
                (Some(length), None) => {
                    let length = parse_length(length)?;
                    if length > config.max_body_bytes {
                        return Err(ObjectError::Malformed);
                    }
                    self.read_body(length, &mut body)?;
                }
                (None, Some(encoding)) if encoding.eq_ignore_ascii_case("chunked") => {
                    self.chunked(config.max_body_bytes, &mut body)?
                }
                _ => return Err(ObjectError::Malformed),
            }
        }
        let elapsed = started.elapsed().as_nanos() as u64;
        self.reader.get_mut().counts.response_body_ns += elapsed;
        self.reader.get_mut().counts.request_work[crate::work::index(method)].response_body_ns +=
            elapsed;
        self.reader.get_mut().counts.request_work[crate::work::index(method)].body_received +=
            body.len() as u64;
        Ok(Reply {
            status,
            headers,
            body,
            close,
        })
    }
    fn line(&mut self, remaining: &mut usize) -> Result<Vec<u8>, ObjectError> {
        let mut line = Vec::new();
        let n = (&mut self.reader)
            .take(*remaining as u64 + 1)
            .read_until(b'\n', &mut line)
            .map_err(|_| ObjectError::Uncertain)?;
        if n == 0 {
            return Err(ObjectError::Uncertain);
        }
        if n > *remaining || !line.ends_with(b"\r\n") {
            return Err(ObjectError::Malformed);
        }
        *remaining -= n;
        line.truncate(n - 2);
        Ok(line)
    }
    fn read_body(&mut self, length: usize, out: &mut Vec<u8>) -> Result<(), ObjectError> {
        let old = out.len();
        out.resize(old + length, 0);
        let mut position = old;
        while position < out.len() {
            let n = self
                .reader
                .read(&mut out[position..])
                .map_err(|_| ObjectError::Uncertain)?;
            if n == 0 {
                return Err(ObjectError::Uncertain);
            }
            position += n;
            self.reader.get_mut().counts.body_received += n as u64;
        }
        Ok(())
    }
    fn chunked(&mut self, limit: usize, out: &mut Vec<u8>) -> Result<(), ObjectError> {
        loop {
            let mut remaining = 128;
            let line = self.line(&mut remaining)?;
            let text = std::str::from_utf8(&line).map_err(|_| ObjectError::Malformed)?;
            let count = text.split(';').next().ok_or(ObjectError::Malformed)?;
            if count.is_empty() || count.len() > 16 || !count.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err(ObjectError::Malformed);
            }
            let count = usize::from_str_radix(count, 16).map_err(|_| ObjectError::Malformed)?;
            if count == 0 {
                let mut remaining = HEADER_LIMIT;
                while !self.line(&mut remaining)?.is_empty() {}
                return Ok(());
            }
            if out.len().checked_add(count).is_none_or(|n| n > limit) {
                return Err(ObjectError::Malformed);
            }
            self.read_body(count, out)?;
            let mut end = [0; 2];
            self.reader
                .read_exact(&mut end)
                .map_err(|_| ObjectError::Uncertain)?;
            if end != *b"\r\n" {
                return Err(ObjectError::Malformed);
            }
        }
    }
}
pub(crate) fn parse_length(value: &str) -> Result<usize, ObjectError> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ObjectError::Malformed);
    }
    value.parse().map_err(|_| ObjectError::Malformed)
}
