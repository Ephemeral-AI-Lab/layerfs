//! One frame: a 4-byte big-endian length, then that many bytes of JSON.
//! Bytes travel as `{"base64": ...}` so a value keeps its type.
use crate::calls::json;
use crate::host::outcome::refused;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::io::{Read, Write};

/// The largest frame either side accepts.
pub const MAX_FRAME: usize = 512 << 20;

/// Writes `value` as one frame.
pub fn write_frame(out: &mut impl Write, value: &SdkValue) -> std::io::Result<()> {
    let text = json::print(&typed(value.clone()));
    if text.len() > MAX_FRAME {
        return Err(std::io::Error::other(
            "frame larger than the worker accepts",
        ));
    }
    out.write_all(&(text.len() as u32).to_be_bytes())?;
    out.write_all(text.as_bytes())?;
    out.flush()
}

/// Reads one frame; `Ok(None)` at a clean end before any byte.
pub fn read_frame(input: &mut impl Read) -> Result<Option<SdkValue>, SdkRefusal> {
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
    if length > MAX_FRAME {
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
