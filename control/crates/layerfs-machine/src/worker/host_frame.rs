//! The bounded plain JSON frames of the host-worker read exchange.
use super::frame::ATTACHED;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::{home::host_socket, json, outcome::refused};
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

/// Largest host request body.
pub const HOST_MAX_REQUEST: usize = 64 << 10;
/// Largest host reply body.
pub const HOST_MAX_REPLY: usize = 1 << 20;
const DECODED_BUDGET: usize = 4 << 20;
const DEPTH: usize = 32;
const OBJECT_FIELDS: u16 = 128;

fn invalid(message: &str) -> SdkRefusal {
    refused("InvalidInput", format!("host worker frame: {message}"))
}

fn lost(error: io::Error) -> SdkRefusal {
    refused("Unavailable", format!("host worker connection: {error}"))
}

/// Bounds the allocations from JSON structure before the existing parser
/// constructs values. Object width also bounds its duplicate-name scan.
fn budget(body: &[u8]) -> Result<(), SdkRefusal> {
    let mut bytes = 3 * body.len();
    let mut fields = [0_u16; DEPTH];
    let mut depth = 0;
    let mut quoted = false;
    let mut escaped = false;
    for byte in body {
        if quoted {
            match (*byte, escaped) {
                (_, true) => escaped = false,
                (b'\\', false) => escaped = true,
                (b'"', false) => quoted = false,
                _ => {}
            }
            continue;
        }
        match *byte {
            b'"' => quoted = true,
            b'{' | b'[' => {
                let Some(level) = fields.get_mut(depth) else {
                    return Err(invalid("JSON nested too deeply"));
                };
                *level = 0;
                depth += 1;
                bytes += 256;
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            b':' if depth > 0 => {
                fields[depth - 1] += 1;
                if fields[depth - 1] > OBJECT_FIELDS {
                    return Err(invalid("too many fields in one JSON object"));
                }
                bytes += 256;
            }
            b',' => bytes += 256,
            _ => {}
        }
        if bytes > DECODED_BUDGET {
            return Err(invalid("JSON exceeds the decoded size budget"));
        }
    }
    Ok(())
}

struct Until<'a> {
    stream: &'a UnixStream,
    end: Option<Instant>,
}

fn remaining(end: Instant) -> io::Result<std::time::Duration> {
    let left = end.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err(io::ErrorKind::TimedOut.into());
    }
    Ok(left)
}

impl Read for Until<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        host_socket::read_before(self.stream, buffer, self.end)
    }
}

/// Reads one host frame by the absolute deadline. Checks the body length
/// and attachment flag before allocating. A clean EOF yields None.
pub fn read_host_frame(
    stream: &UnixStream,
    max: usize,
    end: Option<Instant>,
) -> Result<Option<SdkValue>, SdkRefusal> {
    let mut input = Until { stream, end };
    let mut header = [0u8; 4];
    if input.read(&mut header[..1]).map_err(lost)? == 0 {
        return Ok(None);
    }
    // The operation can take its time. Once the answer starts, its frame
    // must finish by a separate transport deadline.
    if input.end.is_none() {
        input.end = Some(Instant::now() + Duration::from_secs(10));
    }
    input.read_exact(&mut header[1..]).map_err(lost)?;
    let header = u32::from_be_bytes(header);
    if header & ATTACHED != 0 {
        return Err(invalid(
            "attachments are unavailable on the host read route",
        ));
    }
    let len = header as usize;
    if len > max.min(HOST_MAX_REPLY) {
        return Err(refused(
            "TooLarge",
            "host worker frame exceeds its byte ceiling",
        ));
    }
    let mut body = vec![0; len];
    let read = input.read_exact(&mut body).map_err(lost);
    let parsed = read.and_then(|()| {
        budget(&body)?;
        let text = std::str::from_utf8(&body).map_err(|_| invalid("JSON must be UTF-8"))?;
        json::parse(text)
            .map(Some)
            .map_err(|error| invalid(&error.0))
    });
    // a request that carries a login (#819 P-B) has its bytes overwritten
    // once decoded; so has a frame that did not decode, unread
    if !matches!(&parsed, Ok(Some(value)) if !carries_login(value)) {
        crate::wipe::bytes(body);
    }
    parsed
}

/// Whether host frame `value` carries a login: a request whose `args` hold
/// a `password` (#819 P-B). Only such frames are overwritten.
pub fn carries_login(value: &SdkValue) -> bool {
    (value.get("args")).is_some_and(|args| args.get("password").is_some())
}

/// Frees an encoded `frame`, overwritten first when it carries a login
/// (`secret`, [`carries_login`]).
pub(super) fn release(frame: Vec<u8>, secret: bool) {
    if secret {
        crate::wipe::bytes(frame);
    }
}

pub(super) fn encode(value: &SdkValue, max: usize) -> Result<Vec<u8>, SdkRefusal> {
    let body = json::print(value);
    let secret = carries_login(value);
    let fail = |body: String, refusal: SdkRefusal| {
        if secret {
            crate::wipe::text(body);
        }
        Err(refusal)
    };
    if body.len() > max.min(HOST_MAX_REPLY) {
        let refusal = refused("TooLarge", "host worker frame exceeds its byte ceiling");
        return fail(body, refusal);
    }
    if let Err(refusal) = budget(body.as_bytes()) {
        return fail(body, refusal);
    }
    let mut frame = Vec::with_capacity(4 + body.len());
    frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
    frame.extend_from_slice(body.as_bytes());
    if secret {
        crate::wipe::text(body);
    }
    Ok(frame)
}

pub(super) fn send(
    mut stream: &UnixStream,
    mut bytes: &[u8],
    end: Instant,
) -> Result<(), (bool, SdkRefusal)> {
    let mut started = false;
    while !bytes.is_empty() {
        stream
            .set_write_timeout(Some(
                remaining(end).map_err(|error| (started, lost(error)))?,
            ))
            .map_err(|error| (started, lost(error)))?;
        started = true;
        match stream.write(bytes) {
            Ok(0) => return Err((started, lost(io::ErrorKind::WriteZero.into()))),
            Ok(written) => bytes = &bytes[written..],
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err((started, lost(error))),
        }
    }
    Ok(())
}

/// Encodes and sends one plain JSON host frame by the absolute deadline.
pub fn write_host_frame(
    stream: &UnixStream,
    value: &SdkValue,
    max: usize,
    end: Instant,
) -> Result<(), SdkRefusal> {
    send(stream, &encode(value, max)?, end).map_err(|(_, error)| error)
}
