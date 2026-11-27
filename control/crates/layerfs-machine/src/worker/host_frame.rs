//! The bounded plain JSON frames of the host-worker read exchange.
use super::frame::ATTACHED;
use layerfs_protocol::provision::install::{SdkRefusal, SdkValue};
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

/// The decoded budget of one frame's JSON, for a value read as a stream:
/// a spilled `file apply` change is held to the same bounds as a frame
/// (§11: 4 MiB decoded per exchange).
pub const FRAME_LIMITS: json::Limits = json::Limits {
    decoded: DECODED_BUDGET,
    fields: OBJECT_FIELDS as usize,
    depth: DEPTH,
};

/// Whether JSON text `body` is within the frame budget: what the worker
/// accepts as one change, however it is framed. The CLI checks a change
/// before it sends it.
pub fn within_budget(body: &[u8]) -> bool {
    budget(body).is_ok()
}

/// Whether change `value`, printed as JSON in the change frame the worker
/// decodes (`{"change": value}`), would be [`within_budget`], computed
/// from the value without printing it: the CLI checks a `file apply`
/// change this way before sending, holding no copy of its text. The frame
/// adds one level and its own cost; a spilled change, decoded without it,
/// is then within the budget too.
pub fn value_within_budget(value: &SdkValue) -> bool {
    let mut cost = Cost { bytes: 0 };
    cost.container(0, 1, true).is_some()
        && cost.text(b"change").is_some()
        && cost.value(value, 1).is_some()
}

/// [`budget`]'s count over a value: three per printed byte, 256 per
/// container, field and comma; `None` once past a bound.
struct Cost {
    bytes: usize,
}

impl Cost {
    fn add(&mut self, bytes: usize) -> Option<()> {
        self.bytes = self.bytes.checked_add(bytes)?;
        (self.bytes <= DECODED_BUDGET).then_some(())
    }

    /// A container at nesting `depth` (0 for the outermost) with `items`
    /// items: its brackets, its commas and, for an object, its colons.
    fn container(&mut self, depth: usize, items: usize, object: bool) -> Option<()> {
        if depth >= DEPTH || (object && items > OBJECT_FIELDS as usize) {
            return None;
        }
        let separators = items.saturating_sub(1) + if object { items } else { 0 };
        self.add(3 * (2 + separators) + 256 * (1 + separators))
    }

    fn text(&mut self, text: &[u8]) -> Option<()> {
        let escaped: usize = text
            .iter()
            .map(|byte| match byte {
                b'"' | b'\\' | b'\n' | b'\r' | b'\t' => 1,
                0..=0x1f => 5,
                _ => 0,
            })
            .sum();
        self.add(3 * (text.len() + escaped + 2))
    }

    fn value(&mut self, value: &SdkValue, depth: usize) -> Option<()> {
        match value {
            SdkValue::Null => self.add(3 * 4),
            SdkValue::Bool(true) => self.add(3 * 4),
            SdkValue::Bool(false) => self.add(3 * 5),
            SdkValue::Int(number) => self.add(3 * number.to_string().len()),
            SdkValue::Text(text) => self.text(text.as_bytes()),
            SdkValue::Bytes(bytes) => match std::str::from_utf8(bytes) {
                Ok(_) => self.text(bytes),
                // {"base64":"..."}: one object, one field
                Err(_) => {
                    self.container(depth, 1, true)?;
                    self.text(b"base64")?;
                    self.add(3 * (bytes.len().div_ceil(3) * 4 + 2))
                }
            },
            SdkValue::List(items) => {
                self.container(depth, items.len(), false)?;
                items
                    .iter()
                    .try_for_each(|item| self.value(item, depth + 1))
            }
            SdkValue::Map(fields) => {
                self.container(depth, fields.len(), true)?;
                fields.iter().try_for_each(|(name, item)| {
                    self.text(name.as_bytes())?;
                    self.value(item, depth + 1)
                })
            }
        }
    }
}

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
        layerfs_home::wipe::bytes(body);
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
        layerfs_home::wipe::bytes(frame);
    }
}

/// One JSON frame body, its decoded size bounded before it is parsed.
pub(super) fn decode(body: &[u8]) -> Result<SdkValue, SdkRefusal> {
    budget(body)?;
    let text = std::str::from_utf8(body).map_err(|_| invalid("JSON must be UTF-8"))?;
    json::parse(text).map_err(|error| invalid(&error.0))
}

pub(super) fn encode(value: &SdkValue, max: usize) -> Result<Vec<u8>, SdkRefusal> {
    let body = json::print(value);
    let secret = carries_login(value);
    let fail = |body: String, refusal: SdkRefusal| {
        if secret {
            layerfs_home::wipe::text(body);
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
        layerfs_home::wipe::text(body);
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
