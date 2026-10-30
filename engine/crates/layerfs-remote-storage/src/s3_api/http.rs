//! One HTTP/1.1 request and its reply, with a streamed file body and a
//! streamed reply body, on a kept-alive connection of the client or a new
//! one ([`super::connection`], #510). Revived from the retired `layerfs-s3`
//! framing; bodies no longer have to fit in memory. An `https://` endpoint
//! runs the same exchange over TLS (`tls` feature).
//!
//! Failures before the first byte is written, the connect and TLS handshake
//! included, are `Unavailable`; after it, any failure to complete the
//! exchange is `Uncertain`, and the request is never sent again, on this
//! connection or another. A reply read to its exact end leaves its
//! connection to the client's next request. A file body's bytes are
//! reported to the thread's transfer progress as they are written.
use crate::driver::Span;
use crate::error::{local, RemoteError};
use crate::s3_api::config::S3Config;
use crate::s3_api::connection::{Connection, Kept};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{TcpStream, ToSocketAddrs};

const HEADER_LIMIT: usize = 16 * 1024;
/// Largest reply body read into memory (error and XML replies).
pub(crate) const SMALL_BODY: u64 = 4 * 1024 * 1024;
const BUFFER: usize = 256 * 1024;

/// A request body.
pub(crate) enum Body<'a> {
    Empty,
    Bytes(&'a [u8]),
    File { file: &'a mut File, span: Span },
}

impl Body<'_> {
    fn length(&self) -> u64 {
        match self {
            Self::Empty => 0,
            Self::Bytes(bytes) => bytes.len() as u64,
            Self::File { span, .. } => span.length,
        }
    }
}

/// Where a reply body goes: into `out` when the status is `status`,
/// otherwise into memory.
pub(crate) struct Sink<'a> {
    pub(crate) status: u16,
    pub(crate) out: &'a mut dyn Write,
}

pub(crate) struct Reply {
    pub(crate) status: u16,
    pub(crate) headers: BTreeMap<String, String>,
    /// The body when it was not streamed.
    pub(crate) body: Vec<u8>,
    /// Bytes written to the sink.
    pub(crate) streamed: u64,
}

enum Framing {
    Length(u64),
    Chunked,
    Close,
    None,
}

/// Opens a new connection to the endpoint; nothing is sent yet.
fn connect(config: &S3Config, host: &str, port: u16, tls: bool) -> Result<Connection, RemoteError> {
    let unavailable = |e: io::Error| RemoteError::Unavailable(format!("{host}:{port}: {e}"));
    let address = (host, port)
        .to_socket_addrs()
        .map_err(unavailable)?
        .next()
        .ok_or_else(|| RemoteError::Unavailable(format!("{host} has no address")))?;
    let socket =
        TcpStream::connect_timeout(&address, config.connect_timeout).map_err(unavailable)?;
    let configure = |socket: &TcpStream| {
        socket.set_nodelay(true)?;
        socket.set_read_timeout(Some(config.io_timeout))?;
        socket.set_write_timeout(Some(config.io_timeout))
    };
    configure(&socket).map_err(unavailable)?;
    Connection::open(socket, host, tls).map_err(RemoteError::Unavailable)
}

/// Sends one request, on a connection `kept` holds or a new one, and reads
/// its reply; a connection the reply leaves reusable goes back to `kept`.
pub(crate) fn exchange(
    config: &S3Config,
    kept: &Kept,
    method: &str,
    target: &str,
    headers: &BTreeMap<String, String>,
    mut body: Body<'_>,
    sink: Option<Sink<'_>>,
) -> Result<Reply, RemoteError> {
    let (host, port, _, tls) = config.address()?;
    if let Body::File { file, span } = &mut body {
        file.seek(SeekFrom::Start(span.offset))
            .map_err(|e| local("seek", e))?;
    }
    let path = target.split('?').next().unwrap_or(target);
    let (mut connection, what) = match kept.take() {
        Some((connection, idle)) => (
            connection,
            format!(
                "{method} {path} (on a connection kept {} ms)",
                idle.as_millis()
            ),
        ),
        None => (
            connect(config, &host, port, tls)?,
            format!("{method} {path}"),
        ),
    };
    let uncertain = |why: String| RemoteError::Uncertain(format!("{what}: {why}"));
    let mut head = format!("{method} {target} HTTP/1.1\r\n");
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str(&format!("content-length: {}\r\n\r\n", body.length()));
    let writer = &mut connection;
    writer
        .write_all(head.as_bytes())
        .map_err(|e| uncertain(format!("send: {e}")))?;
    match body {
        Body::Empty => {}
        Body::Bytes(bytes) => writer
            .write_all(bytes)
            .map_err(|e| uncertain(format!("send: {e}")))?,
        Body::File { file, span } => {
            let mut buffer = vec![0; BUFFER];
            let mut left = span.length;
            while left > 0 {
                let want = left.min(BUFFER as u64) as usize;
                let n = file
                    .read(&mut buffer[..want])
                    .map_err(|e| uncertain(format!("source read mid-request: {e}")))?;
                if n == 0 {
                    return Err(uncertain("source shrank mid-request".to_owned()));
                }
                writer
                    .write_all(&buffer[..n])
                    .map_err(|e| uncertain(format!("send: {e}")))?;
                crate::progress::add(n as u64);
                left -= n as u64;
            }
        }
    }
    writer
        .flush()
        .map_err(|e| uncertain(format!("send: {e}")))?;
    let mut reader = BufReader::with_capacity(BUFFER, &mut connection);
    let (version, status, headers) = read_head(&mut reader).map_err(uncertain)?;
    let framing = if method == "HEAD" || status == 204 || status == 304 {
        Framing::None
    } else {
        match (
            headers.get("content-length"),
            headers.get("transfer-encoding"),
        ) {
            (Some(length), None) => Framing::Length(parse_length(length).map_err(uncertain)?),
            (None, Some(coding)) if coding.eq_ignore_ascii_case("chunked") => Framing::Chunked,
            (None, None) => Framing::Close,
            _ => return Err(uncertain("ambiguous reply framing".to_owned())),
        }
    };
    let until_close = matches!(framing, Framing::Close);
    let mut memory = Vec::new();
    let streamed = match sink {
        Some(sink) if sink.status == status => {
            read_body(&mut reader, framing, sink.out, u64::MAX).map_err(|e| e.into(&what))?
        }
        _ => {
            read_body(&mut reader, framing, &mut memory, SMALL_BODY).map_err(|e| e.into(&what))?;
            0
        }
    };
    // Kept only when the reply ended exactly where its framing said, nothing
    // more was read, and neither side asked to close.
    let reusable = version == "HTTP/1.1"
        && !until_close
        && reader.buffer().is_empty()
        && !headers.get("connection").is_some_and(|value| {
            value
                .split(',')
                .any(|token| token.trim().eq_ignore_ascii_case("close"))
        });
    drop(reader);
    if reusable {
        kept.put(connection);
    }
    Ok(Reply {
        status,
        headers,
        body: memory,
        streamed,
    })
}

/// A body failure: the network (`Uncertain`) or the local sink (`Local`).
enum BodyError {
    Network(String),
    Sink(io::Error),
}

impl BodyError {
    fn into(self, what: &str) -> RemoteError {
        match self {
            Self::Network(why) => RemoteError::Uncertain(format!("{what}: {why}")),
            Self::Sink(error) => local("write download", error),
        }
    }
}

fn read_body(
    reader: &mut impl BufRead,
    framing: Framing,
    out: &mut dyn Write,
    limit: u64,
) -> Result<u64, BodyError> {
    let network = |e: io::Error| BodyError::Network(format!("reply body: {e}"));
    let mut total = 0_u64;
    let mut copy = |reader: &mut dyn BufRead, length: Option<u64>| -> Result<(), BodyError> {
        let mut left = length;
        loop {
            if left == Some(0) {
                return Ok(());
            }
            let chunk = reader.fill_buf().map_err(network)?;
            if chunk.is_empty() {
                return match left {
                    None => Ok(()),
                    Some(left) => Err(BodyError::Network(format!(
                        "reply body ended early: the server closed the connection {left} bytes short"
                    ))),
                };
            }
            let n = left.map_or(chunk.len(), |left| chunk.len().min(left as usize));
            total += n as u64;
            if total > limit {
                return Err(BodyError::Network("reply body too large".to_owned()));
            }
            out.write_all(&chunk[..n]).map_err(BodyError::Sink)?;
            reader.consume(n);
            left = left.map(|left| left - n as u64);
        }
    };
    match framing {
        Framing::None => {}
        Framing::Length(length) => copy(reader, Some(length))?,
        Framing::Close => copy(reader, None)?,
        Framing::Chunked => loop {
            let mut remaining = 128;
            let line = read_line(reader, &mut remaining).map_err(BodyError::Network)?;
            let size = line.split(';').next().unwrap_or_default();
            if size.is_empty() || size.len() > 16 || !size.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(BodyError::Network("chunk size".to_owned()));
            }
            let size = u64::from_str_radix(size, 16)
                .map_err(|_| BodyError::Network("chunk size".to_owned()))?;
            if size == 0 {
                let mut remaining = HEADER_LIMIT;
                while !read_line(reader, &mut remaining)
                    .map_err(BodyError::Network)?
                    .is_empty()
                {}
                break;
            }
            copy(reader, Some(size))?;
            let mut end = [0; 2];
            reader.read_exact(&mut end).map_err(network)?;
            if end != *b"\r\n" {
                return Err(BodyError::Network("chunk end".to_owned()));
            }
        },
    }
    Ok(total)
}

/// A reply's HTTP version, status and headers (names in lower case).
type Head = (&'static str, u16, BTreeMap<String, String>);

fn read_head(reader: &mut impl BufRead) -> Result<Head, String> {
    let mut remaining = HEADER_LIMIT;
    let line = read_line(reader, &mut remaining)?;
    let mut words = line.splitn(3, ' ');
    let version = match words.next() {
        Some("HTTP/1.1") => "HTTP/1.1",
        Some("HTTP/1.0") => "HTTP/1.0",
        _ => return Err(format!("reply status line {line:?}")),
    };
    let status = words
        .next()
        .filter(|s| s.len() == 3)
        .and_then(|s| s.parse::<u16>().ok())
        .filter(|s| (200..=599).contains(s))
        .ok_or_else(|| format!("reply status line {line:?}"))?;
    let mut headers = BTreeMap::new();
    loop {
        let line = read_line(reader, &mut remaining)?;
        if line.is_empty() {
            return Ok((version, status, headers));
        }
        let (name, value) = line.split_once(':').ok_or("reply header")?;
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return Err("reply header name".to_owned());
        }
        let name = name.to_ascii_lowercase();
        if headers.contains_key(&name)
            && matches!(
                name.as_str(),
                "content-length" | "transfer-encoding" | "content-range"
            )
        {
            return Err(format!("repeated reply header {name}"));
        }
        headers.insert(name, value.trim().to_owned());
    }
}

fn read_line(reader: &mut impl BufRead, remaining: &mut usize) -> Result<String, String> {
    let mut line = Vec::new();
    let n = (&mut *reader)
        .take(*remaining as u64 + 1)
        .read_until(b'\n', &mut line)
        .map_err(|e| format!("reply: {e}"))?;
    if n == 0 {
        return Err("connection closed before the reply".to_owned());
    }
    if n > *remaining || !line.ends_with(b"\r\n") {
        return Err("reply line too long or unterminated".to_owned());
    }
    *remaining -= n;
    line.truncate(n - 2);
    String::from_utf8(line).map_err(|_| "reply line is not UTF-8".to_owned())
}

pub(crate) fn parse_length(value: &str) -> Result<u64, String> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("length {value:?}"));
    }
    value.parse().map_err(|_| format!("length {value:?}"))
}
