//! One frame: a 4-byte big-endian length, then that many bytes of JSON.
//! Bytes travel as `{"base64": ...}` so a value keeps its type.
use crate::common::json;
use crate::common::outcome::refused;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

/// The largest frame either side accepts.
pub const MAX_FRAME: usize = 512 << 20;

/// The largest first frame: the caller's secret, or the worker's answer to
/// it. The worker reads no more than this before the secret matches.
pub const MAX_HELLO: usize = 4 << 10;

/// `value` as the bytes of one frame; a value larger than [`MAX_FRAME`] is
/// refused `TooLarge`, before anything is sent.
pub fn frame_bytes(value: &SdkValue) -> Result<Vec<u8>, SdkRefusal> {
    let text = json::print(&typed(value.clone()));
    if text.len() > MAX_FRAME {
        return Err(refused(
            "TooLarge",
            "a frame larger than the worker accepts; nothing was sent",
        ));
    }
    let mut bytes = Vec::with_capacity(4 + text.len());
    bytes.extend_from_slice(&(text.len() as u32).to_be_bytes());
    bytes.extend_from_slice(text.as_bytes());
    Ok(bytes)
}

/// Writes `value` as one frame.
pub fn write_frame(out: &mut impl Write, value: &SdkValue) -> std::io::Result<()> {
    let bytes = frame_bytes(value).map_err(|refusal| std::io::Error::other(refusal.message))?;
    out.write_all(&bytes)?;
    out.flush()
}

/// Reads one frame of at most [`MAX_FRAME`] bytes; `Ok(None)` at a clean
/// end before any byte.
pub fn read_frame(input: &mut impl Read) -> Result<Option<SdkValue>, SdkRefusal> {
    read_frame_within(input, MAX_FRAME)
}

/// Reads one frame of at most `max` bytes. A longer one is refused
/// `TooLarge` from its length alone: none of its body is read.
pub fn read_frame_within(
    input: &mut impl Read,
    max: usize,
) -> Result<Option<SdkValue>, SdkRefusal> {
    let Some(length) = read_length(input)? else {
        return Ok(None);
    };
    let length = length_within(length, max)?;
    read_body(input, length).map(Some)
}

/// The 4-byte length of the next frame, big-endian; `Ok(None)` at a clean
/// end before any byte.
fn read_length(input: &mut impl Read) -> Result<Option<usize>, SdkRefusal> {
    let mut length = [0u8; 4];
    match input.read(&mut length[..1]) {
        Ok(0) => return Ok(None),
        Ok(_) => {}
        Err(error) => return Err(connection(error)),
    }
    input.read_exact(&mut length[1..]).map_err(connection)?;
    Ok(Some(u32::from_be_bytes(length) as usize))
}

/// `length`, if a frame of that size is allowed.
fn length_within(length: usize, max: usize) -> Result<usize, SdkRefusal> {
    if length > max {
        return Err(refused("TooLarge", "a worker frame is larger than allowed"));
    }
    Ok(length)
}

/// One frame body of `length` bytes, as its value.
fn read_body(input: &mut impl Read, length: usize) -> Result<SdkValue, SdkRefusal> {
    let mut text = vec![0u8; length];
    input.read_exact(&mut text).map_err(connection)?;
    let text =
        String::from_utf8(text).map_err(|_| refused("Failed", "a worker frame is not UTF-8"))?;
    json::parse(&text).map_err(|error| refused("Failed", format!("a worker frame: {}", error.0)))
}

/// The slowest a frame body may arrive and not be a trickle: a body gets
/// its length at this rate in addition to the wait after its length, so a
/// 1 MiB body gets 16 s and a 64 KiB one gets the wait alone.
const MIN_FRAME_RATE: usize = 64 << 10;

/// Reads one frame of at most `max` bytes from `stream`: every read and the
/// 4-byte length must arrive within `wait`, and the whole body within
/// `wait` or its length at [`MIN_FRAME_RATE`], whichever is longer. So a
/// peer that sends a byte at a time cannot stretch a frame, while a large
/// body that keeps arriving is not cut off.
pub fn read_frame_within_wait(
    stream: &mut TcpStream,
    max: usize,
    wait: Duration,
) -> Result<Option<SdkValue>, SdkRefusal> {
    let length = {
        let mut prefix = Bounded {
            stream,
            deadline: Instant::now() + wait,
            wait,
        };
        read_length(&mut prefix)?
    };
    let Some(length) = length else {
        return Ok(None);
    };
    let length = length_within(length, max)?;
    let whole = wait.max(at_rate(length));
    let mut body = Bounded {
        stream,
        deadline: Instant::now() + whole,
        wait,
    };
    read_body(&mut body, length).map(Some)
}

/// How long `length` bytes take at [`MIN_FRAME_RATE`].
fn at_rate(length: usize) -> Duration {
    Duration::from_micros((length as u64).saturating_mul(1_000_000) / MIN_FRAME_RATE as u64)
}

/// `stream` with one deadline for the whole frame, and no read longer than
/// `wait`: each read makes progress, and the whole frame is still bounded.
struct Bounded<'a> {
    stream: &'a mut TcpStream,
    deadline: Instant,
    wait: Duration,
}

impl Read for Bounded<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let left = self.deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(no_frame());
        }
        self.stream.set_read_timeout(Some(left.min(self.wait)))?;
        self.stream.read(buf)
    }
}

fn no_frame() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::TimedOut, "no frame within the wait")
}

/// The message of a refusal that says no frame arrived within its wait.
const NO_FRAME: &str = "the connection sent no frame within the wait";

/// Whether `refusal` is a wait that expired (no frame arrived), as opposed
/// to a connection that closed or broke.
pub(crate) fn is_liveness_timeout(refusal: &SdkRefusal) -> bool {
    refusal.code == "Unavailable" && refusal.message == NO_FRAME
}

/// A connection that stopped, or sent nothing in time: `Unavailable`, never
/// a hang.
fn connection(error: std::io::Error) -> SdkRefusal {
    if matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    ) {
        refused("Unavailable", NO_FRAME)
    } else {
        refused("Unavailable", format!("worker connection: {error}"))
    }
}

/// Every byte value as `{"base64": ...}`, even UTF-8 bytes.
fn typed(value: SdkValue) -> SdkValue {
    match value {
        SdkValue::Bytes(bytes) => SdkValue::map([("base64", SdkValue::Text(json::base64(&bytes)))]),
        SdkValue::List(items) => SdkValue::List(items.into_iter().map(typed).collect()),
        SdkValue::Map(fields) => {
            SdkValue::Map(fields.into_iter().map(|(n, v)| (n, typed(v))).collect())
        }
        other => other,
    }
}
