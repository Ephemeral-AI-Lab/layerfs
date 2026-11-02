//! One frame: a 4-byte big-endian length, then that many bytes of JSON.
//! Bytes travel as `{"base64": ...}` so a value keeps its type.
use crate::common::json;
use crate::common::outcome::refused;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::io::{Read, Write};

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
    let mut length = [0u8; 4];
    match input.read(&mut length[..1]) {
        Ok(0) => return Ok(None),
        Ok(_) => {}
        Err(error) => {
            return Err(refused(
                "Unavailable",
                format!("worker connection: {error}"),
            ))
        }
    }
    input
        .read_exact(&mut length[1..])
        .map_err(|error| refused("Unavailable", format!("worker connection: {error}")))?;
    let length = u32::from_be_bytes(length) as usize;
    if length > max {
        return Err(refused("TooLarge", "a worker frame is larger than allowed"));
    }
    let mut text = vec![0u8; length];
    input
        .read_exact(&mut text)
        .map_err(|error| refused("Unavailable", format!("worker connection: {error}")))?;
    let text =
        String::from_utf8(text).map_err(|_| refused("Failed", "a worker frame is not UTF-8"))?;
    json::parse(&text)
        .map(Some)
        .map_err(|error| refused("Failed", format!("a worker frame: {}", error.0)))
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
